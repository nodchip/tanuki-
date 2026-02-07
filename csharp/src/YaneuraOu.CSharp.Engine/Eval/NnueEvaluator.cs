using YaneuraOu.CSharp.Engine.Core;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE優先で評価し、未使用時はフォールバック評価を返す評価器。
/// </summary>
public sealed class NnueEvaluator : IEvaluator
{
    private readonly INnueBackend backend;
    private readonly IEvaluator fallback;

    /// <summary>
    /// NnueEvaluatorのインスタンスを初期化する。
    /// </summary>
    public NnueEvaluator(INnueBackend backend, IEvaluator fallback)
    {
        this.backend = backend;
        this.fallback = fallback;
    }

    /// <summary>
    /// 指定局面を手番側視点で評価する。
    /// </summary>
    public int Evaluate(Position position)
    {
        if (backend.IsEnabled)
        {
            return backend.Evaluate(position);
        }

        return fallback.Evaluate(position);
    }
}
