using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// 駒得ベースの暫定評価器。
/// </summary>
public sealed class MaterialEvaluator : IEvaluator
{
    /// <summary>
    /// 指定局面を手番側視点で評価する。
    /// </summary>
    public int Evaluate(Position position)
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
    /// 持ち駒の評価値を算出する。
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
    /// 駒種類ごとの評価値を返す。
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
