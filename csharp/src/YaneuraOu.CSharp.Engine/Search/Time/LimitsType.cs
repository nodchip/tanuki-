using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search.Time;

/// <summary>
/// USIのgoコマンドから構築される探索制約を表すクラス。
/// </summary>
public sealed class LimitsType
{
    /// <summary>
    /// 先手/後手の残り時間(ミリ秒)を保持する。
    /// </summary>
    public int[] TimeMs { get; } = new int[2];

    /// <summary>
    /// 先手/後手の加算時間(ミリ秒)を保持する。
    /// </summary>
    public int[] IncMs { get; } = new int[2];

    /// <summary>
    /// 秒読み(ミリ秒)を保持する。
    /// </summary>
    public int ByoyomiMs { get; set; }

    /// <summary>
    /// 固定思考時間(ミリ秒)を保持する。
    /// </summary>
    public int MoveTimeMs { get; set; }

    /// <summary>
    /// 探索深さ上限を保持する。
    /// </summary>
    public int Depth { get; set; }

    /// <summary>
    /// ノード数上限を保持する。0以下は無制限。
    /// </summary>
    public long Nodes { get; set; }

    /// <summary>
    /// mate探索指定値を保持する。0以下は未指定。
    /// </summary>
    public int Mate { get; set; }

    /// <summary>
    /// 無制限探索かどうかを保持する。
    /// </summary>
    public bool Infinite { get; set; }

    /// <summary>
    /// ponder探索かどうかを保持する。
    /// </summary>
    public bool Ponder { get; set; }

    /// <summary>
    /// 探索開始時刻を保持する。
    /// </summary>
    public DateTimeOffset StartTime { get; set; } = DateTimeOffset.UtcNow;

    /// <summary>
    /// 先手/後手の残り時間を設定する。
    /// </summary>
    public void SetTime(Color side, int milliseconds)
    {
        TimeMs[(int)side] = Math.Max(0, milliseconds);
    }

    /// <summary>
    /// 先手/後手の加算時間を設定する。
    /// </summary>
    public void SetIncrement(Color side, int milliseconds)
    {
        IncMs[(int)side] = Math.Max(0, milliseconds);
    }
}
