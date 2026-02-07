using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// 局面をNNUE向けの特徴量インデックス列へ変換するクラス。
/// </summary>
public sealed class NnueFeatureTransformer
{
    private static readonly PieceType[] HandPieceTypes =
    {
        PieceType.PAWN,
        PieceType.LANCE,
        PieceType.KNIGHT,
        PieceType.SILVER,
        PieceType.GOLD,
        PieceType.BISHOP,
        PieceType.ROOK,
    };

    private static readonly int[] HandMaxCounts = { 18, 4, 4, 4, 4, 2, 2 };

    /// <summary>
    /// 局面から特徴量インデックス配列を生成する。
    /// </summary>
    public int[] Transform(Position position)
    {
        var features = new List<int>(96);
        bool flip = position.side_to_move() == Color.WHITE;

        for (int sqValue = 0; sqValue < (int)Square.SQ_NB; sqValue++)
        {
            Square sq = (Square)sqValue;
            Piece pc = position.piece_on(sq);
            if (pc == Piece.NO_PIECE)
            {
                continue;
            }

            Square normalizedSquare = flip ? ShogiTypes.Flip(sq) : sq;
            Piece normalizedPiece = NormalizePiece(pc, flip);
            features.Add(((int)normalizedPiece * 81) + (int)normalizedSquare);
        }

        int handBase = ((int)Piece.PIECE_NB) * 81;
        for (int colorIndex = 0; colorIndex < 2; colorIndex++)
        {
            Color color = (Color)colorIndex;
            Color normalizedColor = NormalizeColor(color, flip);
            uint hand = position.hand_of(color);
            for (int handTypeIndex = 0; handTypeIndex < HandPieceTypes.Length; handTypeIndex++)
            {
                PieceType pt = HandPieceTypes[handTypeIndex];
                int count = ShogiTypes.hand_count(hand, pt);
                int capped = Math.Min(count, HandMaxCounts[handTypeIndex]);
                for (int i = 0; i < capped; i++)
                {
                    int offset = (((int)normalizedColor * HandPieceTypes.Length + handTypeIndex) * 18) + i;
                    features.Add(handBase + offset);
                }
            }
        }

        features.Sort();
        return features.ToArray();
    }

    /// <summary>
    /// 手番基準へ駒の色を正規化する。
    /// </summary>
    private static Piece NormalizePiece(Piece pc, bool flip)
    {
        if (!flip)
        {
            return pc;
        }

        Color swapped = ShogiTypes.color_of(pc) == Color.BLACK ? Color.WHITE : Color.BLACK;
        PieceType pt = ShogiTypes.type_of(pc);
        return ShogiTypes.make_piece(swapped, pt);
    }

    /// <summary>
    /// 手番基準へ色を正規化する。
    /// </summary>
    private static Color NormalizeColor(Color color, bool flip)
    {
        if (!flip)
        {
            return color;
        }

        return color == Color.BLACK ? Color.WHITE : Color.BLACK;
    }
}
