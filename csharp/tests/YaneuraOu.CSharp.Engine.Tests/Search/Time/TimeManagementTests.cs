using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Search.Time;

namespace YaneuraOu.CSharp.Engine.Tests.Search.Time;

/// <summary>
/// TimeManagementの時間配分を検証するテストクラス。
/// </summary>
[TestClass]
public class TimeManagementTests
{
    /// <summary>
    /// movetime指定時に固定時間配分になることを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithMoveTime_UsesFixedBudget()
    {
        var limits = new LimitsType
        {
            MoveTimeMs = 1500,
        };
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.AreEqual(1500, sut.MinimumTimeMs);
        Assert.AreEqual(1500, sut.OptimumTimeMs);
        Assert.AreEqual(1500, sut.MaximumTimeMs);
    }

    /// <summary>
    /// byoyomi単独指定時に不変条件を満たすことを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithByoyomiOnly_RespectsOrderingInvariant()
    {
        var limits = new LimitsType
        {
            ByoyomiMs = 2000,
        };
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.IsTrue(sut.MinimumTimeMs <= sut.OptimumTimeMs);
        Assert.IsTrue(sut.OptimumTimeMs <= sut.MaximumTimeMs);
        Assert.IsTrue(sut.MaximumTimeMs > 0);
    }

    /// <summary>
    /// 残り時間と加算指定時に手番側の時間で配分されることを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithMainTimeAndIncrement_UsesSideToMoveBudget()
    {
        var limits = new LimitsType();
        limits.SetTime(Color.BLACK, 3000);
        limits.SetTime(Color.WHITE, 120000);
        limits.SetIncrement(Color.BLACK, 300);
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.AreEqual(400, sut.OptimumTimeMs);
        Assert.AreEqual(400, sut.MaximumTimeMs);
    }
}
