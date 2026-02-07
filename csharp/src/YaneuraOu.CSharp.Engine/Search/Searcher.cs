using System.Collections.Generic;
using System.Diagnostics;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 最小構成の1スレッド探索器。
/// </summary>
public sealed class Searcher
{
    private const int MateScore = 100_000;

    private readonly IEvaluator evaluator;
    private readonly MoveOrderingContext orderingContext = new();
    private readonly Dictionary<ulong, TranspositionEntry> transpositionTable = new();

    private Stopwatch? timer;
    private int maxTimeMs;

    /// <summary>
    /// Searcherのインスタンスを初期化する。
    /// </summary>
    public Searcher(IEvaluator? evaluator = null)
    {
        this.evaluator = evaluator ?? new MaterialEvaluator();
    }

    /// <summary>
    /// 直近探索でのTTヒット件数を返す。
    /// </summary>
    public int LastTranspositionHitCount { get; private set; }

    /// <summary>
    /// 探索を実行して最善手を返す。
    /// </summary>
    public SearchResult Search(Position position, SearchLimits limits)
    {
        int depth = limits.Depth <= 0 ? 1 : limits.Depth;

        LastTranspositionHitCount = 0;
        maxTimeMs = limits.MaxTimeMs;
        timer = Stopwatch.StartNew();

        Move bestMove = Move.none();
        int bestScore = int.MinValue;
        int nodes = 0;
        int completedDepth = 0;

        for (int d = 1; d <= depth; d++)
        {
            if (IsTimeUp())
            {
                break;
            }

            (Move currentBest, int currentScore, int currentNodes) = SearchRoot(position, d);
            nodes += currentNodes;
            if (currentBest.to_u32() != Move.none().to_u32())
            {
                bestMove = currentBest;
                bestScore = currentScore;
            }

            completedDepth = d;
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
        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            return (Move.none(), position.in_check() ? -MateScore : 0, 1);
        }

        ulong rootKey = position.state().key().ToUInt64();
        Move ttMove = TryProbeTransposition(rootKey, depth, out _, out Move savedMove) ? savedMove : Move.none();

        Move bestMove = legal[0];
        int bestScore = int.MinValue;
        int nodes = 0;

        foreach (Move move in MoveOrdering.Order(position, legal, orderingContext, 0, ttMove))
        {
            if (IsTimeUp())
            {
                break;
            }

            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            int score = -AlphaBeta(position, depth - 1, int.MinValue + 1, int.MaxValue - 1, 1, ref nodes);
            position.undo_move(move);

            if (score > bestScore)
            {
                bestScore = score;
                bestMove = move;
            }
        }

        StoreTransposition(rootKey, depth, bestScore, bestMove);
        return (bestMove, bestScore, nodes);
    }

    /// <summary>
    /// alpha-beta探索を実行する。
    /// </summary>
    private int AlphaBeta(Position position, int depth, int alpha, int beta, int ply, ref int nodes)
    {
        if (IsTimeUp())
        {
            return alpha;
        }

        nodes++;

        int terminal = EvaluateTerminal(position, depth);
        if (terminal != int.MinValue)
        {
            return terminal;
        }

        ulong key = position.state().key().ToUInt64();
        if (TryProbeTransposition(key, depth, out int ttScore, out Move ttMove))
        {
            LastTranspositionHitCount++;
            return ttScore;
        }

        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            int noLegal = position.in_check() ? -MateScore + depth : 0;
            StoreTransposition(key, depth, noLegal, Move.none());
            return noLegal;
        }

        if (depth <= 0)
        {
            int q = Quiescence(position, alpha, beta, ref nodes);
            StoreTransposition(key, 0, q, Move.none());
            return q;
        }

        int best = alpha;
        Move bestMove = Move.none();

        foreach (Move move in MoveOrdering.Order(position, legal, orderingContext, ply, ttMove))
        {
            if (IsTimeUp())
            {
                break;
            }

            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            int score = -AlphaBeta(position, depth - 1, -beta, -best, ply + 1, ref nodes);
            position.undo_move(move);

            if (score >= beta)
            {
                orderingContext.RegisterKiller(ply, move);
                if (!move.is_drop())
                {
                    orderingContext.AddHistory(move, depth);
                }

                StoreTransposition(key, depth, beta, move);
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

        StoreTransposition(key, depth, best, bestMove);
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
        if (IsTimeUp())
        {
            return alpha;
        }

        nodes++;
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
        foreach (Move move in MoveOrdering.Order(position, captures, orderingContext))
        {
            if (IsTimeUp())
            {
                break;
            }

            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            int score = -Quiescence(position, -beta, -alpha, ref nodes);
            position.undo_move(move);

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
    /// 時間上限に達したかを判定する。
    /// </summary>
    private bool IsTimeUp()
    {
        return maxTimeMs > 0 && timer is not null && timer.ElapsedMilliseconds >= maxTimeMs;
    }

    /// <summary>
    /// 置換表を参照する。
    /// </summary>
    private bool TryProbeTransposition(ulong key, int depth, out int score, out Move bestMove)
    {
        if (transpositionTable.TryGetValue(key, out TranspositionEntry entry) && entry.Depth >= depth)
        {
            score = entry.Score;
            bestMove = entry.BestMove;
            return true;
        }

        score = 0;
        bestMove = Move.none();
        return false;
    }

    /// <summary>
    /// 置換表へ結果を保存する。
    /// </summary>
    private void StoreTransposition(ulong key, int depth, int score, Move bestMove)
    {
        if (transpositionTable.TryGetValue(key, out TranspositionEntry current) && current.Depth > depth)
        {
            return;
        }

        transpositionTable[key] = new TranspositionEntry(depth, score, bestMove);
    }

    /// <summary>
    /// 置換表エントリを表す値型。
    /// </summary>
    private readonly record struct TranspositionEntry(int Depth, int Score, Move BestMove);
}

/// <summary>
/// 探索結果を表す値オブジェクト。
/// </summary>
public readonly record struct SearchResult(Move BestMove, int Score, int Nodes, int Depth);
