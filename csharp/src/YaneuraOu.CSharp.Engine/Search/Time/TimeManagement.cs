using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search.Time;

/// <summary>
/// C++版の時間配分方針に寄せた時間管理クラス。
/// </summary>
public sealed class TimeManagement
{
    private const int MinimumSafetyMarginMs = 10;
    private const int MaximumSafetyMarginMs = 50;

    /// <summary>
    /// 最低保証時間(ミリ秒)を返す。
    /// </summary>
    public int MinimumTimeMs { get; private set; }

    /// <summary>
    /// 最適探索時間(ミリ秒)を返す。
    /// </summary>
    public int OptimumTimeMs { get; private set; }

    /// <summary>
    /// 最大探索時間(ミリ秒)を返す。
    /// </summary>
    public int MaximumTimeMs { get; private set; }

    /// <summary>
    /// ponderhit受理時刻を返す。
    /// </summary>
    public DateTimeOffset? PonderHitTime { get; private set; }

    /// <summary>
    /// 時間配分を初期化する。
    /// </summary>
    public void Init(LimitsType limits, Color sideToMove)
    {
        if (limits.Infinite)
        {
            MinimumTimeMs = 0;
            OptimumTimeMs = 0;
            MaximumTimeMs = 0;
            return;
        }

        if (limits.MoveTimeMs > 0)
        {
            int fixedMs = limits.MoveTimeMs;
            MinimumTimeMs = fixedMs;
            OptimumTimeMs = fixedMs;
            MaximumTimeMs = fixedMs;
            return;
        }

        int sideIndex = (int)sideToMove;
        int remain = limits.TimeMs[sideIndex];
        int inc = limits.IncMs[sideIndex];
        int byoyomi = limits.ByoyomiMs;
        int safetyMargin = ComputeSafetyMargin(remain, byoyomi);
        int available = Math.Max(0, remain + inc + byoyomi - safetyMargin);
        if (available <= 0)
        {
            MinimumTimeMs = 0;
            OptimumTimeMs = 0;
            MaximumTimeMs = 0;
            return;
        }

        int movesToGo = limits.MovesToGo > 0 ? limits.MovesToGo : 30;
        int baseTime = remain > 0 ? Math.Max(1, remain / Math.Max(1, movesToGo)) : 0;
        int reserve = remain > 0
            ? (limits.MovesToGo > 0 ? Math.Max(1, remain / Math.Max(1, limits.MovesToGo)) : Math.Max(1, remain / 8))
            : available;
        int optimum = baseTime + inc + byoyomi;
        if (optimum <= 0)
        {
            optimum = Math.Max(1, byoyomi > 0 ? byoyomi : available / 2);
        }

        MinimumTimeMs = Math.Max(1, optimum / 2);
        OptimumTimeMs = Math.Max(MinimumTimeMs, Math.Min(optimum, available));
        MaximumTimeMs = Math.Max(OptimumTimeMs, Math.Min(Math.Max(OptimumTimeMs, reserve), available));
    }

    /// <summary>
    /// 時間切れ回避のために控除する安全マージンを算出する。
    /// </summary>
    private static int ComputeSafetyMargin(int remain, int byoyomi)
    {
        if (remain <= 0 && byoyomi <= 0)
        {
            return 0;
        }

        int source = remain > 0 ? remain : byoyomi;
        int scaled = Math.Max(MinimumSafetyMarginMs, source / 40);
        return Math.Min(MaximumSafetyMarginMs, scaled);
    }

    /// <summary>
    /// ponderhit受理を通知する。
    /// </summary>
    public void NotifyPonderHit()
    {
        PonderHitTime = DateTimeOffset.UtcNow;
    }
}
