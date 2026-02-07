namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEモデルヘッダのメタ情報を表すクラス。
/// </summary>
public sealed class NnueModelMetadata
{
    /// <summary>
    /// ヘッダに記録されたバージョン値。
    /// </summary>
    public uint Version { get; init; }

    /// <summary>
    /// ヘッダに記録されたハッシュ値。
    /// </summary>
    public uint HashValue { get; init; }

    /// <summary>
    /// ヘッダに記録されたアーキテクチャ文字列。
    /// </summary>
    public string Architecture { get; init; } = string.Empty;

    /// <summary>
    /// ヘッダ領域のバイト長。
    /// </summary>
    public int HeaderByteLength { get; init; }
}
