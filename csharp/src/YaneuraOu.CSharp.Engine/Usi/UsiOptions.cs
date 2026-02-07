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
    /// 1手あたりの固定オーバーヘッド(ミリ秒)。
    /// </summary>
    public int MoveOverheadMs { get; set; }

    /// <summary>
    /// 最小思考時間(ミリ秒)。
    /// </summary>
    public int MinimumThinkingTimeMs { get; set; } = 2000;

    /// <summary>
    /// 序盤重視率(百分率)。
    /// </summary>
    public int SlowMover { get; set; } = 100;

    /// <summary>
    /// 秒未満を1秒単位に切り上げるかどうか。
    /// </summary>
    public bool RoundUpToFullSecond { get; set; }

    /// <summary>
    /// 通常時のネットワーク遅延見込み(ミリ秒)。
    /// </summary>
    public int NetworkDelayMs { get; set; }

    /// <summary>
    /// 切れ負け回避用の最大ネットワーク遅延見込み(ミリ秒)。
    /// </summary>
    public int NetworkDelay2Ms { get; set; }

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

    /// <summary>
    /// NNUE増分評価の厳密照合を有効化するかどうか。
    /// </summary>
    public bool NnueIncrementalStrict { get; set; }

    /// <summary>
    /// Null Move Pruningを有効化するかどうか。
    /// </summary>
    public bool UseNullMovePruning { get; set; } = true;

    /// <summary>
    /// LMRを有効化するかどうか。
    /// </summary>
    public bool UseLmr { get; set; } = true;

    /// <summary>
    /// Aspiration Windowを有効化するかどうか。
    /// </summary>
    public bool UseAspirationWindow { get; set; } = true;
}
