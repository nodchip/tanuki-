using YaneuraOu.CSharp.Engine.Core;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEモデルまたは固定値から評価値を返すバックエンド。
/// </summary>
public sealed class FileNnueBackend : INnueBackend
{
    private readonly bool useFixedScore;
    private readonly int fixedScore;
    private readonly NnueAccumulator? accumulator;

    /// <summary>
    /// 固定評価値モードのインスタンスを初期化する。
    /// </summary>
    public FileNnueBackend(int score)
    {
        useFixedScore = true;
        fixedScore = score;
    }

    /// <summary>
    /// バイナリモデルモードのインスタンスを初期化する。
    /// </summary>
    public FileNnueBackend(byte[] modelBytes)
    {
        useFixedScore = false;
        accumulator = new NnueAccumulator(new NnueFeatureTransformer(), modelBytes);
    }

    /// <summary>
    /// NNUEが有効かどうかを返す。
    /// </summary>
    public bool IsEnabled => true;

    /// <summary>
    /// 指定局面の評価値を返す。
    /// </summary>
    public int Evaluate(Position position)
    {
        if (useFixedScore)
        {
            return fixedScore;
        }

        return accumulator!.EvaluateIncremental(position);
    }
}
