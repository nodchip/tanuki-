namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 探索制約を表す設定クラス。
/// </summary>
public sealed class SearchLimits
{
    /// <summary>
    /// 探索深さの上限。
    /// </summary>
    public int Depth { get; set; } = 1;

    /// <summary>
    /// 探索時間の上限(ミリ秒)。0以下の場合は無制限。
    /// </summary>
    public int MaxTimeMs { get; set; }
}
