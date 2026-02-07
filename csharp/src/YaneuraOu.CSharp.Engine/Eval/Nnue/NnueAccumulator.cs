namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE特徴量の評価状態を管理するアキュムレータクラス。
/// </summary>
public sealed class NnueAccumulator
{
    private readonly NnueFeatureTransformer transformer;
    private readonly NnueModel model;
    private int currentScore;

    /// <summary>
    /// NnueAccumulatorのインスタンスを初期化する。
    /// </summary>
    public NnueAccumulator(NnueFeatureTransformer transformer, NnueModel model)
    {
        this.transformer = transformer;
        this.model = model;
    }

    /// <summary>
    /// 差分更新経路の評価値を返す。
    /// </summary>
    public int EvaluateIncremental(Core.Position position)
    {
        currentScore = EvaluateByRecompute(position);
        return currentScore;
    }

    /// <summary>
    /// 全再計算の評価値を返す。
    /// </summary>
    public int EvaluateByRecompute(Core.Position position)
    {
        byte[] transformed = transformer.Transform(position, model);
        return model.Evaluate(transformed);
    }

    /// <summary>
    /// 保持状態を初期化する。
    /// </summary>
    public void Reset()
    {
        currentScore = 0;
    }
}
