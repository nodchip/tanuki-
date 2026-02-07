using YaneuraOu.CSharp.Engine.Bench;
using YaneuraOu.CSharp.Engine.Tools;
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
        if (args.Length > 0 && string.Equals(args[0], "parity-sfen", StringComparison.OrdinalIgnoreCase))
        {
            RunParitySfen(args);
            return;
        }

        if (args.Length > 0 && string.Equals(args[0], "bench", StringComparison.OrdinalIgnoreCase))
        {
            RunBench(args);
            return;
        }

        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.OutputSink = Console.WriteLine;

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
        if (args.Length >= 2 && string.Equals(args[1], "nnue", StringComparison.OrdinalIgnoreCase))
        {
            RunNnueBench(args);
            return;
        }

        int depth = args.Length >= 2 && int.TryParse(args[1], out int parsedDepth) ? parsedDepth : 3;
        int iterations = args.Length >= 3 && int.TryParse(args[2], out int parsedIterations) ? parsedIterations : 4;

        var runner = new BenchRunner();
        BenchResult result = runner.Run(depth, iterations);
        Console.WriteLine($"info string bench depth {result.Depth} iterations {result.Iterations} nodes {result.Nodes} time {result.ElapsedMilliseconds} nps {result.NodesPerSecond}");
    }

    /// <summary>
    /// NNUE比較ベンチを実行する。
    /// </summary>
    private static void RunNnueBench(string[] args)
    {
        if (args.Length < 3)
        {
            Console.WriteLine("info string usage: bench nnue <EvalFile> [depth] [iterations]");
            return;
        }

        string evalFile = args[2];
        int depth = args.Length >= 4 && int.TryParse(args[3], out int parsedDepth) ? parsedDepth : 3;
        int iterations = args.Length >= 5 && int.TryParse(args[4], out int parsedIterations) ? parsedIterations : 4;

        var runner = new BenchRunner();
        BenchComparisonResult result = runner.RunNnueComparison(evalFile, depth, iterations);

        BenchResult material = result.Material;
        Console.WriteLine($"info string bench material depth {material.Depth} iterations {material.Iterations} nodes {material.Nodes} time {material.ElapsedMilliseconds} nps {material.NodesPerSecond}");

        if (!result.NnueEnabled || result.Nnue is null)
        {
            Console.WriteLine($"info string bench nnue disabled evalfile {evalFile}");
            return;
        }

        BenchResult nnue = result.Nnue.Value;
        Console.WriteLine($"info string bench nnue depth {nnue.Depth} iterations {nnue.Iterations} nodes {nnue.Nodes} time {nnue.ElapsedMilliseconds} nps {nnue.NodesPerSecond}");

        double ratio = material.NodesPerSecond > 0 ? (double)nnue.NodesPerSecond / material.NodesPerSecond : 0.0;
        Console.WriteLine($"info string bench nnue/material nps_ratio {ratio:F3}");
    }

    /// <summary>
    /// NNUE parity用のSFEN一覧を生成する。
    /// </summary>
    private static void RunParitySfen(string[] args)
    {
        int count = args.Length >= 2 && int.TryParse(args[1], out int parsedCount) ? parsedCount : 40;
        int seed = args.Length >= 3 && int.TryParse(args[2], out int parsedSeed) ? parsedSeed : 20260207;
        int minPlies = args.Length >= 4 && int.TryParse(args[3], out int parsedMinPlies) ? parsedMinPlies : 8;
        int maxPlies = args.Length >= 5 && int.TryParse(args[4], out int parsedMaxPlies) ? parsedMaxPlies : 40;
        string outFile = args.Length >= 6 ? args[5] : "eval/nnue-parity-sfens.txt";

        var sampler = new SfenSampler();
        IReadOnlyList<SfenSample> samples = sampler.GenerateAnnotated(count, seed, minPlies, maxPlies);
        IReadOnlyList<string> sfens = samples.Select(s => s.Sfen).ToList();
        int kingMoveCount = samples.Count(s => (s.Features & SfenSampleFeatures.KingMove) != 0);
        int promotionCount = samples.Count(s => (s.Features & SfenSampleFeatures.Promotion) != 0);
        int dropCount = samples.Count(s => (s.Features & SfenSampleFeatures.Drop) != 0);

        string? outDir = Path.GetDirectoryName(outFile);
        if (!string.IsNullOrEmpty(outDir))
        {
            Directory.CreateDirectory(outDir);
        }

        File.WriteAllLines(outFile, sfens);
        Console.WriteLine($"info string parity-sfen generated count {sfens.Count} seed {seed} minPlies {minPlies} maxPlies {maxPlies} out {outFile}");
        Console.WriteLine($"info string parity-sfen coverage kingmove {kingMoveCount} promotion {promotionCount} drop {dropCount}");
    }
}
