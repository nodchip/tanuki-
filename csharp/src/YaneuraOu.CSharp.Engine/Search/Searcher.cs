using System.Collections.Generic;
using System.Diagnostics;
using YaneuraOu.CSharp.Engine.Core;
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

    private readonly IEvaluator evaluator;
    private readonly IIncrementalEvaluator? incrementalEvaluator;
    private readonly MoveOrderingContext orderingContext = new();
    private readonly Dictionary<ulong, TranspositionEntry> transpositionTable = new();

    private Stopwatch? timer;
    private int maxTimeMs;
    private long maxNodes;
    private long totalNodes;
    private Func<bool>? shouldStopCallback;
    private SearchStopPolicy? stopPolicy;

    /// <summary>
    /// Searcherのインスタンスを初期化する。
    /// </summary>
    public Searcher(IEvaluator? evaluator = null)
    {
        this.evaluator = evaluator ?? new MaterialEvaluator();
        incrementalEvaluator = this.evaluator as IIncrementalEvaluator;
    }

    /// <summary>
    /// 直近探索でのTTヒット件数を返す。
    /// </summary>
    public int LastTranspositionHitCount { get; private set; }

    /// <summary>
    /// 探索を実行して最善手を返す。
    /// </summary>
    public SearchResult Search(Position position, SearchLimits limits, Action<SearchProgress>? progress = null)
    {
        int depth = limits.Depth <= 0 ? 1 : limits.Depth;

        LastTranspositionHitCount = 0;
        maxTimeMs = limits.MaxTimeMs;
        maxNodes = limits.NodesLimit;
        shouldStopCallback = limits.ShouldStop;
        stopPolicy = limits.StopPolicy;
        totalNodes = 0;
        timer = Stopwatch.StartNew();
        incrementalEvaluator?.ResetIncrementalState(position);

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

            (Move currentBest, int currentScore, int currentNodes) = SearchRoot(position, d);
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
    /// ルート探索を実行する。
    /// </summary>
    private (Move BestMove, int Score, int Nodes) SearchRoot(Position position, int depth)
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
        int nodes = 0;

        foreach (Move move in MoveOrdering.Order(position, legal, orderingContext, 0, ttMove))
        {
            if (ShouldStopNow())
            {
                break;
            }

            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
            int score = -AlphaBeta(position, depth - 1, int.MinValue + 1, int.MaxValue - 1, 1, ref nodes);
            position.undo_move(move);
            incrementalEvaluator?.OnMoveUndone(position, move);

            if (score > bestScore)
            {
                bestScore = score;
                bestMove = move;
            }
        }

        StoreTransposition(rootKey, depth, bestScore, bestMove, TranspositionBound.Exact);
        return (bestMove, bestScore, nodes);
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

        int best = alpha;
        Move bestMove = Move.none();
        Color us = position.side_to_move();

        foreach (Move move in MoveOrdering.Order(position, legal, orderingContext, ply, ttMove))
        {
            if (ShouldStopNow())
            {
                break;
            }

            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
            int score = -AlphaBeta(position, depth - 1, -beta, -best, ply + 1, ref nodes);
            position.undo_move(move);
            incrementalEvaluator?.OnMoveUndone(position, move);

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
        int standPat = evaluator.Evaluate(position);
        if (standPat >= beta)
        {
            return beta;
        }

        if (standPat > alpha)
        {
            alpha = standPat;
        }

        MoveList captures = MoveGenerator.GenerateCaptures(position);
        Color us = position.side_to_move();
        foreach (Move move in MoveOrdering.Order(position, captures, orderingContext))
        {
            if (ShouldStopNow())
            {
                break;
            }

            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            incrementalEvaluator?.OnMoveApplied(position, move, st.capturedPiece, us);
            int score = -Quiescence(position, -beta, -alpha, ref nodes);
            position.undo_move(move);
            incrementalEvaluator?.OnMoveUndone(position, move);

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
