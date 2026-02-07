using YaneuraOu.CSharp.Engine.Search.Time;

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

    /// <summary>
    /// ノード数上限。0以下の場合は無制限。
    /// </summary>
    public long NodesLimit { get; set; }

    /// <summary>
    /// 探索スレッド数。現実装では1を推奨。
    /// </summary>
    public int Threads { get; set; } = 1;

    /// <summary>
    /// 外部停止要求を返すコールバック。
    /// </summary>
    public Func<bool>? ShouldStop { get; set; }

    /// <summary>
    /// 停止判定ポリシー。
    /// </summary>
    public SearchStopPolicy? StopPolicy { get; set; }

    /// <summary>
    /// go mateで指定された詰み手数を保持する。0以下は未指定。
    /// </summary>
    public int MateMoves { get; set; }
}
