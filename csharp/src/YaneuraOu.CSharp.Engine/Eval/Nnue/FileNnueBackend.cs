using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEモデルまたは固定値から評価値を返すバックエンド。
/// </summary>
public sealed class FileNnueBackend : INnueBackend, IIncrementalNnueBackend
{
    private readonly bool useFixedScore;
    private readonly int fixedScore;
    private readonly NnueAccumulator? accumulator;
    private readonly bool verifyIncremental;
    private int verificationCount;
    private int mismatchCount;

    /// <summary>
    /// 固定評価値モードのインスタンスを初期化する。
    /// </summary>
    public FileNnueBackend(int score)
    {
        useFixedScore = true;
        fixedScore = score;
        verifyIncremental = false;
    }

    /// <summary>
    /// バイナリモデルモードのインスタンスを初期化する。
    /// </summary>
    public FileNnueBackend(NnueModel model)
        : this(model, Environment.GetEnvironmentVariable("NNUE_INCREMENTAL_STRICT") == "1")
    {
    }

    /// <summary>
    /// 厳密検証モード指定付きでバイナリモデルを初期化する。
    /// </summary>
    public FileNnueBackend(NnueModel model, bool verifyIncremental)
    {
        useFixedScore = false;
        accumulator = new NnueAccumulator(new NnueFeatureTransformer(), model);
        this.verifyIncremental = verifyIncremental;
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

        int incremental = accumulator!.EvaluateIncremental(position);
        if (verifyIncremental)
        {
            verificationCount++;
            int recompute = accumulator.EvaluateByRecompute(position);
            if (incremental != recompute)
            {
                mismatchCount++;
            }
        }

        return incremental;
    }

    /// <summary>
    /// 探索開始局面で差分状態を初期化する。
    /// </summary>
    public void ResetIncrementalState(Position position)
    {
        if (useFixedScore)
        {
            return;
        }

        accumulator!.Reset(position);
    }

    /// <summary>
    /// do_move適用後に差分状態を進める。
    /// </summary>
    public void OnMoveApplied(Position positionAfterMove, Move move, Piece capturedPiece, Color movingSide)
    {
        if (useFixedScore)
        {
            return;
        }

        accumulator!.PushMove(positionAfterMove, move, capturedPiece, movingSide);
    }

    /// <summary>
    /// undo_move適用後に差分状態を戻す。
    /// </summary>
    public void OnMoveUndone(Position positionAfterUndo, Move move)
    {
        if (useFixedScore)
        {
            return;
        }

        accumulator!.Pop();
    }

    /// <summary>
    /// 直近探索における差分更新統計を返す。
    /// </summary>
    public NnueIncrementalStats GetStats()
    {
        if (useFixedScore)
        {
            return new NnueIncrementalStats(0, 0, 0, 0, 0);
        }

        return new NnueIncrementalStats(
            accumulator!.RebuildCount,
            accumulator.DeltaApplyCount,
            accumulator.EvaluateCount,
            verificationCount,
            mismatchCount);
    }
}
