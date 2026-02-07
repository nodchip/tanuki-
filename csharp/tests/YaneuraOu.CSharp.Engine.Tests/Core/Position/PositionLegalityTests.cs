using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tests.Core.Positioning;

[TestClass]
/// <summary>
/// Position の合法性判定に関する互換挙動を検証するテストクラス。
/// </summary>
public class PositionLegalityTests
{
    [TestMethod]
    /// <summary>
    /// 二歩となる歩打ちは擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_PawnDropNifu_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/4K4 b P 1", new StateInfo());
        pos.put_piece(Piece.B_PAWN, Square.SQ_57);

        Move pawnDrop = ShogiTypes.make_move_drop(PieceType.PAWN, Square.SQ_55, Color.BLACK);

        Assert.IsFalse(pos.pseudo_legal(pawnDrop));
    }

    [TestMethod]
    /// <summary>
    /// 敵陣外での成り指し手は擬似合法でも legal_promote では不許可となることを検証する。
    /// </summary>
    public void PseudoLegal_PromoteOutsidePromotionZone_ReturnsTrueForCompatibility()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
        pos.put_piece(Piece.B_PAWN, Square.SQ_57);

        Move invalidPromote = ShogiTypes.make_move_promote(Square.SQ_57, Square.SQ_56, Piece.B_PAWN);

        Assert.IsTrue(pos.pseudo_legal(invalidPromote));
        Assert.IsFalse(pos.legal_promote(invalidPromote));
    }

    [TestMethod]
    /// <summary>
    /// 歩の最終段不成は擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_PawnToLastRankWithoutPromotion_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
        pos.put_piece(Piece.B_PAWN, Square.SQ_12);

        Move nonPromote = ShogiTypes.make_move(Square.SQ_12, Square.SQ_11, Piece.B_PAWN);

        Assert.IsFalse(pos.pseudo_legal(nonPromote));
    }

    [TestMethod]
    /// <summary>
    /// 歩の最終段成りは擬似合法であることを検証する。
    /// </summary>
    public void PseudoLegal_PawnToLastRankWithPromotion_ReturnsTrue()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
        pos.put_piece(Piece.B_PAWN, Square.SQ_12);

        Move promote = ShogiTypes.make_move_promote(Square.SQ_12, Square.SQ_11, Piece.B_PAWN);

        Assert.IsTrue(pos.pseudo_legal(promote));
    }

    [TestMethod]
    /// <summary>
    /// 打ち歩詰めとなる歩打ちは擬似合法で除外されることを検証する。
    /// </summary>
    public void Legal_PawnDropMate_IsIllegal()
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
    /// 詰みでない歩打ちは legal_pawn_drop で許可されることを検証する。
    /// </summary>
    public void LegalPawnDrop_NotMate_ReturnsTrue()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.B_ROOK, Square.SQ_31);

        Assert.IsTrue(pos.legal_pawn_drop(Color.BLACK, Square.SQ_12));
    }

    [TestMethod]
    /// <summary>
    /// 打ち歩詰め局面では legal_drop が false になることを検証する。
    /// </summary>
    public void LegalDrop_PawnDropMate_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.B_ROOK, Square.SQ_31);
        pos.put_piece(Piece.B_GOLD, Square.SQ_23);

        Assert.IsFalse(pos.legal_drop(Square.SQ_12));
    }

    [TestMethod]
    /// <summary>
    /// 相手玉が打った歩を直接取れる場合は legal_drop が true になることを検証する。
    /// </summary>
    public void LegalDrop_KingCanCapturePawn_ReturnsTrue()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_KING, Square.SQ_99);

        Assert.IsTrue(pos.legal_drop(Square.SQ_12));
    }

    [TestMethod]
    /// <summary>
    /// 受け駒が pin で動けない場合は legal_drop が false になることを検証する。
    /// </summary>
    public void LegalDrop_PinnedDefenderCannotCapture_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_ROOK, Square.SQ_22);
        pos.put_piece(Piece.B_BISHOP, Square.SQ_33);
        pos.put_piece(Piece.B_ROOK, Square.SQ_31);
        pos.put_piece(Piece.B_GOLD, Square.SQ_23);

        Assert.IsFalse(pos.legal_drop(Square.SQ_12));
    }

    [TestMethod]
    /// <summary>
    /// 同一直線上の受けで pin を保てる場合は legal_drop が true になることを検証する。
    /// </summary>
    public void LegalDrop_SameFilePinnedDefenderCanCapture_ReturnsTrue()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b P 1", new StateInfo());
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_ROOK, Square.SQ_13);
        pos.put_piece(Piece.B_ROOK, Square.SQ_19);
        pos.put_piece(Piece.B_GOLD, Square.SQ_23);

        Assert.IsTrue(pos.legal_drop(Square.SQ_12));
    }

    [TestMethod]
    /// <summary>
    /// 王手中に王手回避へ寄与しない移動は擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_InCheck_UnrelatedMove_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.W_ROOK, Square.SQ_51);
        pos.put_piece(Piece.B_GOLD, Square.SQ_99);

        Move unrelated = ShogiTypes.make_move(Square.SQ_99, Square.SQ_98, Piece.B_GOLD);

        Assert.IsTrue(pos.in_check());
        Assert.IsFalse(pos.pseudo_legal(unrelated));
    }

    [TestMethod]
    /// <summary>
    /// 王手中の駒打ちは合駒のみ許可されることを検証する。
    /// </summary>
    public void PseudoLegal_InCheck_DropThatDoesNotBlock_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b G 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_59);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.W_ROOK, Square.SQ_51);

        Move badDrop = ShogiTypes.make_move_drop(PieceType.GOLD, Square.SQ_11, Color.BLACK);
        Move blockDrop = ShogiTypes.make_move_drop(PieceType.GOLD, Square.SQ_55, Color.BLACK);

        Assert.IsTrue(pos.in_check());
        Assert.IsFalse(pos.pseudo_legal(badDrop));
        Assert.IsTrue(pos.pseudo_legal(blockDrop));
    }

    [TestMethod]
    /// <summary>
    /// all=false では敵陣での歩の不成が擬似合法でないことを検証する。
    /// </summary>
    public void PseudoLegal_DefaultMode_DisallowsUnpromotedPawnIntoEnemyField()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_PAWN, Square.SQ_44);

        Move nonPromote = ShogiTypes.make_move(Square.SQ_44, Square.SQ_43, Piece.B_PAWN);

        Assert.IsFalse(pos.pseudo_legal(nonPromote));
        Assert.IsTrue(pos.pseudo_legal(nonPromote, true));
    }
}
