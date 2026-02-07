using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// NNUEアキュムレータの整合性を検証するテストクラス。
/// </summary>
[TestClass]
public class NnueAccumulatorTests
{
    /// <summary>
    /// 初期局面で差分評価と全再計算が一致することを検証する。
    /// </summary>
    [TestMethod]
    public void EvaluateIncremental_InitialPosition_MatchesRecompute()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var accumulator = new NnueAccumulator(new NnueFeatureTransformer(), CreateModel());

        int incremental = accumulator.EvaluateIncremental(position);
        int recompute = accumulator.EvaluateByRecompute(position);

        Assert.AreEqual(recompute, incremental);
    }

    /// <summary>
    /// do_move/undo_move後も差分評価と全再計算が一致することを検証する。
    /// </summary>
    [TestMethod]
    public void EvaluateIncremental_AfterMoveCycle_MatchesRecompute()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var accumulator = new NnueAccumulator(new NnueFeatureTransformer(), CreateModel());

        int before = accumulator.EvaluateIncremental(position);
        Move move = ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN);
        position.do_move(move, new StateInfo(), position.gives_check(move));

        int afterMoveIncremental = accumulator.EvaluateIncremental(position);
        int afterMoveRecompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(afterMoveRecompute, afterMoveIncremental);

        position.undo_move(move);
        int afterUndoIncremental = accumulator.EvaluateIncremental(position);
        int afterUndoRecompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(afterUndoRecompute, afterUndoIncremental);
        Assert.AreEqual(before, afterUndoIncremental);
    }

    /// <summary>
    /// do_null_move/undo_null_move後も差分評価と全再計算が一致することを検証する。
    /// </summary>
    [TestMethod]
    public void EvaluateIncremental_AfterNullMoveCycle_MatchesRecompute()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var accumulator = new NnueAccumulator(new NnueFeatureTransformer(), CreateModel());

        int before = accumulator.EvaluateIncremental(position);

        position.do_null_move(new StateInfo());
        int afterNullIncremental = accumulator.EvaluateIncremental(position);
        int afterNullRecompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(afterNullRecompute, afterNullIncremental);

        position.undo_null_move();
        int afterUndoIncremental = accumulator.EvaluateIncremental(position);
        int afterUndoRecompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(afterUndoRecompute, afterUndoIncremental);
        Assert.AreEqual(before, afterUndoIncremental);
    }

    /// <summary>
    /// テスト用のNNUEモデルを生成する。
    /// </summary>
    private static NnueModel CreateModel()
    {
        var model = new NnueModel();
        for (int i = 0; i < model.FtBiases.Length; i++)
        {
            model.FtBiases[i] = (short)(i % 16);
        }

        for (int i = 0; i < model.OutputWeights.Length; i++)
        {
            model.OutputWeights[i] = 1;
        }

        return model;
    }
}
