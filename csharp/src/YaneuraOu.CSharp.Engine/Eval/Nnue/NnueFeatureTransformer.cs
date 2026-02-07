using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE向けのHalfKP特徴量を変換するクラス。
/// </summary>
public sealed class NnueFeatureTransformer
{
    private const int FeEnd = 1548;
    private const int FeHandEnd = 90;
    private const int PieceNumberKing = 38;

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

    private static readonly int[] HandBaseBlack = { 1, 39, 49, 59, 69, 79, 85 };
    private static readonly int[] HandBaseWhite = { 20, 44, 54, 64, 74, 82, 88 };

    /// <summary>
    /// 局面から変換後特徴量(512次元)を返す。
    /// </summary>
    public byte[] Transform(Position position, NnueModel model)
    {
        var transformed = new byte[NnueModel.HalfDimensions * 2];
        var friendAccum = new int[NnueModel.HalfDimensions];
        var enemyAccum = new int[NnueModel.HalfDimensions];

        BuildAccumulation(position, model, position.side_to_move(), friendAccum);
        BuildAccumulation(position, model, position.side_to_move() == Color.BLACK ? Color.WHITE : Color.BLACK, enemyAccum);

        for (int i = 0; i < NnueModel.HalfDimensions; i++)
        {
            transformed[i] = (byte)Math.Clamp(friendAccum[i], 0, 127);
            transformed[NnueModel.HalfDimensions + i] = (byte)Math.Clamp(enemyAccum[i], 0, 127);
        }

        return transformed;
    }

    /// <summary>
    /// 指定視点のHalfKP累積値を生成する。
    /// </summary>
    private static void BuildAccumulation(Position position, NnueModel model, Color perspective, int[] accumulation)
    {
        for (int i = 0; i < accumulation.Length; i++)
        {
            accumulation[i] = model.FtBiases[i];
        }

        List<int> bonaPieces = BuildBonaPieces(position, perspective);

        Square kingSquare = position.king_square(perspective);
        if (kingSquare == Square.SQ_NB)
        {
            return;
        }

        int kingIndex = perspective == Color.BLACK ? (int)kingSquare : (int)ShogiTypes.Flip(kingSquare);
        int baseIndex = kingIndex * FeEnd;

        for (int i = 0; i < bonaPieces.Count; i++)
        {
            int activeIndex = baseIndex + bonaPieces[i];
            int weightOffset = activeIndex * NnueModel.HalfDimensions;
            for (int j = 0; j < NnueModel.HalfDimensions; j++)
            {
                accumulation[j] += model.FtWeights[weightOffset + j];
            }
        }
    }

    /// <summary>
    /// 視点側のBonaPiece列を生成する。
    /// </summary>
    private static List<int> BuildBonaPieces(Position position, Color perspective)
    {
        var bonaPieces = new List<int>(PieceNumberKing);

        for (int sqValue = 0; sqValue < (int)Square.SQ_NB; sqValue++)
        {
            Square sq = (Square)sqValue;
            Piece piece = position.piece_on(sq);
            if (piece == Piece.NO_PIECE)
            {
                continue;
            }

            PieceType rawType = ShogiTypes.raw_type_of(piece);
            if (rawType == PieceType.KING || rawType == PieceType.NO_PIECE_TYPE)
            {
                continue;
            }

            int boardBase = GetBoardBase(piece, perspective);
            int squareOffset = perspective == Color.BLACK ? sqValue : (int)ShogiTypes.Flip(sq);
            bonaPieces.Add(boardBase + squareOffset);
        }

        for (int ownerValue = 0; ownerValue < (int)Color.COLOR_NB; ownerValue++)
        {
            Color owner = (Color)ownerValue;
            uint hand = position.hand_of(owner);
            for (int i = 0; i < HandPieceTypes.Length; i++)
            {
                int count = ShogiTypes.hand_count(hand, HandPieceTypes[i]);
                int handBase = GetHandBase(owner, perspective, i);
                for (int n = 0; n < count; n++)
                {
                    bonaPieces.Add(handBase + n);
                }
            }
        }

        while (bonaPieces.Count < PieceNumberKing)
        {
            bonaPieces.Add(0);
        }

        if (bonaPieces.Count > PieceNumberKing)
        {
            bonaPieces.RemoveRange(PieceNumberKing, bonaPieces.Count - PieceNumberKing);
        }

        return bonaPieces;
    }

    /// <summary>
    /// 盤上駒のBonaPiece先頭番号を返す。
    /// </summary>
    private static int GetBoardBase(Piece piece, Color perspective)
    {
        bool blackPiece = ShogiTypes.color_of(piece) == Color.BLACK;
        PieceType type = ShogiTypes.type_of(piece);
        type = NormalizeGoldFamily(type);

        return (perspective, blackPiece, type) switch
        {
            (Color.BLACK, true, PieceType.PAWN) => 90,
            (Color.BLACK, false, PieceType.PAWN) => 171,
            (Color.BLACK, true, PieceType.LANCE) => 252,
            (Color.BLACK, false, PieceType.LANCE) => 333,
            (Color.BLACK, true, PieceType.KNIGHT) => 414,
            (Color.BLACK, false, PieceType.KNIGHT) => 495,
            (Color.BLACK, true, PieceType.SILVER) => 576,
            (Color.BLACK, false, PieceType.SILVER) => 657,
            (Color.BLACK, true, PieceType.GOLD) => 738,
            (Color.BLACK, false, PieceType.GOLD) => 819,
            (Color.BLACK, true, PieceType.BISHOP) => 900,
            (Color.BLACK, false, PieceType.BISHOP) => 981,
            (Color.BLACK, true, PieceType.HORSE) => 1062,
            (Color.BLACK, false, PieceType.HORSE) => 1143,
            (Color.BLACK, true, PieceType.ROOK) => 1224,
            (Color.BLACK, false, PieceType.ROOK) => 1305,
            (Color.BLACK, true, PieceType.DRAGON) => 1386,
            (Color.BLACK, false, PieceType.DRAGON) => 1467,

            (Color.WHITE, true, PieceType.PAWN) => 171,
            (Color.WHITE, false, PieceType.PAWN) => 90,
            (Color.WHITE, true, PieceType.LANCE) => 333,
            (Color.WHITE, false, PieceType.LANCE) => 252,
            (Color.WHITE, true, PieceType.KNIGHT) => 495,
            (Color.WHITE, false, PieceType.KNIGHT) => 414,
            (Color.WHITE, true, PieceType.SILVER) => 657,
            (Color.WHITE, false, PieceType.SILVER) => 576,
            (Color.WHITE, true, PieceType.GOLD) => 819,
            (Color.WHITE, false, PieceType.GOLD) => 738,
            (Color.WHITE, true, PieceType.BISHOP) => 981,
            (Color.WHITE, false, PieceType.BISHOP) => 900,
            (Color.WHITE, true, PieceType.HORSE) => 1143,
            (Color.WHITE, false, PieceType.HORSE) => 1062,
            (Color.WHITE, true, PieceType.ROOK) => 1305,
            (Color.WHITE, false, PieceType.ROOK) => 1224,
            (Color.WHITE, true, PieceType.DRAGON) => 1467,
            (Color.WHITE, false, PieceType.DRAGON) => 1386,
            _ => FeHandEnd,
        };
    }

    /// <summary>
    /// 手駒のBonaPiece先頭番号を返す。
    /// </summary>
    private static int GetHandBase(Color owner, Color perspective, int pieceTypeIndex)
    {
        if (perspective == Color.BLACK)
        {
            return owner == Color.BLACK ? HandBaseBlack[pieceTypeIndex] : HandBaseWhite[pieceTypeIndex];
        }

        return owner == Color.BLACK ? HandBaseWhite[pieceTypeIndex] : HandBaseBlack[pieceTypeIndex];
    }

    /// <summary>
    /// 金系へ集約する駒種を正規化する。
    /// </summary>
    private static PieceType NormalizeGoldFamily(PieceType type)
    {
        return type switch
        {
            PieceType.PRO_PAWN => PieceType.GOLD,
            PieceType.PRO_LANCE => PieceType.GOLD,
            PieceType.PRO_KNIGHT => PieceType.GOLD,
            PieceType.PRO_SILVER => PieceType.GOLD,
            _ => type,
        };
    }
}
