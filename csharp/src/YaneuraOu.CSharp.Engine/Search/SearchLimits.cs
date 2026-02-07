namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 探索制限を表す設定クラス。
/// </summary>
public sealed class SearchLimits
{
    /// <summary>
    /// 探索深さ上限。
    /// </summary>
    public int Depth { get; set; } = 1;
}
