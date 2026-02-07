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

    /// <summary>
    /// Searcherのインスタンスを初期化する。
    /// </summary>
    public Searcher(IEvaluator? evaluator = null)
    {
        this.evaluator = evaluator ?? new MaterialEvaluator();
    }

    /// <summary>
    /// 探索を実行して最善手を返す。
    /// </summary>
    public SearchResult Search(Position position, SearchLimits limits)
    {
        int depth = limits.Depth <= 0 ? 1 : limits.Depth;

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

        if (depth <= 0)
        {
            return Quiescence(position, alpha, beta, ref nodes);
        }

        MoveList legal = MoveGenerator.GenerateLegal(position);
        if (legal.Count == 0)
        {
            return position.in_check() ? -MateScore + depth : 0;
        }

        foreach (Move move in MoveOrdering.Order(position, legal))
        {
            var st = new StateInfo();
            position.do_move(move, st, position.gives_check(move));
            int score = -AlphaBeta(position, depth - 1, -beta, -alpha, ref nodes);
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
}

/// <summary>
/// 探索結果を表す値オブジェクト。
/// </summary>
public readonly record struct SearchResult(Move BestMove, int Score, int Nodes, int Depth);
