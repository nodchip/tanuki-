using System.Diagnostics;

namespace YaneuraOu.CSharp.Engine.Search.Time;

/// <summary>
/// 探索停止判定を行うポリシークラス。
/// </summary>
public sealed class SearchStopPolicy
{
    private readonly Stopwatch stopwatch = Stopwatch.StartNew();
    private readonly TimeManagement timeManagement;
    private readonly bool infinite;
    private readonly bool ponder;
    private readonly long nodeLimit;
    private Func<bool>? externalStop;
    private long ponderHitBaselineMs = -1;

    /// <summary>
    /// SearchStopPolicyのインスタンスを初期化する。
    /// </summary>
    public SearchStopPolicy(TimeManagement timeManagement, bool infinite, long nodeLimit, Func<bool>? externalStop, bool ponder = false)
    {
        this.timeManagement = timeManagement;
        this.infinite = infinite;
        this.nodeLimit = nodeLimit;
        this.externalStop = externalStop;
        this.ponder = ponder;
    }

    /// <summary>
    /// 外部停止条件を設定する。
    /// </summary>
    public void SetExternalStop(Func<bool>? externalStop)
    {
        this.externalStop = externalStop;
    }

    /// <summary>
    /// 停止理由を返す。
    /// </summary>
    public string LastReason { get; private set; } = "none";

    /// <summary>
    /// 実効経過時間(ミリ秒)を返す。
    /// </summary>
    public int ElapsedMilliseconds => GetEffectiveElapsedMilliseconds();

    /// <summary>
    /// 最大探索時間(ミリ秒)を返す。
    /// </summary>
    public int MaximumTimeMs => timeManagement.MaximumTimeMs;

    /// <summary>
    /// depth完了時点で停止すべきかどうかを判定する。
    /// </summary>
    public bool ShouldStopAfterCompletedDepth(long currentNodes)
    {
        if (ShouldStopNow(currentNodes))
        {
            return true;
        }

        if (infinite)
        {
            return false;
        }

        if (ponder && timeManagement.PonderHitTime is null)
        {
            return false;
        }

        if (timeManagement.OptimumTimeMs > 0 && GetEffectiveElapsedMilliseconds() >= timeManagement.OptimumTimeMs)
        {
            LastReason = "optimum_time";
            return true;
        }

        return false;
    }

    /// <summary>
    /// 探索中に即時停止すべきかどうかを判定する。
    /// </summary>
    public bool ShouldStopNow(long currentNodes)
    {
        if (externalStop is not null && externalStop())
        {
            LastReason = "external_stop";
            return true;
        }

        if (nodeLimit > 0 && currentNodes >= nodeLimit)
        {
            LastReason = "nodes";
            return true;
        }

        if (ponder && timeManagement.PonderHitTime is null)
        {
            return false;
        }

        if (!infinite && timeManagement.MaximumTimeMs > 0 && GetEffectiveElapsedMilliseconds() >= timeManagement.MaximumTimeMs)
        {
            LastReason = "maximum_time";
            return true;
        }

        return false;
    }

    /// <summary>
    /// ponderhitを考慮した実効経過時間を返す。
    /// </summary>
    private int GetEffectiveElapsedMilliseconds()
    {
        long elapsed = stopwatch.ElapsedMilliseconds;
        if (!ponder)
        {
            return (int)elapsed;
        }

        if (timeManagement.PonderHitTime is null)
        {
            return 0;
        }

        if (ponderHitBaselineMs < 0)
        {
            ponderHitBaselineMs = elapsed;
            return 0;
        }

        long effective = elapsed - ponderHitBaselineMs;
        return (int)Math.Max(0, effective);
    }
}
