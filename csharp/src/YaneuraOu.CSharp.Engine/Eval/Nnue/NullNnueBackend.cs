using YaneuraOu.CSharp.Engine.Core;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE未接続時に使用する無効バックエンド。
/// </summary>
public sealed class NullNnueBackend : INnueBackend
{
    /// <summary>
    /// NNUEが有効かどうかを返す。
    /// </summary>
    public bool IsEnabled => false;

    /// <summary>
    /// 未使用時の評価値を返す。
    /// </summary>
    public int Evaluate(Position position)
    {
        return 0;
    }
}
