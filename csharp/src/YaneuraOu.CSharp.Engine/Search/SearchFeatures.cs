namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 探索機能の有効/無効を保持する設定クラス。
/// </summary>
public sealed class SearchFeatures
{
    /// <summary>
    /// Null Move Pruningを有効化するかどうか。
    /// </summary>
    public bool EnableNullMovePruning { get; set; } = true;

    /// <summary>
    /// LMRを有効化するかどうか。
    /// </summary>
    public bool EnableLmr { get; set; } = true;

    /// <summary>
    /// Aspiration Windowを有効化するかどうか。
    /// </summary>
    public bool EnableAspirationWindow { get; set; } = true;
}
