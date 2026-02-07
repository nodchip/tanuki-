using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// NNUEバックエンド統計の更新を検証するテストクラス。
/// </summary>
[TestClass]
public class NnueBackendStatsTests
{
    /// <summary>
    /// 差分更新を実行すると統計が増加することを検証する。
    /// </summary>
    [TestMethod]
    public void FileNnueBackend_IncrementalStats_IncreaseAfterMove()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var backend = new FileNnueBackend(CreateModel());

        backend.ResetIncrementalState(position);
        _ = backend.Evaluate(position);
        Move move = ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN);
        var st = new StateInfo();
        Color movingSide = position.side_to_move();
        position.do_move(move, st, position.gives_check(move));
        backend.OnMoveApplied(position, move, st.capturedPiece, movingSide);
        _ = backend.Evaluate(position);

        NnueIncrementalStats stats = backend.GetStats();
        Assert.IsTrue(stats.RebuildCount >= 1);
        Assert.IsTrue(stats.DeltaApplyCount >= 1);
        Assert.IsTrue(stats.EvaluateCount >= 2);
    }

    /// <summary>
    /// テスト用のNNUEモデルを構築する。
    /// </summary>
    private static NnueModel CreateModel()
    {
        var model = new NnueModel();
        for (int i = 0; i < model.FtBiases.Length; i++)
        {
            model.FtBiases[i] = (short)(i % 16);
        }

        for (int i = 0; i < model.FtWeights.Length; i++)
        {
            model.FtWeights[i] = (short)((i % 5) - 2);
        }

        for (int i = 0; i < model.OutputWeights.Length; i++)
        {
            model.OutputWeights[i] = 1;
        }

        return model;
    }
}
