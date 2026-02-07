using System.Collections.Generic;
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
    private readonly Dictionary<ulong, TranspositionEntry> transpositionTable = new();

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

        Move bestMove = Move.none();
        int bestScore = int.MinValue;
        int nodes = 0;

        for (int d = 1; d <= depth; d++)
        {
            (Move currentBest, int currentScore, int currentNodes) = SearchRoot(position, d);
            nodes += currentNodes;
            if (currentBest.to_u32() != Move.none().to_u32())
            {
                bestMove = currentBest;
                bestScore = currentScore;
            }
        }

        return new SearchResult(bestMove, bestScore, nodes, depth);
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

        Move bestMove = legal[0];
        int bestScore = int.MinValue;
        int nodes = 0;

        foreach (Move move in MoveOrdering.Order(position, legal))
        {
            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            int score = -AlphaBeta(position, depth - 1, int.MinValue + 1, int.MaxValue - 1, ref nodes);
            position.undo_move(move);

            if (score > bestScore)
            {
                bestScore = score;
                bestMove = move;
            }
        }

        return (bestMove, bestScore, nodes);
    }

    /// <summary>
    /// alpha-beta探索を実行する。
    /// </summary>
    private int AlphaBeta(Position position, int depth, int alpha, int beta, ref int nodes)
    {
        nodes++;

        ulong key = position.state().key().ToUInt64();
        if (TryProbeTransposition(key, depth, out int ttScore))
        {
            LastTranspositionHitCount++;
            return ttScore;
        }

        if (depth <= 0)
        {
            int q = Quiescence(position, alpha, beta, ref nodes);
            StoreTransposition(key, 0, q);
            return q;
        }

        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            int terminal = position.in_check() ? -MateScore + depth : 0;
            StoreTransposition(key, depth, terminal);
            return terminal;
        }

        int best = alpha;
        foreach (Move move in MoveOrdering.Order(position, legal))
        {
            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            int score = -AlphaBeta(position, depth - 1, -beta, -best, ref nodes);
            position.undo_move(move);

            if (score >= beta)
            {
                StoreTransposition(key, depth, beta);
                return beta;
            }

            if (score > best)
            {
                best = score;
            }
        }

        StoreTransposition(key, depth, best);
        return best;
    }

    /// <summary>
    /// 静止探索を実行する。
    /// </summary>
    private int Quiescence(Position position, int alpha, int beta, ref int nodes)
    {
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
        foreach (Move move in MoveOrdering.Order(position, captures))
        {
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
    /// 置換表を参照する。
    /// </summary>
    private bool TryProbeTransposition(ulong key, int depth, out int score)
    {
        if (transpositionTable.TryGetValue(key, out TranspositionEntry entry) && entry.Depth >= depth)
        {
            score = entry.Score;
            return true;
        }

        score = 0;
        return false;
    }

    /// <summary>
    /// 置換表へ結果を保存する。
    /// </summary>
    private void StoreTransposition(ulong key, int depth, int score)
    {
        if (transpositionTable.TryGetValue(key, out TranspositionEntry current) && current.Depth > depth)
        {
            return;
        }

        transpositionTable[key] = new TranspositionEntry(depth, score);
    }

    /// <summary>
    /// 置換表エントリを表す値型。
    /// </summary>
    private readonly record struct TranspositionEntry(int Depth, int Score);
}

/// <summary>
/// 探索結果を表す値オブジェクト。
/// </summary>
public readonly record struct SearchResult(Move BestMove, int Score, int Nodes, int Depth);
