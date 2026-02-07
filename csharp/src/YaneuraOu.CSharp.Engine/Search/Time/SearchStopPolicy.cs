using System.Diagnostics;

namespace YaneuraOu.CSharp.Engine.Search.Time;

/// <summary>
/// 探索停止判定を担当するポリシークラス。
/// </summary>
public sealed class SearchStopPolicy
{
    private readonly Stopwatch stopwatch = Stopwatch.StartNew();
    private readonly TimeManagement timeManagement;
    private readonly bool infinite;
    private readonly long nodeLimit;
    private Func<bool>? externalStop;

    /// <summary>
    /// SearchStopPolicyのインスタンスを初期化する。
    /// </summary>
    public SearchStopPolicy(TimeManagement timeManagement, bool infinite, long nodeLimit, Func<bool>? externalStop)
    {
        this.timeManagement = timeManagement;
        this.infinite = infinite;
        this.nodeLimit = nodeLimit;
        this.externalStop = externalStop;
    }

    /// <summary>
    /// 外部停止コールバックを設定する。
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
    /// 経過時間(ミリ秒)を返す。
    /// </summary>
    public int ElapsedMilliseconds => (int)stopwatch.ElapsedMilliseconds;

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

        if (timeManagement.OptimumTimeMs > 0 && ElapsedMilliseconds >= timeManagement.OptimumTimeMs)
        {
            LastReason = "optimum_time";
            return true;
        }

        return false;
    }

    /// <summary>
    /// 探索中に即停止すべきかどうかを判定する。
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

        if (!infinite && timeManagement.MaximumTimeMs > 0 && ElapsedMilliseconds >= timeManagement.MaximumTimeMs)
        {
            LastReason = "maximum_time";
            return true;
        }

        return false;
    }
}
