using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Search;

namespace YaneuraOu.CSharp.Engine.Tests.Regression;

/// <summary>
/// 主要シナリオの回帰を検証するテストクラス。
/// </summary>
[TestClass]
public class EngineRegressionTests
{
    /// <summary>
    /// 開始局面で探索すると常に有効なbestmoveが返ることを検証する。
    /// </summary>
    [TestMethod]
    public void Search_StartPosition_ReturnsNonNoneMove()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher();

        SearchResult result = searcher.Search(position, new SearchLimits { Depth = 1 });

        Assert.AreNotEqual(0u, result.BestMove.to_u32());
    }

    /// <summary>
    /// 王の往復による反復局面が引き分け判定になることを検証する。
    /// </summary>
    [TestMethod]
    public void Repetition_BackAndForthKings_IsDraw()
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

        Assert.AreEqual(RepetitionState.REPETITION_DRAW, pos.is_repetition(100));
    }

    /// <summary>
    /// 規定点以上の入玉局面で宣言勝ちを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void DeclarationWin_EnoughPoints_ReturnsWinMove()
    {
        var pos = new Position();
        pos.set("9/9/9/9/9/9/9/9/9 b 2R2B4G4S4N4L18P 1", new StateInfo());
        pos.put_piece(Piece.B_KING, Square.SQ_53);
        pos.put_piece(Piece.W_KING, Square.SQ_11);
        pos.set_ekr(EnteringKingRule.EKR_27_POINT);

        Assert.AreEqual(Move.win().to_u32(), pos.DeclarationWin().to_u32());
    }

    /// <summary>
    /// 同一局面を同一条件で探索したときに最善手が再現することを検証する。
    /// </summary>
    [TestMethod]
    public void Search_SamePositionSameDepth_IsDeterministic()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher();

        SearchResult first = searcher.Search(position, new SearchLimits { Depth = 2 });
        SearchResult second = searcher.Search(position, new SearchLimits { Depth = 2 });

        Assert.AreEqual(first.BestMove.to_u32(), second.BestMove.to_u32());
    }
}
