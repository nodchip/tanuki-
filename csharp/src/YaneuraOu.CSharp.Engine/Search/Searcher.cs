using System.Collections.Generic;
using System.Diagnostics;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Board;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;
using YaneuraOu.CSharp.Engine.Search.Time;

namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 最小構成の1スレッド探索器。
/// </summary>
public sealed class Searcher
{
    private const int MateScore = 100_000;
    private const int NullMoveDepthThreshold = 3;
    private const int NullMoveReductionBase = 2;
    private const int LmrDepthThreshold = 3;
    private const int LmrLateMoveIndex = 4;
    private const int AspirationInitialWindow = 64;

    private readonly IEvaluator evaluator;
    private readonly IIncrementalEvaluator? incrementalEvaluator;
    private readonly SearchFeatures features;
    private readonly MoveOrderingContext orderingContext = new();
    private readonly Dictionary<ulong, TranspositionEntry> transpositionTable = new();
    private readonly Stack<StateInfo> statePool = new();

    private Stopwatch? timer;
    private int maxTimeMs;
    private long maxNodes;
    private long totalNodes;
    private Func<bool>? shouldStopCallback;
    private SearchStopPolicy? stopPolicy;

    /// <summary>
    /// Searcherのインスタンスを初期化する。
    /// </summary>
    public Searcher(IEvaluator? evaluator = null, SearchFeatures? features = null)
    {
        this.evaluator = evaluator ?? new MaterialEvaluator();
        this.features = features ?? new SearchFeatures();
        incrementalEvaluator = this.evaluator as IIncrementalEvaluator;
    }

    /// <summary>
    /// 直近探索でのTTヒット件数を返す。
    /// </summary>
    public int LastTranspositionHitCount { get; private set; }

    /// <summary>
    /// 直近探索でのNull Move枝刈り発生件数を返す。
    /// </summary>
    public int LastNullMovePruningCount { get; private set; }

    /// <summary>
    /// 直近探索でのLMR適用件数を返す。
    /// </summary>
    public int LastLmrReductionCount { get; private set; }

    /// <summary>
    /// 直近探索でのAspiration再探索回数を返す。
    /// </summary>
    public int LastAspirationReSearchCount { get; private set; }

    /// <summary>
    /// 探索を実行して最善手を返す。
    /// </summary>
    public SearchResult Search(Position position, SearchLimits limits, Action<SearchProgress>? progress = null)
    {
        int depth = limits.Depth <= 0 ? 1 : limits.Depth;

        LastTranspositionHitCount = 0;
        LastNullMovePruningCount = 0;
        LastLmrReductionCount = 0;
        LastAspirationReSearchCount = 0;
        maxTimeMs = limits.MaxTimeMs;
        maxNodes = limits.NodesLimit;
        shouldStopCallback = limits.ShouldStop;
        stopPolicy = limits.StopPolicy;
        totalNodes = 0;
        timer = Stopwatch.StartNew();
        incrementalEvaluator?.ResetIncrementalState(position);

        if (limits.MateMoves > 0)
        {
            SearchResult mateResult = SearchMate(position, limits.MateMoves, progress);
            timer.Stop();
            return mateResult;
        }

        Move bestMove = Move.none();
        int bestScore = int.MinValue;
        int nodes = 0;
        int completedDepth = 0;

        for (int d = 1; d <= depth; d++)
        {
            if (ShouldStopNow())
            {
                break;
            }

            Move currentBest;
            int currentScore;
            int currentNodes;
            if (!features.EnableAspirationWindow || d == 1 || bestScore == int.MinValue)
            {
                (currentBest, currentScore, currentNodes) = SearchRoot(position, d, int.MinValue + 1, int.MaxValue - 1);
            }
            else
            {
                int window = AspirationInitialWindow;
                int alpha = Math.Max(int.MinValue + 1, bestScore - window);
                int beta = Math.Min(int.MaxValue - 1, bestScore + window);
                currentBest = Move.none();
                currentScore = int.MinValue;
                currentNodes = 0;

                while (true)
                {
                    (Move attemptBest, int attemptScore, int attemptNodes) = SearchRoot(position, d, alpha, beta);
                    currentBest = attemptBest;
                    currentScore = attemptScore;
                    currentNodes += attemptNodes;

                    if (ShouldStopNow())
                    {
                        break;
                    }

                    if (currentScore <= alpha)
                    {
                        LastAspirationReSearchCount++;
                        window = Math.Min(window * 2, 32000);
                        alpha = Math.Max(int.MinValue + 1, bestScore - window);
                        continue;
                    }

                    if (currentScore >= beta)
                    {
                        LastAspirationReSearchCount++;
                        window = Math.Min(window * 2, 32000);
                        beta = Math.Min(int.MaxValue - 1, bestScore + window);
                        continue;
                    }

                    break;
                }
            }
            nodes += currentNodes;
            bestScore = currentScore;
            if (currentBest.to_u32() != Move.none().to_u32())
            {
                bestMove = currentBest;
            }

            completedDepth = d;
            progress?.Invoke(new SearchProgress(
                d,
                d,
                bestScore,
                nodes,
                stopPolicy?.ElapsedMilliseconds ?? (int)timer.ElapsedMilliseconds,
                0,
                bestMove,
                bestMove.to_u32() == Move.none().to_u32() ? [] : [bestMove]));

            if (ShouldStopAfterCompletedDepth())
            {
                break;
            }
        }

        timer.Stop();
        int resultDepth = completedDepth > 0 ? completedDepth : 1;
        return new SearchResult(bestMove, bestScore, nodes, resultDepth);
    }

    /// <summary>
    /// go mate向けの詰み探索を実行する。
    /// </summary>
    private SearchResult SearchMate(Position position, int mateMoves, Action<SearchProgress>? progress)
    {
        int maxPly = Math.Max(1, mateMoves * 2 - 1);
        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            int terminalScore = position.in_check() ? -MateScore : 0;
            progress?.Invoke(new SearchProgress(1, 1, terminalScore, 1, (int)timer!.ElapsedMilliseconds, 0, Move.none(), []));
            return new SearchResult(Move.none(), terminalScore, 1, 1);
        }

        int nodes = 0;
        int bestScore = int.MinValue;
        Move bestMove = legal[0];
        Color us = position.side_to_move();
        foreach (Move move in legal)
        {
            if (ShouldStopNow())
            {
                break;
            }

            StateInfo st = RentStateInfo();
            int score;
            try
            {
                position.do_move(move, st, position.gives_check(move));
                incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
                score = -MateSearch(position, maxPly - 1, 1, ref nodes);
                position.undo_move(move);
                incrementalEvaluator?.OnMoveUndone(position, move);
            }
            finally
            {
                ReturnStateInfo(st);
            }

            if (score > bestScore)
            {
                bestScore = score;
                bestMove = move;
            }
        }

        int elapsedMs = stopPolicy?.ElapsedMilliseconds ?? (int)timer!.ElapsedMilliseconds;
        progress?.Invoke(new SearchProgress(1, 1, bestScore, nodes, elapsedMs, 0, bestMove, [bestMove]));
        return new SearchResult(bestMove, bestScore, nodes, 1);
    }

    /// <summary>
    /// 手数制限付きの詰み探索を実行する。
    /// </summary>
    private int MateSearch(Position position, int remainingPly, int plyFromRoot, ref int nodes)
    {
        if (ShouldStopNow())
        {
            return 0;
        }

        nodes++;
        totalNodes++;

        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            return position.in_check() ? -MateScore + plyFromRoot : 0;
        }

        if (remainingPly <= 0)
        {
            return 0;
        }

        int best = int.MinValue;
        Color us = position.side_to_move();
        foreach (Move move in legal)
        {
            if (ShouldStopNow())
            {
                break;
            }

            StateInfo st = RentStateInfo();
            int score;
            try
            {
                position.do_move(move, st, position.gives_check(move));
                incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
                score = -MateSearch(position, remainingPly - 1, plyFromRoot + 1, ref nodes);
                position.undo_move(move);
                incrementalEvaluator?.OnMoveUndone(position, move);
            }
            finally
            {
                ReturnStateInfo(st);
            }

            if (score > best)
            {
                best = score;
            }
        }

        return best == int.MinValue ? 0 : best;
    }

    /// <summary>
    /// ルート探索を実行する。
    /// </summary>
    private (Move BestMove, int Score, int Nodes) SearchRoot(Position position, int depth, int alpha, int beta)
    {
        Color us = position.side_to_move();
        Color them = us == Color.BLACK ? Color.WHITE : Color.BLACK;
        if (position.king_square(us) == Square.SQ_NB)
        {
            return (Move.none(), -MateScore, 1);
        }

        if (position.king_square(them) == Square.SQ_NB)
        {
            return (Move.none(), MateScore, 1);
        }

        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            return (Move.none(), position.in_check() ? -MateScore : 0, 1);
        }

        ulong rootKey = position.state().key().ToUInt64();
        Move ttMove = TryGetTranspositionMove(rootKey, depth, out Move savedMove) ? savedMove : Move.none();

        Move bestMove = legal[0];
        int bestScore = int.MinValue;
        int originalAlpha = alpha;
        int nodes = 0;

        foreach (Move move in MoveOrdering.Order(position, legal, orderingContext, 0, ttMove))
        {
            if (ShouldStopNow())
            {
                break;
            }

            StateInfo st = RentStateInfo();
            int score;
            try
            {
                position.do_move(move, st, position.gives_check(move));
                incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
                score = -AlphaBeta(position, depth - 1, -beta, -alpha, 1, ref nodes);
                position.undo_move(move);
                incrementalEvaluator?.OnMoveUndone(position, move);
            }
            finally
            {
                ReturnStateInfo(st);
            }

            if (score > bestScore)
            {
                bestScore = score;
                bestMove = move;
            }

            if (score > alpha)
            {
                alpha = score;
            }

            if (alpha >= beta)
            {
                break;
            }
        }

        int finalScore = bestScore == int.MinValue ? originalAlpha : bestScore;
        TranspositionBound bound = finalScore <= originalAlpha ? TranspositionBound.Upper
            : finalScore >= beta ? TranspositionBound.Lower
            : TranspositionBound.Exact;
        StoreTransposition(rootKey, depth, finalScore, bestMove, bound);
        return (bestMove, finalScore, nodes);
    }

    /// <summary>
    /// alpha-beta探索を実行する。
    /// </summary>
    private int AlphaBeta(Position position, int depth, int alpha, int beta, int ply, ref int nodes)
    {
        if (ShouldStopNow())
        {
            return alpha;
        }

        nodes++;
        totalNodes++;

        int terminal = EvaluateTerminal(position, depth);
        if (terminal != int.MinValue)
        {
            return terminal;
        }

        ulong key = position.state().key().ToUInt64();
        int originalAlpha = alpha;
        if (TryProbeTransposition(key, depth, alpha, beta, out int ttScore, out Move ttMove))
        {
            LastTranspositionHitCount++;
            return ttScore;
        }

        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            int noLegal = position.in_check() ? -MateScore + depth : 0;
            StoreTransposition(key, depth, noLegal, Move.none(), TranspositionBound.Exact);
            return noLegal;
        }

        if (depth <= 0)
        {
            int q = Quiescence(position, alpha, beta, ref nodes);
            StoreTransposition(key, 0, q, Move.none(), TranspositionBound.Exact);
            return q;
        }

        if (features.EnableNullMovePruning && CanTryNullMove(position, depth, beta))
        {
            StateInfo nullState = RentStateInfo();
            int nullScore;
            try
            {
                position.do_null_move(nullState);
                incrementalEvaluator?.ResetIncrementalState(position);

                int reduction = NullMoveReductionBase + depth / 4;
                int nullDepth = Math.Max(0, depth - 1 - reduction);
                nullScore = -AlphaBeta(position, nullDepth, -beta, -beta + 1, ply + 1, ref nodes);

                position.undo_null_move();
                incrementalEvaluator?.ResetIncrementalState(position);
            }
            finally
            {
                ReturnStateInfo(nullState);
            }

            if (nullScore >= beta)
            {
                LastNullMovePruningCount++;
                StoreTransposition(key, depth, nullScore, Move.none(), TranspositionBound.Lower);
                return beta;
            }
        }

        int best = alpha;
        Move bestMove = Move.none();
        Color us = position.side_to_move();
        bool inCheck = position.in_check();
        int moveIndex = 0;

        foreach (Move move in MoveOrdering.Order(position, legal, orderingContext, ply, ttMove))
        {
            moveIndex++;
            if (ShouldStopNow())
            {
                break;
            }

            StateInfo st = RentStateInfo();
            int score;
            try
            {
                position.do_move(move, st, position.gives_check(move));
                incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
                bool applyLmr = features.EnableLmr && CanApplyLmr(depth, moveIndex, inCheck, move, st.capturedPiece);
                if (applyLmr)
                {
                    LastLmrReductionCount++;
                    int reducedDepth = Math.Max(1, depth - 1 - CalculateLmrReduction(depth));
                    score = -AlphaBeta(position, reducedDepth, -best - 1, -best, ply + 1, ref nodes);
                    if (score > best)
                    {
                        score = -AlphaBeta(position, depth - 1, -beta, -best, ply + 1, ref nodes);
                    }
                }
                else
                {
                    score = -AlphaBeta(position, depth - 1, -beta, -best, ply + 1, ref nodes);
                }
                position.undo_move(move);
                incrementalEvaluator?.OnMoveUndone(position, move);
            }
            finally
            {
                ReturnStateInfo(st);
            }

            if (score >= beta)
            {
                orderingContext.RegisterKiller(ply, move);
                if (!move.is_drop())
                {
                    orderingContext.AddHistory(move, depth);
                }

                StoreTransposition(key, depth, score, move, TranspositionBound.Lower);
                return beta;
            }

            if (score > best)
            {
                best = score;
                bestMove = move;
            }
        }

        if (bestMove.to_u32() != Move.none().to_u32() && !bestMove.is_drop())
        {
            orderingContext.AddHistory(bestMove, depth);
        }

        TranspositionBound bound = best <= originalAlpha ? TranspositionBound.Upper : TranspositionBound.Exact;
        StoreTransposition(key, depth, best, bestMove, bound);
        return best;
    }

    /// <summary>
    /// 早期終局判定を行い、非終局ならint.MinValueを返す。
    /// </summary>
    private int EvaluateTerminal(Position position, int depth)
    {
        Color us = position.side_to_move();
        Color them = us == Color.BLACK ? Color.WHITE : Color.BLACK;

        if (position.king_square(us) == Square.SQ_NB)
        {
            return -MateScore + depth;
        }

        if (position.king_square(them) == Square.SQ_NB)
        {
            return MateScore - depth;
        }

        return int.MinValue;
    }

    /// <summary>
    /// 静止探索を実行する。
    /// </summary>
    private int Quiescence(Position position, int alpha, int beta, ref int nodes)
    {
        if (ShouldStopNow())
        {
            return alpha;
        }

        nodes++;
        totalNodes++;
        bool inCheck = position.in_check();
        if (!inCheck)
        {
            int standPat = evaluator.Evaluate(position);
            if (standPat >= beta)
            {
                return beta;
            }

            if (standPat > alpha)
            {
                alpha = standPat;
            }
        }

        MoveList moves = inCheck
            ? MoveGenerator.GenerateLegal(position)
            : MoveGenerator.GenerateCaptures(position);
        if (moves.Count == 0)
        {
            return inCheck ? -MateScore + 1 : alpha;
        }

        Color us = position.side_to_move();
        foreach (Move move in MoveOrdering.Order(position, moves, orderingContext))
        {
            if (ShouldStopNow())
            {
                break;
            }

            StateInfo st = RentStateInfo();
            int score;
            try
            {
                position.do_move(move, st, position.gives_check(move));
                incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
                score = -Quiescence(position, -beta, -alpha, ref nodes);
                position.undo_move(move);
                incrementalEvaluator?.OnMoveUndone(position, move);
            }
            finally
            {
                ReturnStateInfo(st);
            }

            if (score >= beta)
            {
                return beta;
            }

            if (score > alpha)
            {
                alpha = score;
            }
        }

        return alpha;
    }

    /// <summary>
    /// Null Move枝刈りを試行可能かを判定する。
    /// </summary>
    private static bool CanTryNullMove(Position position, int depth, int beta)
    {
        if (depth < NullMoveDepthThreshold)
        {
            return false;
        }

        if (position.in_check())
        {
            return false;
        }

        return Math.Abs(beta) < MateScore - 1024;
    }

    /// <summary>
    /// LMRを適用可能かを判定する。
    /// </summary>
    private static bool CanApplyLmr(int depth, int moveIndex, bool inCheck, Move move, Piece capturedPiece)
    {
        if (depth < LmrDepthThreshold || inCheck || moveIndex < LmrLateMoveIndex)
        {
            return false;
        }

        if (capturedPiece != Piece.NO_PIECE)
        {
            return false;
        }

        if (move.is_drop() || move.is_promote())
        {
            return false;
        }

        return true;
    }

    /// <summary>
    /// 深さに応じたLMR削減量を返す。
    /// </summary>
    private static int CalculateLmrReduction(int depth)
    {
        return Math.Max(1, depth / 3);
    }

    /// <summary>
    /// depth完了時点で停止すべきかどうかを判定する。
    /// </summary>
    private bool ShouldStopAfterCompletedDepth()
    {
        if (stopPolicy is not null)
        {
            return stopPolicy.ShouldStopAfterCompletedDepth(totalNodes);
        }

        return ShouldStopNow();
    }

    /// <summary>
    /// 探索を即停止すべきかどうかを判定する。
    /// </summary>
    private bool ShouldStopNow()
    {
        if (stopPolicy is not null)
        {
            return stopPolicy.ShouldStopNow(totalNodes);
        }

        if (shouldStopCallback is not null && shouldStopCallback())
        {
            return true;
        }

        if (maxNodes > 0 && totalNodes >= maxNodes)
        {
            return true;
        }

        return maxTimeMs > 0 && timer is not null && timer.ElapsedMilliseconds >= maxTimeMs;
    }

    /// <summary>
    /// 置換表を参照する。
    /// </summary>
    private bool TryGetTranspositionMove(ulong key, int depth, out Move bestMove)
    {
        if (transpositionTable.TryGetValue(key, out TranspositionEntry entry) && entry.Depth >= depth)
        {
            bestMove = entry.BestMove;
            return true;
        }

        bestMove = Move.none();
        return false;
    }

    /// <summary>
    /// 置換表を参照する。
    /// </summary>
    private bool TryProbeTransposition(ulong key, int depth, int alpha, int beta, out int score, out Move bestMove)
    {
        if (transpositionTable.TryGetValue(key, out TranspositionEntry entry)
            && TranspositionPolicy.TryResolve(
                entry.Depth,
                depth,
                entry.Score,
                entry.Bound,
                alpha,
                beta,
                entry.BestMove,
                out score,
                out bestMove))
        {
            return true;
        }

        score = 0;
        bestMove = Move.none();
        return false;
    }

    /// <summary>
    /// 置換表へ結果を保存する。
    /// </summary>
    private void StoreTransposition(ulong key, int depth, int score, Move bestMove, TranspositionBound bound)
    {
        if (transpositionTable.TryGetValue(key, out TranspositionEntry current) && current.Depth > depth)
        {
            return;
        }

        transpositionTable[key] = new TranspositionEntry(depth, score, bestMove, bound);
    }

    /// <summary>
    /// 置換表エントリを表す値型。
    /// </summary>
    private readonly record struct TranspositionEntry(int Depth, int Score, Move BestMove, TranspositionBound Bound);

    /// <summary>
    /// StateInfoをプールから取得する。
    /// </summary>
    private StateInfo RentStateInfo()
    {
        if (statePool.Count > 0)
        {
            return statePool.Pop();
        }

        return new StateInfo();
    }

    /// <summary>
    /// StateInfoを初期化してプールへ返却する。
    /// </summary>
    private void ReturnStateInfo(StateInfo state)
    {
        state.previous = null;
        state.capturedPiece = Piece.NO_PIECE;
        state.repetition = 0;
        state.repetition_times = 0;
        state.repetition_type = 0;
        state.pliesFromNull = 0;
        state.continuousCheck[(int)Color.BLACK] = 0;
        state.continuousCheck[(int)Color.WHITE] = 0;
        state.checkersBB = new Bitboard(0);
        state.blockersForKing[(int)Color.BLACK] = new Bitboard(0);
        state.blockersForKing[(int)Color.WHITE] = new Bitboard(0);
        state.pinners[(int)Color.BLACK] = new Bitboard(0);
        state.pinners[(int)Color.WHITE] = new Bitboard(0);
        statePool.Push(state);
    }
}

/// <summary>
/// 探索進捗を表す値オブジェクト。
/// </summary>
public readonly record struct SearchProgress(
    int Depth,
    int SelDepth,
    int Score,
    int Nodes,
    int ElapsedMilliseconds,
    int HashFullPermill,
    Move CurrentMove,
    Move[] PrincipalVariation);

/// <summary>
/// 探索結果を表す値オブジェクト。
/// </summary>
public readonly record struct SearchResult(Move BestMove, int Score, int Nodes, int Depth);


