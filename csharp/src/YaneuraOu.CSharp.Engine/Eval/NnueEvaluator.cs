using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE優先で評価し、未使用時はフォールバック評価を返す評価器。
/// </summary>
public sealed class NnueEvaluator : IEvaluator, IIncrementalEvaluator
{
    private readonly INnueBackend backend;
    private readonly IEvaluator fallback;
    private readonly IIncrementalNnueBackend? incrementalBackend;

    /// <summary>
    /// NnueEvaluatorのインスタンスを初期化する。
    /// </summary>
    public NnueEvaluator(INnueBackend backend, IEvaluator fallback)
    {
        this.backend = backend;
        this.fallback = fallback;
        incrementalBackend = backend as IIncrementalNnueBackend;
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

    /// <summary>
    /// 探索開始局面で差分状態を初期化する。
    /// </summary>
    public void ResetIncrementalState(Position position)
    {
        incrementalBackend?.ResetIncrementalState(position);
    }

    /// <summary>
    /// do_move適用後に差分状態を進める。
    /// </summary>
    public void OnMoveApplied(Position positionAfterMove, Move move, Piece capturedPiece, Color movingSide)
    {
        incrementalBackend?.OnMoveApplied(positionAfterMove, move, capturedPiece, movingSide);
    }

    /// <summary>
    /// undo_move適用後に差分状態を戻す。
    /// </summary>
    public void OnMoveUndone(Position positionAfterUndo, Move move)
    {
        incrementalBackend?.OnMoveUndone(positionAfterUndo, move);
    }
}
