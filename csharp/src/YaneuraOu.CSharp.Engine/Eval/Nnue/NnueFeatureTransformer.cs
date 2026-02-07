using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEのHalfKP特徴を構築・差分更新するクラス。
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
    /// 局面からNNUE入力特徴(512次元)を生成する。
    /// </summary>
    public byte[] Transform(Position position, NnueModel model)
    {
        var transformed = new byte[NnueModel.HalfDimensions * 2];
        var blackAccum = new int[NnueModel.HalfDimensions];
        var whiteAccum = new int[NnueModel.HalfDimensions];
        BuildAccumulation(position, model, Color.BLACK, blackAccum);
        BuildAccumulation(position, model, Color.WHITE, whiteAccum);
        ConvertAccumulatorsToFeatures(position.side_to_move(), blackAccum, whiteAccum, transformed);
        return transformed;
    }

    /// <summary>
    /// 指定視点の累積値をフル再構築する。
    /// </summary>
    public void BuildAccumulation(Position position, NnueModel model, Color perspective, int[] accumulation)
    {
        if (accumulation.Length != NnueModel.HalfDimensions)
        {
            throw new ArgumentException("accumulation length must be HalfDimensions", nameof(accumulation));
        }

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
            ApplyFeatureWeight(model, accumulation, baseIndex + bonaPieces[i], add: true);
        }
    }

    /// <summary>
    /// 2視点の累積値を入力特徴へ変換する。
    /// </summary>
    public void ConvertAccumulatorsToFeatures(Color sideToMove, int[] blackAccum, int[] whiteAccum, byte[] transformed)
    {
        if (blackAccum.Length != NnueModel.HalfDimensions || whiteAccum.Length != NnueModel.HalfDimensions)
        {
            throw new ArgumentException("accumulator length must be HalfDimensions");
        }

        if (transformed.Length != NnueModel.HalfDimensions * 2)
        {
            throw new ArgumentException("transformed length must be HalfDimensions*2", nameof(transformed));
        }

        int[] friend = sideToMove == Color.BLACK ? blackAccum : whiteAccum;
        int[] enemy = sideToMove == Color.BLACK ? whiteAccum : blackAccum;
        for (int i = 0; i < NnueModel.HalfDimensions; i++)
        {
            transformed[i] = (byte)Math.Clamp(friend[i], 0, 127);
            transformed[NnueModel.HalfDimensions + i] = (byte)Math.Clamp(enemy[i], 0, 127);
        }
    }

    /// <summary>
    /// 1手の差分を累積値へ適用する。失敗時はfalseを返す。
    /// </summary>
    public bool TryApplyMoveDelta(
        Position positionAfterMove,
        NnueModel model,
        Move move,
        Piece capturedPiece,
        Color movingSide,
        int[] blackAccum,
        int[] whiteAccum)
    {
        RefreshKingSquares(positionAfterMove);

        if (move.is_drop())
        {
            Piece dropped = move.moved_after_piece();
            Square to = move.to_sq();
            ApplyBoardAdd(model, blackAccum, whiteAccum, dropped, to);

            int newCount = ShogiTypes.hand_count(positionAfterMove.hand_of(movingSide), move.move_dropped_piece());
            ApplyHandRemove(model, blackAccum, whiteAccum, movingSide, move.move_dropped_piece(), newCount);
            return true;
        }

        Square from = move.from_sq();
        Square toSq = move.to_sq();
        Piece movedAfter = move.moved_after_piece();
        PieceType movedTypeAfter = ShogiTypes.type_of(movedAfter);
        if (movedTypeAfter == PieceType.KING)
        {
            return false;
        }

        Piece movingBefore = move.is_promote()
            ? ShogiTypes.make_piece(movingSide, Demote(movedTypeAfter))
            : ShogiTypes.make_piece(movingSide, movedTypeAfter);

        ApplyBoardRemove(model, blackAccum, whiteAccum, movingBefore, from);
        ApplyBoardAdd(model, blackAccum, whiteAccum, movedAfter, toSq);

        if (capturedPiece != Piece.NO_PIECE)
        {
            if (ShogiTypes.type_of(capturedPiece) == PieceType.KING)
            {
                return false;
            }

            ApplyBoardRemove(model, blackAccum, whiteAccum, capturedPiece, toSq);
            PieceType rawCaptured = ShogiTypes.raw_type_of(capturedPiece);
            int newCount = ShogiTypes.hand_count(positionAfterMove.hand_of(movingSide), rawCaptured);
            ApplyHandAdd(model, blackAccum, whiteAccum, movingSide, rawCaptured, newCount - 1);
        }

        return true;
    }

    /// <summary>
    /// 局面と視点からBonaPiece一覧を生成する。
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
    /// 盤上特徴を追加する。
    /// </summary>
    private void ApplyBoardAdd(NnueModel model, int[] blackAccum, int[] whiteAccum, Piece piece, Square square)
    {
        ApplyBoard(model, blackAccum, Color.BLACK, piece, square, add: true);
        ApplyBoard(model, whiteAccum, Color.WHITE, piece, square, add: true);
    }

    /// <summary>
    /// 盤上特徴を除去する。
    /// </summary>
    private void ApplyBoardRemove(NnueModel model, int[] blackAccum, int[] whiteAccum, Piece piece, Square square)
    {
        ApplyBoard(model, blackAccum, Color.BLACK, piece, square, add: false);
        ApplyBoard(model, whiteAccum, Color.WHITE, piece, square, add: false);
    }

    /// <summary>
    /// 持ち駒特徴を追加する。
    /// </summary>
    private void ApplyHandAdd(NnueModel model, int[] blackAccum, int[] whiteAccum, Color owner, PieceType rawType, int slot)
    {
        if (slot < 0)
        {
            return;
        }

        int indexBlack = GetHandBonaIndex(owner, Color.BLACK, rawType, slot);
        int indexWhite = GetHandBonaIndex(owner, Color.WHITE, rawType, slot);
        ApplyFeatureByBona(model, blackAccum, Color.BLACK, indexBlack, add: true);
        ApplyFeatureByBona(model, whiteAccum, Color.WHITE, indexWhite, add: true);
    }

    /// <summary>
    /// 持ち駒特徴を除去する。
    /// </summary>
    private void ApplyHandRemove(NnueModel model, int[] blackAccum, int[] whiteAccum, Color owner, PieceType rawType, int slot)
    {
        if (slot < 0)
        {
            return;
        }

        int indexBlack = GetHandBonaIndex(owner, Color.BLACK, rawType, slot);
        int indexWhite = GetHandBonaIndex(owner, Color.WHITE, rawType, slot);
        ApplyFeatureByBona(model, blackAccum, Color.BLACK, indexBlack, add: false);
        ApplyFeatureByBona(model, whiteAccum, Color.WHITE, indexWhite, add: false);
    }

    /// <summary>
    /// 盤上特徴を指定視点に適用する。
    /// </summary>
    private void ApplyBoard(NnueModel model, int[] accumulation, Color perspective, Piece piece, Square square, bool add)
    {
        PieceType raw = ShogiTypes.raw_type_of(piece);
        if (raw == PieceType.KING || raw == PieceType.NO_PIECE_TYPE)
        {
            return;
        }

        int boardBase = GetBoardBase(piece, perspective);
        int squareOffset = perspective == Color.BLACK ? (int)square : (int)ShogiTypes.Flip(square);
        int bonaIndex = boardBase + squareOffset;
        ApplyFeatureByBona(model, accumulation, perspective, bonaIndex, add);
    }

    /// <summary>
    /// BonaIndexから重みを適用する。
    /// </summary>
    private void ApplyFeatureByBona(NnueModel model, int[] accumulation, Color perspective, int bonaIndex, bool add)
    {
        Square kingSquare = perspective == Color.BLACK
            ? positionKingBlack
            : positionKingWhite;
        if (kingSquare == Square.SQ_NB)
        {
            return;
        }

        int kingIndex = perspective == Color.BLACK ? (int)kingSquare : (int)ShogiTypes.Flip(kingSquare);
        ApplyFeatureWeight(model, accumulation, kingIndex * FeEnd + bonaIndex, add);
    }

    private Square positionKingBlack;
    private Square positionKingWhite;

    /// <summary>
    /// 差分適用前に王位置を更新する。
    /// </summary>
    private void RefreshKingSquares(Position position)
    {
        positionKingBlack = position.king_square(Color.BLACK);
        positionKingWhite = position.king_square(Color.WHITE);
    }

    private void ApplyFeatureWeight(NnueModel model, int[] accumulation, int featureIndex, bool add)
    {
        int weightOffset = featureIndex * NnueModel.HalfDimensions;
        int sign = add ? 1 : -1;
        for (int j = 0; j < NnueModel.HalfDimensions; j++)
        {
            accumulation[j] += sign * model.FtWeights[weightOffset + j];
        }
    }

    private int GetHandBonaIndex(Color owner, Color perspective, PieceType rawType, int slot)
    {
        int pieceTypeIndex = Array.IndexOf(HandPieceTypes, rawType);
        if (pieceTypeIndex < 0)
        {
            return 0;
        }

        int handBase = GetHandBase(owner, perspective, pieceTypeIndex);
        return handBase + slot;
    }

    private static PieceType Demote(PieceType type)
    {
        return type switch
        {
            PieceType.PRO_PAWN => PieceType.PAWN,
            PieceType.PRO_LANCE => PieceType.LANCE,
            PieceType.PRO_KNIGHT => PieceType.KNIGHT,
            PieceType.PRO_SILVER => PieceType.SILVER,
            PieceType.HORSE => PieceType.BISHOP,
            PieceType.DRAGON => PieceType.ROOK,
            _ => type,
        };
    }

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

    private static int GetHandBase(Color owner, Color perspective, int pieceTypeIndex)
    {
        if (perspective == Color.BLACK)
        {
            return owner == Color.BLACK ? HandBaseBlack[pieceTypeIndex] : HandBaseWhite[pieceTypeIndex];
        }

        return owner == Color.BLACK ? HandBaseWhite[pieceTypeIndex] : HandBaseBlack[pieceTypeIndex];
    }

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

    /// <summary>
    /// 差分適用の前処理として王位置を読み込む。
    /// </summary>
    public void PrepareForDelta(Position position)
    {
        RefreshKingSquares(position);
    }
}
