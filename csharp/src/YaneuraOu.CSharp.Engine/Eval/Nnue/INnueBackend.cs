using YaneuraOu.CSharp.Engine.Core;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEバックエンドの抽象インターフェース。
/// </summary>
public interface INnueBackend
{
    /// <summary>
    /// NNUEが有効かどうかを返す。
    /// </summary>
    bool IsEnabled { get; }

    /// <summary>
    /// 指定局面のNNUE評価値を返す。
    /// </summary>
    int Evaluate(Position position);
}
