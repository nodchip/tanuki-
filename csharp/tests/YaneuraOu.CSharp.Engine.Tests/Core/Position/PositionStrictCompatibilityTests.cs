using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tests.Core.Positioning;

[TestClass]
/// <summary>
/// Position 縺ｮ C++ 莠呈鋤諤ｧ・亥宍蟇・擅莉ｶ・峨ｒ讀懆ｨｼ縺吶ｋ繝・せ繝医け繝ｩ繧ｹ縲・/// </summary>
public class PositionStrictCompatibilityTests
{
    [TestMethod]
    /// <summary>
    /// drop 謖・＠謇九・ moved_after_piece 縺御ｸ堺ｸ閾ｴ縺ｪ繧画闘莨ｼ蜷域ｳ輔〒縺ｪ縺・％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// 荳肴・謖・＠謇九・ moved_after_piece 縺御ｸ堺ｸ閾ｴ縺ｪ繧画闘莨ｼ蜷域ｳ輔〒縺ｪ縺・％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// 謌舌ｊ謖・＠謇九・ moved_after_piece 縺御ｸ堺ｸ閾ｴ縺ｪ繧画闘莨ｼ蜷域ｳ輔〒縺ｪ縺・％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// 荳｡邇区焔譎ゅ↓髱樒脂遘ｻ蜍輔・謫ｬ莨ｼ蜷域ｳ輔〒縺ｪ縺・％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// 邇区焔荳ｭ縺ｧ縺ｪ縺・壼ｸｸ螻髱｢縺ｧ繧ゅ∫脂縺悟茜縺阪∈鬟帙・霎ｼ繧謇九・ legal 縺ｧ縺ｪ縺・％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// pin 縺輔ｌ縺ｦ縺・ｋ鬧偵〒繧ら脂縺ｨ荳逶ｴ邱壹・譁ｹ蜷代↑繧・legal 縺ｧ險ｱ蜿ｯ縺輔ｌ繧九％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// 謫ｬ莨ｼ蜷域ｳ輔〒縺ｪ縺・ｭｩ謇薙■縺ｧ繧・legal(drop) 縺ｯ true 繧定ｿ斐☆縺薙→繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// C++莠呈鋤縺ｨ縺励※縲∵遠縺ｦ縺ｪ縺・ｮｵ縺ｮ豁ｩ謇薙■縺ｧ繧・pseudo_legal 縺・true 縺ｫ縺ｪ繧九％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// C++莠呈鋤縺ｨ縺励※縲∵｡ゅ・荳肴・縺ｧ陦後″蝣ｴ縺後↑縺・焔縺ｧ繧・pseudo_legal 縺・true 縺ｫ縺ｪ繧九％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// 謌舌ｌ縺ｪ縺・ｧ抵ｼ育脂/驥・謌宣ｧ抵ｼ峨・謌舌ｊ謖・＠謇九・謫ｬ莨ｼ蜷域ｳ輔〒縺ｪ縺・％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// all=false 縺ｧ縺ｯ螟ｧ鬧偵・謨ｵ髯｣荳肴・繧堤ｦ∵ｭ｢縺励∥ll=true 縺ｧ縺ｯ險ｱ蜿ｯ縺吶ｋ縺薙→繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
    /// 鬧呈遠縺｡縺ｧKING遞ｮ蛻･縺ｯ謫ｬ莨ｼ蜷域ｳ輔〒縺ｪ縺・％縺ｨ繧呈､懆ｨｼ縺吶ｋ縲・    /// </summary>
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
