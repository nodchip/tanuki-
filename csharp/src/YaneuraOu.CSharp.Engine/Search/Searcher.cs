using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 最小構成の1スレッド探索器。
/// </summary>
public sealed class Searcher
{
    private const int MateScore = 100_000;

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
    /// ルート探索を行う。
    /// </summary>
    private static (Move BestMove, int Score, int Nodes) SearchRoot(Position position, int depth)
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
    /// alpha-beta 探索を行う。
    /// </summary>
    private static int AlphaBeta(Position position, int depth, int alpha, int beta, ref int nodes)
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
    /// 簡易 quiescence 探索を行う。
    /// </summary>
    private static int Quiescence(Position position, int alpha, int beta, ref int nodes)
    {
        nodes++;
        int standPat = Evaluate(position);
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
    /// 材料差ベースの簡易評価値を返す。
    /// </summary>
    private static int Evaluate(Position position)
    {
        int score = 0;
        for (int sq = 0; sq < (int)Square.SQ_NB; sq++)
        {
            Piece pc = position.piece_on((Square)sq);
            if (pc == Piece.NO_PIECE)
            {
                continue;
            }

            int value = PieceValue(ShogiTypes.raw_type_of(pc));
            score += ShogiTypes.color_of(pc) == Color.BLACK ? value : -value;
        }

        score += EvaluateHand(position, Color.BLACK);
        score -= EvaluateHand(position, Color.WHITE);

        return position.side_to_move() == Color.BLACK ? score : -score;
    }

    /// <summary>
    /// 手駒の評価値を返す。
    /// </summary>
    private static int EvaluateHand(Position position, Color side)
    {
        uint hand = position.hand_of(side);
        int score = 0;
        for (int pt = (int)PieceType.PAWN; pt < (int)PieceType.PIECE_HAND_NB; pt++)
        {
            int count = ShogiTypes.hand_count(hand, (PieceType)pt);
            score += PieceValue((PieceType)pt) * count;
        }

        return score;
    }

    /// <summary>
    /// 駒価値を返す。
    /// </summary>
    private static int PieceValue(PieceType pt)
    {
        return pt switch
        {
            PieceType.PAWN => 100,
            PieceType.LANCE => 300,
            PieceType.KNIGHT => 300,
            PieceType.SILVER => 400,
            PieceType.GOLD => 500,
            PieceType.BISHOP => 800,
            PieceType.ROOK => 1000,
            PieceType.PRO_PAWN => 500,
            PieceType.PRO_LANCE => 500,
            PieceType.PRO_KNIGHT => 500,
            PieceType.PRO_SILVER => 500,
            PieceType.HORSE => 900,
            PieceType.DRAGON => 1100,
            _ => 0,
        };
    }
}

/// <summary>
/// 探索結果を表す値オブジェクト。
/// </summary>
public readonly record struct SearchResult(Move BestMove, int Score, int Nodes, int Depth);
