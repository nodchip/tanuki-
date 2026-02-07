using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tests.Core.Positioning;

[TestClass]
public class PositionMoveCycleTests
{
    [TestMethod]
    /// <summary>
    /// do_move 後の StateInfo.hand が手番側の手駒情報と一致することを検証する。
    /// </summary>
    public void DoMove_StateHandMatchesSideToMoveHand()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());

        Move move = ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN);
        pos.do_move(move, new StateInfo(), pos.gives_check(move));

        Assert.AreEqual(pos.hand_of(pos.side_to_move()), pos.state().hand);
    }

    [TestMethod]
    /// <summary>
    /// do_move で pliesFromNull と continuousCheck が更新されることを検証する。
    /// </summary>
    public void DoMove_UpdatesPliesFromNullAndContinuousCheck()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        pos.state().pliesFromNull = 5;
        pos.state().continuousCheck[(int)Color.BLACK] = 4;
        pos.state().continuousCheck[(int)Color.WHITE] = 2;

        Move move = ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN);
        pos.do_move(move, new StateInfo(), false);

        Assert.AreEqual(6, pos.state().pliesFromNull);
        Assert.AreEqual(0, pos.state().continuousCheck[(int)Color.BLACK]);
        Assert.AreEqual(2, pos.state().continuousCheck[(int)Color.WHITE]);
    }

    [TestMethod]
    /// <summary>
    /// do_null_move で pliesFromNull と連続王手カウンタがリセットされることを検証する。
    /// </summary>
    public void DoNullMove_ResetsPliesFromNullAndContinuousCheck()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        pos.state().pliesFromNull = 7;
        pos.state().continuousCheck[(int)Color.BLACK] = 6;
        pos.state().continuousCheck[(int)Color.WHITE] = 2;

        pos.do_null_move(new StateInfo());

        Assert.AreEqual(0, pos.state().pliesFromNull);
        Assert.AreEqual(0, pos.state().continuousCheck[(int)Color.BLACK]);
        Assert.AreEqual(2, pos.state().continuousCheck[(int)Color.WHITE]);
        Assert.AreEqual(pos.hand_of(pos.side_to_move()), pos.state().hand);
    }

    [TestMethod]
    public void DoMoveUndoMove_RoundTrip_StartPosition()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        string before = pos.sfen();

        Move move = ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN);
        pos.do_move(move, new StateInfo(), pos.gives_check(move));
        pos.undo_move(move);

        Assert.AreEqual(before, pos.sfen());
    }

    [TestMethod]
    public void DoMoveUndoMove_DropRoundTrip()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/4K4 b R 1", new StateInfo());
        string before = pos.sfen();

        Move move = ShogiTypes.make_move_drop(PieceType.ROOK, Square.SQ_55, Color.BLACK);
        pos.do_move(move, new StateInfo(), pos.gives_check(move));
        pos.undo_move(move);

        Assert.AreEqual(before, pos.sfen());
    }

    [TestMethod]
    public void DoNullUndoNull_RoundTrip()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        string before = pos.sfen();

        pos.do_null_move(new StateInfo());
        Assert.IsTrue(pos.sfen().Contains(" w "));

        pos.undo_null_move();
        Assert.AreEqual(before, pos.sfen());
    }
}
