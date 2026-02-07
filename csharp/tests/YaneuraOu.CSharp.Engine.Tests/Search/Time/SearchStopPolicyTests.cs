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
}
