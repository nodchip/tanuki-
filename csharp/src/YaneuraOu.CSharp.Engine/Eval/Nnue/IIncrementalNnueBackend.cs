using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEバックエンドの差分更新フックを表すインターフェース。
/// </summary>
public interface IIncrementalNnueBackend
{
    /// <summary>
    /// 探索開始局面で差分状態を初期化する。
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

    /// <summary>
    /// 直近探索における差分更新統計を返す。
    /// </summary>
    NnueIncrementalStats GetStats();
}
