using System.Diagnostics;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Eval;
using YaneuraOu.CSharp.Engine.Search;

namespace YaneuraOu.CSharp.Engine.Bench;

/// <summary>
/// 簡易ベンチマークを実行するクラス。
/// </summary>
public sealed class BenchRunner
{
    /// <summary>
    /// 指定条件でベンチマークを実行する。
    /// </summary>
    public BenchResult Run(int depth, int iterations, IEvaluator? evaluator = null)
    {
        int safeDepth = Math.Max(1, depth);
        int safeIterations = Math.Max(1, iterations);

        long nodes = 0;
        var stopwatch = Stopwatch.StartNew();
        var searcher = new Searcher(evaluator);

        for (int i = 0; i < safeIterations; i++)
        {
            var position = new Position();
            position.set(Position.StartSfen, new StateInfo());
            SearchResult result = searcher.Search(position, new SearchLimits { Depth = safeDepth });
            nodes += result.Nodes;
        }

        stopwatch.Stop();
        long elapsedMs = Math.Max(1, stopwatch.ElapsedMilliseconds);
        long nps = nodes * 1000 / elapsedMs;
        return new BenchResult(nodes, elapsedMs, nps, safeDepth, safeIterations);
    }

    /// <summary>
    /// Material評価とNNUE評価のベンチ結果を比較する。
    /// </summary>
    public BenchComparisonResult RunNnueComparison(string evalFilePath, int depth, int iterations)
    {
        BenchResult material = Run(depth, iterations, new MaterialEvaluator());

        var loader = new NnueModelLoader();
        INnueBackend backend = loader.Load(evalFilePath);
        if (!backend.IsEnabled)
        {
            return new BenchComparisonResult(material, null, false);
        }

        IEvaluator nnueEvaluator = new NnueEvaluator(backend, new MaterialEvaluator());
        BenchResult nnue = Run(depth, iterations, nnueEvaluator);
        return new BenchComparisonResult(material, nnue, true);
    }
}

/// <summary>
/// ベンチマーク結果を表す値オブジェクト。
/// </summary>
public readonly record struct BenchResult(long Nodes, long ElapsedMilliseconds, long NodesPerSecond, int Depth, int Iterations);

/// <summary>
/// Material/NNUE比較ベンチ結果を表す値オブジェクト。
/// </summary>
public readonly record struct BenchComparisonResult(BenchResult Material, BenchResult? Nnue, bool NnueEnabled);
