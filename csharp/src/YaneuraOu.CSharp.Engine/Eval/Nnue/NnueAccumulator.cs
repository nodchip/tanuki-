namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE特徴量の差分更新を管理するアキュムレータクラス。
/// </summary>
public sealed class NnueAccumulator
{
    private readonly NnueFeatureTransformer transformer;
    private readonly int[] weightTable;
    private HashSet<int>? previousFeatures;
    private int currentScore;

    /// <summary>
    /// NnueAccumulatorのインスタンスを初期化する。
    /// </summary>
    public NnueAccumulator(NnueFeatureTransformer transformer, byte[] modelBytes)
    {
        this.transformer = transformer;
        weightTable = BuildWeightTable(modelBytes);
    }

    /// <summary>
    /// 差分更新で評価値を算出する。
    /// </summary>
    public int EvaluateIncremental(Core.Position position)
    {
        int[] current = transformer.Transform(position);
        var currentSet = new HashSet<int>(current);
        if (previousFeatures is null)
        {
            currentScore = EvaluateFeatures(currentSet);
            previousFeatures = currentSet;
            return currentScore;
        }

        foreach (int feature in currentSet)
        {
            if (!previousFeatures.Contains(feature))
            {
                currentScore += WeightOf(feature);
            }
        }

        foreach (int feature in previousFeatures)
        {
            if (!currentSet.Contains(feature))
            {
                currentScore -= WeightOf(feature);
            }
        }

        previousFeatures = currentSet;
        return currentScore;
    }

    /// <summary>
    /// 全再計算で評価値を算出する。
    /// </summary>
    public int EvaluateByRecompute(Core.Position position)
    {
        int[] current = transformer.Transform(position);
        return EvaluateFeatures(new HashSet<int>(current));
    }

    /// <summary>
    /// 保持状態を初期化する。
    /// </summary>
    public void Reset()
    {
        previousFeatures = null;
        currentScore = 0;
    }

    /// <summary>
    /// 特徴量集合を評価値へ変換する。
    /// </summary>
    private int EvaluateFeatures(HashSet<int> features)
    {
        int score = 0;
        foreach (int feature in features)
        {
            score += WeightOf(feature);
        }

        return score;
    }

    /// <summary>
    /// 特徴量インデックスに対応する重みを返す。
    /// </summary>
    private int WeightOf(int feature)
    {
        int index = Math.Abs(feature) % weightTable.Length;
        return weightTable[index];
    }

    /// <summary>
    /// モデルバイト列から重みテーブルを生成する。
    /// </summary>
    private static int[] BuildWeightTable(byte[] modelBytes)
    {
        const int tableSize = 4096;
        var table = new int[tableSize];

        if (modelBytes.Length == 0)
        {
            return table;
        }

        for (int i = 0; i < tableSize; i++)
        {
            byte b0 = modelBytes[(i * 4) % modelBytes.Length];
            byte b1 = modelBytes[(i * 4 + 1) % modelBytes.Length];
            int value = ((b0 << 8) | b1) - 32768;
            table[i] = value / 64;
        }

        return table;
    }
}
