using System.Diagnostics;
using YaneuraOu.CSharp.Engine.Core;
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
    public BenchResult Run(int depth, int iterations)
    {
        int safeDepth = Math.Max(1, depth);
        int safeIterations = Math.Max(1, iterations);

        long nodes = 0;
        var stopwatch = Stopwatch.StartNew();
        var searcher = new Searcher();

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
}

/// <summary>
/// ベンチマーク結果を表す値オブジェクト。
/// </summary>
public readonly record struct BenchResult(long Nodes, long ElapsedMilliseconds, long NodesPerSecond, int Depth, int Iterations);
