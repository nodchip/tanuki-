using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tests.Core.Positioning;

[TestClass]
/// <summary>
/// Position の反復判定に関するテストクラス。
/// </summary>
public class PositionRepetitionTests
{
    [TestMethod]
    /// <summary>
    /// 初期局面では反復が成立していないことを検証する。
    /// </summary>
    public void Repetition_InitialPosition_ReturnsNone()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);

        Assert.IsFalse(pos.has_repeated());
        Assert.AreEqual(RepetitionState.REPETITION_NONE, pos.is_repetition(100));
    }

    [TestMethod]
    /// <summary>
    /// 同一局面に戻る往復を行うと反復が検出されることを検証する。
    /// </summary>
    public void Repetition_BackAndForth_Detected()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_99);
        pos.put_piece(Piece.W_KING, Square.SQ_11);

        Move b1 = ShogiTypes.make_move(Square.SQ_99, Square.SQ_98, Piece.B_KING);
        Move w1 = ShogiTypes.make_move(Square.SQ_11, Square.SQ_12, Piece.W_KING);
        Move b2 = ShogiTypes.make_move(Square.SQ_98, Square.SQ_99, Piece.B_KING);
        Move w2 = ShogiTypes.make_move(Square.SQ_12, Square.SQ_11, Piece.W_KING);

        pos.do_move(b1, new StateInfo(), false);
        pos.do_move(w1, new StateInfo(), false);
        pos.do_move(b2, new StateInfo(), false);
        pos.do_move(w2, new StateInfo(), false);

        RepetitionState state = pos.is_repetition(100, out int foundPly);

        Assert.IsTrue(pos.has_repeated());
        Assert.AreEqual(RepetitionState.REPETITION_DRAW, state);
        Assert.AreEqual(4, foundPly);
    }
}
