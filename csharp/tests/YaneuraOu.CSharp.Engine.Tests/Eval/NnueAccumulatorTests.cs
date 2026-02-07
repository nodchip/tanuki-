using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// NNUEアキュムレータの差分更新を検証するテストクラス。
/// </summary>
[TestClass]
public class NnueAccumulatorTests
{
    /// <summary>
    /// 初期局面で差分評価と再計算評価が一致することを検証する。
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
    /// 通常手の差分更新が再計算評価と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void PushMove_NormalMove_MatchesRecompute()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var accumulator = CreateInitializedAccumulator(position);

        Move move = ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN);
        ApplyMoveAndAssert(position, accumulator, move);
        UndoMoveAndAssert(position, accumulator, move);

        Assert.AreEqual(1, accumulator.RebuildCount);
        Assert.IsTrue(accumulator.DeltaApplyCount >= 1);
    }

    /// <summary>
    /// 捕獲手の差分更新が再計算評価と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void PushMove_CaptureMove_MatchesRecompute()
    {
        var position = new Position();
        position.set("4k4/9/9/9/9/9/4P4/4p4/4K4 b - 1", new StateInfo());
        var accumulator = CreateInitializedAccumulator(position);

        Move move = ShogiTypes.make_move(Square.SQ_57, Square.SQ_58, Piece.B_PAWN);
        ApplyMoveAndAssert(position, accumulator, move);
        UndoMoveAndAssert(position, accumulator, move);
    }

    /// <summary>
    /// 成り手の差分更新が再計算評価と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void PushMove_PromotionMove_MatchesRecompute()
    {
        var position = new Position();
        position.set("4k4/6P2/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
        var accumulator = CreateInitializedAccumulator(position);

        Move move = ShogiTypes.make_move_promote(Square.SQ_72, Square.SQ_71, Piece.B_PAWN);
        ApplyMoveAndAssert(position, accumulator, move);
        UndoMoveAndAssert(position, accumulator, move);
    }

    /// <summary>
    /// 打ち手の差分更新が再計算評価と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void PushMove_DropMove_MatchesRecompute()
    {
        var position = new Position();
        position.set("4k4/9/9/9/9/9/9/9/4K4 b P 1", new StateInfo());
        var accumulator = CreateInitializedAccumulator(position);

        Move move = ShogiTypes.make_move_drop(PieceType.PAWN, Square.SQ_55, Color.BLACK);
        ApplyMoveAndAssert(position, accumulator, move);
        UndoMoveAndAssert(position, accumulator, move);
    }

    /// <summary>
    /// 王手移動時はフォールバックしても再計算評価と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void PushMove_KingMove_FallbackMatchesRecompute()
    {
        var position = new Position();
        position.set("4k4/9/9/9/9/9/9/4K4/9 b - 1", new StateInfo());
        var accumulator = CreateInitializedAccumulator(position);

        Move move = ShogiTypes.make_move(Square.SQ_58, Square.SQ_59, Piece.B_KING);
        ApplyMoveAndAssert(position, accumulator, move);
        UndoMoveAndAssert(position, accumulator, move);

        Assert.IsTrue(accumulator.RebuildCount >= 2);
    }

    /// <summary>
    /// 連続手順の各plyで差分更新と再計算評価が一致することを検証する。
    /// </summary>
    [TestMethod]
    public void PushMove_MoveSequence_MatchesRecomputeOnEachPly()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var accumulator = CreateInitializedAccumulator(position);
        var stack = new Stack<Move>();
        Move[] sequence =
        {
            ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN),
            ShogiTypes.make_move(Square.SQ_33, Square.SQ_34, Piece.W_PAWN),
            ShogiTypes.make_move(Square.SQ_88, Square.SQ_22, Piece.B_BISHOP),
            ShogiTypes.make_move(Square.SQ_31, Square.SQ_22, Piece.W_SILVER),
        };

        foreach (Move move in sequence)
        {
            ApplyMoveAndAssert(position, accumulator, move);
            stack.Push(move);
        }

        while (stack.Count > 0)
        {
            Move move = stack.Pop();
            UndoMoveAndAssert(position, accumulator, move);
        }
    }

    /// <summary>
    /// Null手往復後も再計算評価と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void EvaluateIncremental_AfterNullMoveCycle_MatchesRecompute()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var accumulator = CreateInitializedAccumulator(position);
        int before = accumulator.EvaluateIncremental(position);

        position.do_null_move(new StateInfo());
        accumulator.Reset(position);
        int afterNullIncremental = accumulator.EvaluateIncremental(position);
        int afterNullRecompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(afterNullRecompute, afterNullIncremental);

        position.undo_null_move();
        accumulator.Reset(position);
        int afterUndoIncremental = accumulator.EvaluateIncremental(position);
        int afterUndoRecompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(afterUndoRecompute, afterUndoIncremental);
        Assert.AreEqual(before, afterUndoIncremental);
    }

    /// <summary>
    /// 指定局面で初期化済みのアキュムレータを生成する。
    /// </summary>
    private static NnueAccumulator CreateInitializedAccumulator(Position position)
    {
        var accumulator = new NnueAccumulator(new NnueFeatureTransformer(), CreateModel());
        accumulator.Reset(position);
        return accumulator;
    }

    /// <summary>
    /// 1手進めて差分評価と再計算評価が一致することを検証する。
    /// </summary>
    private static void ApplyMoveAndAssert(Position position, NnueAccumulator accumulator, Move move)
    {
        Color movingSide = position.side_to_move();
        var st = new StateInfo();
        position.do_move(move, st, position.gives_check(move));
        accumulator.PushMove(position, move, st.capturedPiece, movingSide);

        int incremental = accumulator.EvaluateIncremental(position);
        int recompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(recompute, incremental);
    }

    /// <summary>
    /// 1手戻して差分評価と再計算評価が一致することを検証する。
    /// </summary>
    private static void UndoMoveAndAssert(Position position, NnueAccumulator accumulator, Move move)
    {
        position.undo_move(move);
        accumulator.Pop();

        int incremental = accumulator.EvaluateIncremental(position);
        int recompute = accumulator.EvaluateByRecompute(position);
        Assert.AreEqual(recompute, incremental);
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
            model.FtWeights[i] = (short)((i % 7) - 3);
        }

        for (int i = 0; i < model.OutputWeights.Length; i++)
        {
            model.OutputWeights[i] = 1;
        }

        return model;
    }
}
