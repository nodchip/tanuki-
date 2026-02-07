using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
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
}
