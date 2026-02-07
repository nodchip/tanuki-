using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Search;

namespace YaneuraOu.CSharp.Engine.Tests.Searching;

/// <summary>
/// MoveOrderingの優先順位を検証するテストクラス。
/// </summary>
[TestClass]
public class MoveOrderingTests
{
    /// <summary>
    /// TT手が最優先されることを検証する。
    /// </summary>
    [TestMethod]
    public void Order_WithTtMove_PrioritizesTtMove()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        MoveList legal = MoveGenerator.GenerateLegal(position);
        Move ttMove = legal[legal.Count - 1];

        var ordered = MoveOrdering.Order(position, legal, new MoveOrderingContext(), 0, ttMove);

        Assert.AreEqual(ttMove.to_u32(), ordered[0].to_u32());
    }

    /// <summary>
    /// historyが高い手が優先されることを検証する。
    /// </summary>
    [TestMethod]
    public void Order_WithHistoryBonus_PrioritizesHistoryMove()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        MoveList legal = MoveGenerator.GenerateLegal(position);
        Move preferred = legal[legal.Count - 1];

        var context = new MoveOrderingContext();
        context.AddHistory(preferred, 200);

        var ordered = MoveOrdering.Order(position, legal, context, 0, Move.none());

        Assert.AreEqual(preferred.to_u32(), ordered[0].to_u32());
    }
}
