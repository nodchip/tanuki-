using YaneuraOu.CSharp.Engine.Bench;

namespace YaneuraOu.CSharp.Tools.Bench;

/// <summary>
/// BenchRunnerを呼び出すCLIエントリの補助クラス。
/// </summary>
public static class BenchTool
{
    /// <summary>
    /// ベンチマークを実行して結果文字列を返す。
    /// </summary>
    public static string Run(int depth, int iterations)
    {
        var runner = new BenchRunner();
        BenchResult result = runner.Run(depth, iterations);
        return $"depth={result.Depth} iterations={result.Iterations} nodes={result.Nodes} time={result.ElapsedMilliseconds} nps={result.NodesPerSecond}";
    }
}
