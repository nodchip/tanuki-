using System.Threading;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Search.Time;

namespace YaneuraOu.CSharp.Engine.Tests.Search.Time;

/// <summary>
/// SearchStopPolicyの停止判定を検証するテストクラス。
/// </summary>
[TestClass]
public class SearchStopPolicyTests
{
    /// <summary>
    /// ノード上限で停止することを検証する。
    /// </summary>
    [TestMethod]
    public void ShouldStopNow_StopsOnNodeLimit()
    {
        var limits = new LimitsType
        {
            MoveTimeMs = 10000,
        };
        var tm = new TimeManagement();
        tm.Init(limits, Color.BLACK);
        var sut = new SearchStopPolicy(tm, infinite: false, nodeLimit: 10, externalStop: null);

        bool stopped = sut.ShouldStopNow(10);

        Assert.IsTrue(stopped);
        Assert.AreEqual("nodes", sut.LastReason);
    }

    /// <summary>
    /// 外部停止要求を優先することを検証する。
    /// </summary>
    [TestMethod]
    public void ShouldStopNow_PrioritizesExternalStop()
    {
        var limits = new LimitsType
        {
            MoveTimeMs = 10000,
        };
        var tm = new TimeManagement();
        tm.Init(limits, Color.BLACK);
        var sut = new SearchStopPolicy(tm, infinite: false, nodeLimit: 0, externalStop: () => true);

        bool stopped = sut.ShouldStopNow(0);

        Assert.IsTrue(stopped);
        Assert.AreEqual("external_stop", sut.LastReason);
    }

    /// <summary>
    /// 最適時間経過後にdepth境界で停止することを検証する。
    /// </summary>
    [TestMethod]
    public void ShouldStopAfterCompletedDepth_StopsOnOptimumTime()
    {
        var limits = new LimitsType
        {
            MoveTimeMs = 5,
        };
        var tm = new TimeManagement();
        tm.Init(limits, Color.BLACK);
        var sut = new SearchStopPolicy(tm, infinite: false, nodeLimit: 0, externalStop: null);
        Thread.Sleep(10);

        bool stopped = sut.ShouldStopAfterCompletedDepth(0);

        Assert.IsTrue(stopped);
        Assert.AreEqual("maximum_time", sut.LastReason);
    }

    /// <summary>
    /// ponder中はponderhitまで時間切れ停止しないことを検証する。
    /// </summary>
    [TestMethod]
    public void ShouldStopNow_PonderBeforeHit_DoesNotStopOnTime()
    {
        var limits = new LimitsType
        {
            MoveTimeMs = 5,
        };
        var tm = new TimeManagement();
        tm.Init(limits, Color.BLACK);
        var sut = new SearchStopPolicy(tm, infinite: false, nodeLimit: 0, externalStop: null, ponder: true);
        Thread.Sleep(15);

        bool stopped = sut.ShouldStopNow(0);

        Assert.IsFalse(stopped);
        Assert.AreEqual("none", sut.LastReason);
    }

    /// <summary>
    /// ponderhit後はponderhit時点からの経過時間で停止判定されることを検証する。
    /// </summary>
    [TestMethod]
    public void ShouldStopNow_PonderAfterHit_StopsOnTimeSinceHit()
    {
        var limits = new LimitsType
        {
            MoveTimeMs = 10,
        };
        var tm = new TimeManagement();
        tm.Init(limits, Color.BLACK);
        var sut = new SearchStopPolicy(tm, infinite: false, nodeLimit: 0, externalStop: null, ponder: true);
        Thread.Sleep(20);
        tm.NotifyPonderHit();
        _ = sut.ShouldStopNow(0);
        Thread.Sleep(15);

        bool stopped = sut.ShouldStopNow(0);

        Assert.IsTrue(stopped);
        Assert.AreEqual("maximum_time", sut.LastReason);
    }
}
