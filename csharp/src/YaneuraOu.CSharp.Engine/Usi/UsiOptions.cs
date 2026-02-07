namespace YaneuraOu.CSharp.Engine.Usi;

/// <summary>
/// USIオプションを保持するクラス。
/// </summary>
public sealed class UsiOptions
{
    /// <summary>
    /// 既定の探索深さ。
    /// </summary>
    public int DefaultDepth { get; set; } = 1;

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
}
