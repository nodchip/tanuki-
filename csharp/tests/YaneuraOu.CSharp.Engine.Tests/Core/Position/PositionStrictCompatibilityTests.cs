using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tests.Core.Positioning;

[TestClass]
/// <summary>
/// Position の C++ 互換性（厳密条件）を検証するテストクラス。
/// </summary>
public class PositionStrictCompatibilityTests
{
    [TestMethod]
    /// <summary>
    /// drop 指し手の moved_after_piece が不一致なら擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_DropMovedAfterMismatch_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);

        Move valid = ShogiTypes.make_move_drop(PieceType.PAWN, Square.SQ_55, Color.BLACK);
        Move invalid = new Move((valid.to_u32() & 0xFFFFu) + ((uint)Piece.B_GOLD << 16));

        Assert.IsFalse(pos.pseudo_legal(invalid, true));
    }

    [TestMethod]
    /// <summary>
    /// 不成指し手の moved_after_piece が不一致なら擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_NonPromoteMovedAfterMismatch_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_PAWN, Square.SQ_57);

        Move valid = ShogiTypes.make_move(Square.SQ_57, Square.SQ_56, Piece.B_PAWN);
        Move invalid = new Move((valid.to_u32() & 0xFFFFu) + ((uint)Piece.B_GOLD << 16));

        Assert.IsFalse(pos.pseudo_legal(invalid, true));
    }

    [TestMethod]
    /// <summary>
    /// 成り指し手の moved_after_piece が不一致なら擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_PromoteMovedAfterMismatch_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_PAWN, Square.SQ_12);

        Move valid = ShogiTypes.make_move_promote(Square.SQ_12, Square.SQ_11, Piece.B_PAWN);
        Move invalid = new Move((valid.to_u32() & 0xFFFFu) + ((uint)Piece.B_PAWN << 16));

        Assert.IsFalse(pos.pseudo_legal(invalid, true));
    }

    [TestMethod]
    /// <summary>
    /// 両王手時に非玉移動は擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_DoubleCheck_NonKingMove_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.W_ROOK, Square.SQ_51);
        pos.put_piece(Piece.W_BISHOP, Square.SQ_37);
        pos.put_piece(Piece.B_GOLD, Square.SQ_79);

        Move goldMove = ShogiTypes.make_move(Square.SQ_79, Square.SQ_78, Piece.B_GOLD);

        Assert.IsTrue(pos.in_check());
        Assert.IsFalse(pos.pseudo_legal(goldMove, true));
    }

    [TestMethod]
    /// <summary>
    /// 王手中でない通常局面でも、玉が利きへ飛び込む手は legal でないことを検証する。
    /// </summary>
    public void Legal_KingMoveIntoAttack_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.W_ROOK, Square.SQ_51);

        Move kingIntoAttack = ShogiTypes.make_move(Square.SQ_59, Square.SQ_58, Piece.B_KING);

        Assert.IsTrue(pos.pseudo_legal(kingIntoAttack, true));
        Assert.IsFalse(pos.legal(kingIntoAttack));
    }

    [TestMethod]
    /// <summary>
    /// pin されている駒でも玉と一直線の方向なら legal で許可されることを検証する。
    /// </summary>
    public void Legal_PinnedPieceAlignedMove_ReturnsTrue()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_GOLD, Square.SQ_58);
        pos.put_piece(Piece.W_ROOK, Square.SQ_51);

        Move alignedMove = ShogiTypes.make_move(Square.SQ_58, Square.SQ_57, Piece.B_GOLD);

        Assert.IsTrue(pos.pseudo_legal(alignedMove, true));
        Assert.IsTrue(pos.legal(alignedMove));
    }

    [TestMethod]
    /// <summary>
    /// 擬似合法でない歩打ちでも legal(drop) は true を返すことを検証する。
    /// </summary>
    public void Legal_DropMove_ReturnsTrueEvenWhenPseudoLegalIsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.B_ROOK, Square.SQ_31);
        pos.put_piece(Piece.B_GOLD, Square.SQ_23);

        Move pawnDropMate = ShogiTypes.make_move_drop(PieceType.PAWN, Square.SQ_12, Color.BLACK);

        Assert.IsFalse(pos.pseudo_legal(pawnDropMate));
        Assert.IsTrue(pos.legal(pawnDropMate));
    }

    [TestMethod]
    /// <summary>
    /// C++互換として、打てない段の歩打ちでも pseudo_legal が true になることを検証する。
    /// </summary>
    public void PseudoLegal_DropPawnToLastRank_ReturnsTrueForCompatibility()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_55);

        Move pawnDropLastRank = ShogiTypes.make_move_drop(PieceType.PAWN, Square.SQ_11, Color.BLACK);

        Assert.IsTrue(pos.pseudo_legal(pawnDropLastRank, true));
    }

    [TestMethod]
    /// <summary>
    /// C++互換として、桂の不成で行き場がない手でも pseudo_legal が true になることを検証する。
    /// </summary>
    public void PseudoLegal_KnightNoPromoteToLastRank_ReturnsTrueForCompatibility()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_55);
        pos.put_piece(Piece.B_KNIGHT, Square.SQ_23);

        Move knightNoPromote = ShogiTypes.make_move(Square.SQ_23, Square.SQ_11, Piece.B_KNIGHT);

        Assert.IsTrue(pos.pseudo_legal(knightNoPromote, true));
        Assert.IsTrue(pos.pseudo_legal(knightNoPromote, false));
    }

    [TestMethod]
    /// <summary>
    /// 成れない駒（玉/金/成駒）の成り指し手は擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_PromoteNonPromotablePiece_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_GOLD, Square.SQ_44);

        Move invalidPromote = ShogiTypes.make_move_promote(Square.SQ_44, Square.SQ_43, Piece.B_GOLD);

        Assert.IsFalse(pos.pseudo_legal(invalidPromote, true));
        Assert.IsFalse(pos.pseudo_legal(invalidPromote, false));
    }

    [TestMethod]
    /// <summary>
    /// all=false では大駒の敵陣不成を禁止し、all=true では許可することを検証する。
    /// </summary>
    public void PseudoLegal_RookNonPromoteInPromotionZone_DiffersByAllFlag()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_ROOK, Square.SQ_43);

        Move rookNonPromote = ShogiTypes.make_move(Square.SQ_43, Square.SQ_42, Piece.B_ROOK);

        Assert.IsFalse(pos.pseudo_legal(rookNonPromote, false));
        Assert.IsTrue(pos.pseudo_legal(rookNonPromote, true));
    }

    [TestMethod]
    /// <summary>
    /// 駒打ちでKING種別は擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_DropKingPieceType_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b G 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);

        Move invalidKingDrop = ShogiTypes.make_move_drop(PieceType.KING, Square.SQ_55, Color.BLACK);

        Assert.IsFalse(pos.pseudo_legal(invalidKingDrop, true));
        Assert.IsFalse(pos.pseudo_legal(invalidKingDrop, false));
    }

    [TestMethod]
    /// <summary>
    /// 単王手時に王手駒の捕獲は擬似合法になることを検証する。
    /// </summary>
    public void PseudoLegal_SingleCheck_CaptureChecker_ReturnsTrue()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.W_ROOK, Square.SQ_51);
        pos.put_piece(Piece.B_BISHOP, Square.SQ_62);

        Move captureChecker = ShogiTypes.make_move(Square.SQ_62, Square.SQ_51, Piece.B_BISHOP);

        Assert.IsTrue(pos.in_check());
        Assert.IsTrue(pos.pseudo_legal(captureChecker, true));
    }

    [TestMethod]
    /// <summary>
    /// 単王手時に王手回避に関係ない移動は擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_SingleCheck_UnrelatedMove_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.W_ROOK, Square.SQ_51);
        pos.put_piece(Piece.B_GOLD, Square.SQ_79);

        Move unrelatedMove = ShogiTypes.make_move(Square.SQ_79, Square.SQ_78, Piece.B_GOLD);

        Assert.IsTrue(pos.in_check());
        Assert.IsFalse(pos.pseudo_legal(unrelatedMove, true));
    }
}
