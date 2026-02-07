using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tests.Core.Positioning;

[TestClass]
/// <summary>
/// Position.to_move / legal_promote の互換挙動を検証するテストクラス。
/// </summary>
public class PositionToMoveTests
{
    [TestMethod]
    /// <summary>
    /// 特殊指し手は to_move でそのまま返ることを検証する。
    /// </summary>
    public void ToMove_SpecialMoves_ReturnAsIs()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());

        Assert.AreEqual(Move.none().to_u32(), pos.to_move(Move16.none()).to_u32());
        Assert.AreEqual(Move.@null().to_u32(), pos.to_move(Move16.@null()).to_u32());
        Assert.AreEqual(Move.win().to_u32(), pos.to_move(Move16.win()).to_u32());
    }

    [TestMethod]
    /// <summary>
    /// 通常指し手の to_move で移動後の駒情報が付与されることを検証する。
    /// </summary>
    public void ToMove_NormalMove_AttachesMovedPiece()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_PAWN, Square.SQ_77);

        Move move = pos.to_move(ShogiTypes.make_move16(Square.SQ_77, Square.SQ_76));

        Assert.AreEqual(Square.SQ_77, move.from_sq());
        Assert.AreEqual(Square.SQ_76, move.to_sq());
        Assert.AreEqual(Piece.B_PAWN, move.moved_after_piece());
    }

    [TestMethod]
    /// <summary>
    /// 手番側でない駒を動かす Move16 は Move.none() になることを検証する。
    /// </summary>
    public void ToMove_FromOpponentPiece_ReturnsNone()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.W_PAWN, Square.SQ_33);

        Move move = pos.to_move(ShogiTypes.make_move16(Square.SQ_33, Square.SQ_34));

        Assert.AreEqual(Move.none().to_u32(), move.to_u32());
    }

    [TestMethod]
    /// <summary>
    /// 駒打ち Move16 の to_move で手番側の駒種が付与されることを検証する。
    /// </summary>
    public void ToMove_DropMove_AttachesDroppedPieceForSideToMove()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());

        Move move = pos.to_move(ShogiTypes.make_move_drop16(PieceType.GOLD, Square.SQ_55));

        Assert.IsTrue(move.is_drop());
        Assert.AreEqual(Piece.B_GOLD, move.moved_after_piece());
    }

    [TestMethod]
    /// <summary>
    /// 成り Move16 の to_move で成り後の駒が付与されることを検証する。
    /// </summary>
    public void ToMove_PromoteMove_AttachesPromotedPiece()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_PAWN, Square.SQ_12);

        Move move = pos.to_move(ShogiTypes.make_move_promote16(Square.SQ_12, Square.SQ_11));

        Assert.AreEqual(Square.SQ_12, move.from_sq());
        Assert.AreEqual(Square.SQ_11, move.to_sq());
        Assert.AreEqual(Piece.B_PRO_PAWN, move.moved_after_piece());
    }

    [TestMethod]
    /// <summary>
    /// 成り指し手の移動元・移動先が敵陣外なら legal_promote が false になることを検証する。
    /// </summary>
    public void LegalPromote_PromoteOutsideZone_ReturnsFalse()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_PAWN, Square.SQ_57);

        Move move = ShogiTypes.make_move_promote(Square.SQ_57, Square.SQ_56, Piece.B_PAWN);

        Assert.IsFalse(pos.legal_promote(move));
    }

    [TestMethod]
    /// <summary>
    /// 成り指し手の移動元または移動先が敵陣なら legal_promote が true になることを検証する。
    /// </summary>
    public void LegalPromote_PromoteInZone_ReturnsTrue()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.put_piece(Piece.B_PAWN, Square.SQ_44);

        Move move = ShogiTypes.make_move_promote(Square.SQ_44, Square.SQ_43, Piece.B_PAWN);

        Assert.IsTrue(pos.legal_promote(move));
    }
}
