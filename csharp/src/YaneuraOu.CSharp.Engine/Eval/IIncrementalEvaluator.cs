using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// 探索中の差分更新フックを受け取る評価器インターフェース。
/// </summary>
public interface IIncrementalEvaluator
{
    /// <summary>
    /// 探索開始局面で内部状態を初期化する。
    /// </summary>
    void ResetIncrementalState(Position position);

    /// <summary>
    /// do_move適用後に差分状態を進める。
    /// </summary>
    void OnMoveApplied(Position positionAfterMove, Move move, Piece capturedPiece, Color movingSide);

    /// <summary>
    /// undo_move適用後に差分状態を戻す。
    /// </summary>
    void OnMoveUndone(Position positionAfterUndo, Move move);
}
