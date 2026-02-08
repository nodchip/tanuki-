using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search.Time;

/// <summary>
/// やねうら王C++版に寄せた時間管理を行うクラス。
/// </summary>
public sealed class TimeManagement
{
    private const int MoveHorizon = 30;

    private int minimumThinkingTimeMs;
    private int networkDelayMs;
    private int remainTimeMs;
    private bool roundUpToFullSecond;

    /// <summary>
    /// 最小思考時間(ミリ秒)を返す。
    /// </summary>
    public int MinimumTimeMs { get; private set; }

    /// <summary>
    /// 最適思考時間(ミリ秒)を返す。
    /// </summary>
    public int OptimumTimeMs { get; private set; }

    /// <summary>
    /// 最大思考時間(ミリ秒)を返す。
    /// </summary>
    public int MaximumTimeMs { get; private set; }

    /// <summary>
    /// ponderhit受理時刻を返す。
    /// </summary>
    public DateTimeOffset? PonderHitTime { get; private set; }

    /// <summary>
    /// フォールバックを使用したかどうかを返す。
    /// </summary>
    public bool FallbackUsed { get; private set; }

    /// <summary>
    /// 直近計算の要約文字列を返す。
    /// </summary>
    public string LastSummary { get; private set; } = "tm not initialized";

    /// <summary>
    /// 時間管理を初期化する。
    /// </summary>
    public void Init(LimitsType limits, Color sideToMove)
    {
        PonderHitTime = limits.StartTime;
        FallbackUsed = false;

        try
        {
            ComputeCxxStyle(limits, sideToMove);
        }
        catch (Exception ex)
        {
            ApplyFallback(limits, sideToMove, $"exception:{ex.GetType().Name}");
            return;
        }

        if (MaximumTimeMs <= 0)
        {
            ApplyFallback(limits, sideToMove, "non_positive_maximum");
            return;
        }

        LastSummary = BuildSummary(limits, sideToMove, "ok");
    }

    /// <summary>
    /// ponderhit受理時刻を記録する。
    /// </summary>
    public void NotifyPonderHit()
    {
        PonderHitTime = DateTimeOffset.UtcNow;
    }

    /// <summary>
    /// C++版相当の式で思考時間を計算する。
    /// </summary>
    private void ComputeCxxStyle(LimitsType limits, Color sideToMove)
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
            int fixedTime = Math.Max(1, limits.MoveTimeMs);
            MinimumTimeMs = fixedTime;
            OptimumTimeMs = fixedTime;
            MaximumTimeMs = fixedTime;
            return;
        }

        int us = (int)sideToMove;
        int timeMs = limits.TimeMs[us];
        int incMs = limits.IncMs[us];
        int byoyomiMs = limits.ByoyomiMs;

        roundUpToFullSecond = limits.RoundUpToFullSecond;
        minimumThinkingTimeMs = Math.Max(1, limits.MinimumThinkingTimeMs);
        networkDelayMs = Math.Max(0, limits.NetworkDelayMs);

        int rawRemain = timeMs + incMs + byoyomiMs - Math.Max(0, limits.NetworkDelay2Ms) - Math.Max(0, limits.MoveOverheadMs);
        if (rawRemain <= 0)
        {
            throw new InvalidOperationException("budget_underflow");
        }

        remainTimeMs = rawRemain;
        remainTimeMs = Math.Max(remainTimeMs, roundUpToFullSecond ? 100 : 1);

        bool timeForfeit = incMs == 0 && byoyomiMs == 0;
        int ply = Math.Max(1, limits.GamePly);
        int maxMovesToDraw = Math.Max(32, limits.MaxMovesToDraw);

        int moveHorizon = timeForfeit
            ? MoveHorizon + 40 - Math.Min(ply, 40)
            : MoveHorizon + 20 - Math.Min(ply, 80);

        int mtg = Math.Min(maxMovesToDraw - ply + 2, moveHorizon) / 2;

        if (mtg <= 0)
        {
            MinimumTimeMs = 500;
            OptimumTimeMs = 500;
            MaximumTimeMs = 500;
            return;
        }

        if (mtg == 1)
        {
            MinimumTimeMs = remainTimeMs;
            OptimumTimeMs = remainTimeMs;
            MaximumTimeMs = remainTimeMs;
            return;
        }

        MinimumTimeMs = Math.Max(minimumThinkingTimeMs - networkDelayMs, roundUpToFullSecond ? 1000 : 1);
        OptimumTimeMs = remainTimeMs;
        MaximumTimeMs = remainTimeMs;

        long remainEstimate = (long)timeMs + (long)incMs * mtg + (long)byoyomiMs * mtg;
        remainEstimate -= (long)(mtg + 1) * 1000L;
        remainEstimate = Math.Max(0L, remainEstimate);

        int t1 = MinimumTimeMs + (int)(remainEstimate / mtg);

        double maxRatio = 5.0;
        if (timeForfeit)
        {
            maxRatio = Math.Min(maxRatio, Math.Max(timeMs / 60000.0, 1.0));
        }

        int t2 = MinimumTimeMs + (int)(remainEstimate * maxRatio / mtg);
        t2 = Math.Min(t2, (int)(remainEstimate * 0.3));

        int slowMover = Math.Max(1, limits.SlowMover);
        OptimumTimeMs = Math.Min(t1, OptimumTimeMs) * slowMover / 100;
        MaximumTimeMs = Math.Min(t2, MaximumTimeMs);

        if (limits.PonderEnabledOption)
        {
            OptimumTimeMs += OptimumTimeMs / 4;
        }

        if (byoyomiMs > 0 && timeMs < (int)(byoyomiMs * 1.2))
        {
            int finalPush = byoyomiMs + timeMs;
            MinimumTimeMs = finalPush;
            OptimumTimeMs = finalPush;
            MaximumTimeMs = finalPush;
        }

        MinimumTimeMs = Math.Min(RoundUp(MinimumTimeMs), remainTimeMs);
        OptimumTimeMs = Math.Min(OptimumTimeMs, remainTimeMs);
        MaximumTimeMs = Math.Min(RoundUp(MaximumTimeMs), remainTimeMs);

    }

    /// <summary>
    /// 秒単位切り上げと遅延補正を行う。
    /// </summary>
    private int RoundUp(int valueMs)
    {
        if (roundUpToFullSecond)
        {
            int rounded = Math.Max(((valueMs + 999) / 1000) * 1000, minimumThinkingTimeMs);
            rounded -= networkDelayMs;
            if (rounded < valueMs)
            {
                rounded += 1000;
            }

            return Math.Min(rounded, remainTimeMs);
        }

        int adjusted = Math.Max(valueMs, minimumThinkingTimeMs);
        adjusted -= networkDelayMs;
        return Math.Min(adjusted, remainTimeMs);
    }

    /// <summary>
    /// 保守的固定時間へのフォールバックを適用する。
    /// </summary>
    private void ApplyFallback(LimitsType limits, Color sideToMove, string reason)
    {
        FallbackUsed = true;

        int byoyomi = Math.Max(0, limits.ByoyomiMs);
        int fallback = byoyomi > 0 ? Math.Min(byoyomi, 1000) : 1000;
        fallback = Math.Max(1, fallback);

        if (limits.Infinite)
        {
            fallback = 0;
        }

        MinimumTimeMs = fallback;
        OptimumTimeMs = fallback;
        MaximumTimeMs = fallback;

        LastSummary = BuildSummary(limits, sideToMove, $"fallback:{reason}");
    }

    /// <summary>
    /// デバッグ用の要約文字列を生成する。
    /// </summary>
    private string BuildSummary(LimitsType limits, Color sideToMove, string status)
    {
        int us = (int)sideToMove;
        return $"tm status={status} side={(sideToMove == Color.BLACK ? "b" : "w")} btime={limits.TimeMs[(int)Color.BLACK]} wtime={limits.TimeMs[(int)Color.WHITE]} binc={limits.IncMs[(int)Color.BLACK]} winc={limits.IncMs[(int)Color.WHITE]} byoyomi={limits.ByoyomiMs} movetime={limits.MoveTimeMs} movestogo={limits.MovesToGo} ply={limits.GamePly} opt={OptimumTimeMs} max={MaximumTimeMs} min={MinimumTimeMs} remain={(limits.TimeMs[us] + limits.IncMs[us] + limits.ByoyomiMs)} fallback={(FallbackUsed ? "true" : "false")}";
    }
}
