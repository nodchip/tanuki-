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

    /// <summary>
    /// 無効なEvalFile指定ではNNUE比較結果が無効化されることを検証する。
    /// </summary>
    [TestMethod]
    public void RunNnueComparison_InvalidPath_DisablesNnue()
    {
        var runner = new BenchRunner();

        BenchComparisonResult result = runner.RunNnueComparison("C:/not-found/nn.bin", depth: 1, iterations: 1);

        Assert.IsFalse(result.NnueEnabled);
        Assert.IsNull(result.Nnue);
        Assert.IsTrue(result.Material.Nodes > 0);
    }

    /// <summary>
    /// 有効なEvalFile指定でNNUE比較結果が返ることを検証する。
    /// </summary>
    [TestMethod]
    public void RunNnueComparison_ValidEvalFile_ProducesNnueResult()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            File.WriteAllText(modelPath, "42");
            var runner = new BenchRunner();

            BenchComparisonResult result = runner.RunNnueComparison(modelPath, depth: 1, iterations: 1);

            Assert.IsTrue(result.NnueEnabled);
            Assert.IsNotNull(result.Nnue);
            Assert.IsTrue(result.Material.Nodes > 0);
            Assert.IsTrue(result.Nnue!.Value.Nodes > 0);
        }
        finally
        {
            if (File.Exists(modelPath))
            {
                File.Delete(modelPath);
            }
        }
    }
}
