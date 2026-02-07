namespace YaneuraOu.CSharp.Engine.Usi;

/// <summary>
/// USIオプションを保持するクラス。
/// </summary>
public sealed class UsiOptions
{
    /// <summary>
    /// 既定の探索深さ。
    /// </summary>
    public int DefaultDepth { get; set; } = 64;

    /// <summary>
    /// 既定の思考時間(ミリ秒)。
    /// </summary>
    public int DefaultMoveTimeMs { get; set; } = 1000;

    /// <summary>
    /// 既定の探索スレッド数。
    /// </summary>
    public int Threads { get; set; } = 1;

    /// <summary>
    /// NNUEモデルファイルのパス。
    /// </summary>
    public string EvalFilePath { get; set; } = string.Empty;

    /// <summary>
    /// 既定のハッシュサイズ(MB)。
    /// </summary>
    public int HashSizeMb { get; set; } = 64;

    /// <summary>
    /// ponderを有効化するかどうか。
    /// </summary>
    public bool PonderEnabled { get; set; }

    /// <summary>
    /// MultiPV数。
    /// </summary>
    public int MultiPv { get; set; } = 1;

    /// <summary>
    /// 解析モードかどうか。
    /// </summary>
    public bool AnalyseMode { get; set; }

    /// <summary>
    /// デバッグログ出力を有効化するかどうか。
    /// </summary>
    public bool DebugLog { get; set; }
}
