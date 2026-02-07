using YaneuraOu.CSharp.Engine.Bench;
using YaneuraOu.CSharp.Engine.Usi;

namespace YaneuraOu.CSharp.Engine;

/// <summary>
/// エントリーポイントを提供するクラス。
/// </summary>
public static class Program
{
    /// <summary>
    /// プログラムを開始する。
    /// </summary>
    public static void Main(string[] args)
    {
        if (args.Length > 0 && string.Equals(args[0], "bench", StringComparison.OrdinalIgnoreCase))
        {
            RunBench(args);
            return;
        }

        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        while (!engine.ShouldQuit)
        {
            string? line = Console.ReadLine();
            if (line is null)
            {
                break;
            }

            string response = engine.HandleCommand(line);
            if (string.IsNullOrEmpty(response))
            {
                continue;
            }

            string[] outputs = response.Split('\n', StringSplitOptions.RemoveEmptyEntries);
            foreach (string output in outputs)
            {
                Console.WriteLine(output);
            }
        }
    }

    /// <summary>
    /// ベンチコマンドを実行する。
    /// </summary>
    private static void RunBench(string[] args)
    {
        int depth = args.Length >= 2 && int.TryParse(args[1], out int parsedDepth) ? parsedDepth : 3;
        int iterations = args.Length >= 3 && int.TryParse(args[2], out int parsedIterations) ? parsedIterations : 4;

        var runner = new BenchRunner();
        BenchResult result = runner.Run(depth, iterations);
        Console.WriteLine($"info string bench depth {result.Depth} iterations {result.Iterations} nodes {result.Nodes} time {result.ElapsedMilliseconds} nps {result.NodesPerSecond}");
    }
}
