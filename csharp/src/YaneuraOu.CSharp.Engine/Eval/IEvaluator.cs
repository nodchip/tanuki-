using YaneuraOu.CSharp.Engine.Core;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// 局面評価器の共通インターフェース。
/// </summary>
public interface IEvaluator
{
    /// <summary>
    /// 指定局面を手番側視点で評価する。
    /// </summary>
    int Evaluate(Position position);
}
