using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Bench;

namespace YaneuraOu.CSharp.Engine.Tests.Bench;

/// <summary>
/// ベンチ実行の基本動作を検証するテストクラス。
/// </summary>
[TestClass]
public class BenchRunnerTests
{
    /// <summary>
    /// ベンチ実行でノード数が正になることを検証する。
    /// </summary>
    [TestMethod]
    public void Run_DefaultSettings_ProducesPositiveNodes()
    {
        var runner = new BenchRunner();

        BenchResult result = runner.Run(depth: 1, iterations: 2);

        Assert.IsTrue(result.Nodes > 0);
        Assert.IsTrue(result.ElapsedMilliseconds >= 0);
        Assert.IsTrue(result.NodesPerSecond >= 0);
    }
}
