using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE未接続時に使用する無効バックエンド。
/// </summary>
public sealed class NullNnueBackend : INnueBackend, IIncrementalNnueBackend
{
    /// <summary>
    /// NNUEが有効かどうかを返す。
    /// </summary>
    public bool IsEnabled => false;

    /// <summary>
    /// 未使用時の評価値を返す。
    /// </summary>
    public int Evaluate(Position position)
    {
        return 0;
    }

    /// <summary>
    /// 探索開始時の初期化を行う。
    /// </summary>
    public void ResetIncrementalState(Position position)
    {
    }

    /// <summary>
    /// do_move適用時の更新を行う。
    /// </summary>
    public void OnMoveApplied(Position positionAfterMove, Move move, Piece capturedPiece, Color movingSide)
    {
    }

    /// <summary>
    /// undo_move適用時の更新を行う。
    /// </summary>
    public void OnMoveUndone(Position positionAfterUndo, Move move)
    {
    }

    /// <summary>
    /// 直近探索における差分更新統計を返す。
    /// </summary>
    public NnueIncrementalStats GetStats()
    {
        return new NnueIncrementalStats(0, 0, 0);
    }
}
