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
        Assert.AreEqual(1950, sut.MaximumTimeMs);
    }

    /// <summary>
    /// MoveOverhead指定時に持ち時間配分が短縮されることを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithByoyomiAndMoveOverhead_SubtractsOverhead()
    {
        var limits = new LimitsType
        {
            ByoyomiMs = 2000,
            MoveOverheadMs = 100,
        };
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.AreEqual(1850, sut.MaximumTimeMs);
    }

    /// <summary>
    /// SlowMover指定時に最適時間が倍率で変化することを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithSlowMover_AppliesOptimumScale()
    {
        var limits = new LimitsType
        {
            SlowMover = 50,
            MinimumThinkingTimeMs = 1,
        };
        limits.SetTime(Color.BLACK, 6000);
        limits.SetIncrement(Color.BLACK, 0);
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.AreEqual(100, sut.OptimumTimeMs);
    }

    /// <summary>
    /// MinimumThinkingTime指定時に最小時間へ下限が適用されることを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithMinimumThinkingTime_AppliesMinimumFloor()
    {
        var limits = new LimitsType
        {
            MinimumThinkingTimeMs = 1200,
            SlowMover = 100,
            ByoyomiMs = 1500,
        };
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.AreEqual(1200, sut.MinimumTimeMs);
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

    /// <summary>
    /// movestogo指定時に残り手数を考慮した配分になることを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithMovesToGo_UsesShortHorizonBudget()
    {
        var limits = new LimitsType
        {
            MovesToGo = 10,
        };
        limits.SetTime(Color.BLACK, 3000);
        limits.SetIncrement(Color.BLACK, 300);
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.AreEqual(600, sut.OptimumTimeMs);
        Assert.AreEqual(600, sut.MaximumTimeMs);
    }

    /// <summary>
    /// 安全マージンが短時間帯で適用されることを検証する。
    /// </summary>
    [TestMethod]
    public void Init_WithLowRemainTime_AppliesSafetyMargin()
    {
        var limits = new LimitsType();
        limits.SetTime(Color.BLACK, 100);
        var sut = new TimeManagement();

        sut.Init(limits, Color.BLACK);

        Assert.IsTrue(sut.MaximumTimeMs <= 90);
    }
}
