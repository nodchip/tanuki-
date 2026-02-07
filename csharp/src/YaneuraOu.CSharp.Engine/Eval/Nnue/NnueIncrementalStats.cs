namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE差分更新の実行統計を表す値オブジェクト。
/// </summary>
public readonly record struct NnueIncrementalStats(int RebuildCount, int DeltaApplyCount, int EvaluateCount);
