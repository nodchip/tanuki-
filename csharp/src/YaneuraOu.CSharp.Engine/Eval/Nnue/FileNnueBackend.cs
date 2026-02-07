using YaneuraOu.CSharp.Engine.Core;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// ファイルから固定値を読む簡易NNUEバックエンド。
/// </summary>
public sealed class FileNnueBackend : INnueBackend
{
    private readonly int score;

    /// <summary>
    /// FileNnueBackendのインスタンスを初期化する。
    /// </summary>
    public FileNnueBackend(int score)
    {
        this.score = score;
    }

    /// <summary>
    /// NNUEが有効かどうかを返す。
    /// </summary>
    public bool IsEnabled => true;

    /// <summary>
    /// 固定評価値を返す。
    /// </summary>
    public int Evaluate(Position position)
    {
        return score;
    }
}
