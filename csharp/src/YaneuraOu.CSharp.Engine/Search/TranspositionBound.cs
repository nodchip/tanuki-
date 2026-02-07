namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 置換表エントリの境界種別を表す列挙型。
/// </summary>
public enum TranspositionBound
{
    /// <summary>
    /// 完全一致値。
    /// </summary>
    Exact,

    /// <summary>
    /// 下限値(alpha-betaのfail-high)。
    /// </summary>
    Lower,

    /// <summary>
    /// 上限値(alpha-betaのfail-low)。
    /// </summary>
    Upper,
}
