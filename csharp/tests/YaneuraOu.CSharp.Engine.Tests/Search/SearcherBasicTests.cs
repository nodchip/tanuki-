using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Search;

namespace YaneuraOu.CSharp.Engine.Tests.Searching;

[TestClass]
/// <summary>
/// Searcher の基本動作を検証するテストクラス。
/// </summary>
public class SearcherBasicTests
{
    [TestMethod]
    /// <summary>
    /// 深さ1探索で bestmove が返ることを検証する。
    /// </summary>
    public void Search_Depth1_ReturnsBestMove()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher();

        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 1 });

        Assert.AreNotEqual(Move.none().to_u32(), result.BestMove.to_u32());
        Assert.IsTrue(result.BestMove.is_ok());
        Assert.IsTrue(result.Nodes > 0);
    }

    [TestMethod]
    /// <summary>
    /// 返却された bestmove が合法手であることを検証する。
    /// </summary>
    public void Search_ReturnedMove_IsLegal()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher();

        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 2 });
        MoveList legal = MoveGenerator.GenerateLegal(pos);

        bool found = false;
        foreach (Move move in legal)
        {
            if (move.to_u32() == result.BestMove.to_u32())
            {
                found = true;
                break;
            }
        }

        Assert.IsTrue(found);
    }

    [TestMethod]
    /// <summary>
    /// 合法手がない局面では bestmove が none になることを検証する。
    /// </summary>
    public void Search_NoLegalMove_ReturnsNone()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
        pos.remove_piece(Square.SQ_59);

        var searcher = new Searcher();
        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 2 });

        Assert.AreEqual(Move.none().to_u32(), result.BestMove.to_u32());
    }
}
