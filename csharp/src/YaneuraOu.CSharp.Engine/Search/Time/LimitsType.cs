using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search.Time;

/// <summary>
/// USI縺ｮgo繧ｳ繝槭Φ繝峨°繧画ｧ狗ｯ峨＆繧後ｋ謗｢邏｢蛻ｶ邏・ｒ陦ｨ縺吶け繝ｩ繧ｹ縲・/// </summary>
public sealed class LimitsType
{
    /// <summary>
    /// 蜈域焔/蠕梧焔縺ｮ谿九ｊ譎る俣(繝溘Μ遘・繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public int[] TimeMs { get; } = new int[2];

    /// <summary>
    /// 蜈域焔/蠕梧焔縺ｮ蜉邂玲凾髢・繝溘Μ遘・繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public int[] IncMs { get; } = new int[2];

    /// <summary>
    /// 遘定ｪｭ縺ｿ(繝溘Μ遘・繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public int ByoyomiMs { get; set; }

    /// <summary>
    /// 1手ごとの固定オーバーヘッド(ミリ秒)を保持する。
    /// </summary>
    public int MoveOverheadMs { get; set; }

    /// <summary>
    /// 最小思考時間(ミリ秒)を保持する。
    /// </summary>
    public int MinimumThinkingTimeMs { get; set; } = 2000;

    /// <summary>
    /// 序盤重視率(百分率)を保持する。
    /// </summary>
    public int SlowMover { get; set; } = 100;

    /// <summary>
    /// 秒未満の思考時間を1秒単位へ切り上げるかどうか。
    /// </summary>
    public bool RoundUpToFullSecond { get; set; }

    /// <summary>
    /// 蝗ｺ螳壽晁・凾髢・繝溘Μ遘・繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public int MoveTimeMs { get; set; }

    /// <summary>
    /// 謗｢邏｢豺ｱ縺穂ｸ企剞繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public int Depth { get; set; }

    /// <summary>
    /// 繝弱・繝画焚荳企剞繧剃ｿ晄戟縺吶ｋ縲・莉･荳九・辟｡蛻ｶ髯舌・    /// </summary>
    public long Nodes { get; set; }

    /// <summary>
    /// mate謗｢邏｢謖・ｮ壼､繧剃ｿ晄戟縺吶ｋ縲・莉･荳九・譛ｪ謖・ｮ壹・    /// </summary>
    public int Mate { get; set; }

    /// <summary>
    /// 残り手数指定を保持する。0以下は未指定。
    /// </summary>
    public int MovesToGo { get; set; }

    /// <summary>
    /// 辟｡蛻ｶ髯先爾邏｢縺九←縺・°繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public bool Infinite { get; set; }

    /// <summary>
    /// ponder謗｢邏｢縺九←縺・°繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public bool Ponder { get; set; }

    /// <summary>
    /// 謗｢邏｢髢句ｧ区凾蛻ｻ繧剃ｿ晄戟縺吶ｋ縲・    /// </summary>
    public DateTimeOffset StartTime { get; set; } = DateTimeOffset.UtcNow;

    /// <summary>
    /// 蜈域焔/蠕梧焔縺ｮ谿九ｊ譎る俣繧定ｨｭ螳壹☆繧九・    /// </summary>
    public void SetTime(Color side, int milliseconds)
    {
        TimeMs[(int)side] = Math.Max(0, milliseconds);
    }

    /// <summary>
    /// 蜈域焔/蠕梧焔縺ｮ蜉邂玲凾髢薙ｒ險ｭ螳壹☆繧九・    /// </summary>
    public void SetIncrement(Color side, int milliseconds)
    {
        IncMs[(int)side] = Math.Max(0, milliseconds);
    }
}

