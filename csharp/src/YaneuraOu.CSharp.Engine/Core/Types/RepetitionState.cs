namespace YaneuraOu.CSharp.Engine.Core.Types;

/// <summary>
/// 千日手判定の状態を表す列挙体。
/// </summary>
public enum RepetitionState
{
    REPETITION_NONE = 0,
    REPETITION_DRAW = 1,
    REPETITION_WIN = 2,
    REPETITION_LOSE = 3,
    REPETITION_SUPERIOR = 4,
    REPETITION_INFERIOR = 5,
}
