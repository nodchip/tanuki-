using Microsoft.VisualStudio.TestTools.UnitTesting;
using System.Reflection;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;
using YaneuraOu.CSharp.Engine.Search;

namespace YaneuraOu.CSharp.Engine.Tests.Searching;

[TestClass]
/// <summary>
/// Searcher の基本動作を検証するテストクラス。
/// </summary>
public class SearcherBasicTests
{
    [TestMethod]
    /// <summary>
    /// 差分更新フック実装評価器が探索中に呼び出されることを検証する。
    /// </summary>
    public void Search_WithIncrementalEvaluator_InvokesMoveHooks()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var evaluator = new CountingIncrementalEvaluator();
        var searcher = new Searcher(evaluator);

        searcher.Search(pos, new SearchLimits { Depth = 2 });

        Assert.AreEqual(1, evaluator.ResetCount);
        Assert.IsTrue(evaluator.MoveAppliedCount > 0);
        Assert.AreEqual(evaluator.MoveAppliedCount, evaluator.MoveUndoneCount);
    }

    [TestMethod]
    /// <summary>
    /// 深さ1探索で bestmove が返ることを検証する。
    /// </summary>
    public void Search_Depth1_ReturnsBestMove()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher();

        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 1 });

        Assert.AreNotEqual(Move.none().to_u32(), result.BestMove.to_u32());
        Assert.IsTrue(result.BestMove.is_ok());
        Assert.IsTrue(result.Nodes > 0);
    }

    [TestMethod]
    /// <summary>
    /// 返却された bestmove が合法手であることを検証する。
    /// </summary>
    public void Search_ReturnedMove_IsLegal()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher();

        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 2 });
        MoveList legal = MoveGenerator.GenerateLegal(pos);

        bool found = false;
        foreach (Move move in legal)
        {
            if (move.to_u32() == result.BestMove.to_u32())
            {
                found = true;
                break;
            }
        }

        Assert.IsTrue(found);
    }

    [TestMethod]
    /// <summary>
    /// 合法手がない局面では bestmove が none になることを検証する。
    /// </summary>
    public void Search_NoLegalMove_ReturnsNone()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
        pos.remove_piece(Square.SQ_59);

        var searcher = new Searcher();
        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 2 });

        Assert.AreEqual(Move.none().to_u32(), result.BestMove.to_u32());
    }

    [TestMethod]
    /// <summary>
    /// 同一局面を繰り返し探索するとTTヒットが発生することを検証する。
    /// </summary>
    public void Search_RepeatedPosition_ProducesTranspositionHit()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher();

        searcher.Search(pos, new SearchLimits { Depth = 2 });
        searcher.Search(pos, new SearchLimits { Depth = 2 });

        Assert.IsTrue(searcher.LastTranspositionHitCount > 0);
    }

    [TestMethod]
    /// <summary>
    /// 手番側の玉が不在の局面では大差負け評価を返すことを検証する。
    /// </summary>
    public void Search_SideToMoveKingMissing_ReturnsLargeNegativeScore()
    {
        var pos = new Position();
        pos.set("4k4/9/9/9/9/9/9/9/9 b - 1", new StateInfo());
        var searcher = new Searcher();

        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 1 });

        Assert.IsTrue(result.Score <= -90_000);
    }

    /// <summary>
    /// 静止探索で王手中の局面では王手回避手を探索してstand pat固定を回避することを検証する。
    /// </summary>
    [TestMethod]
    public void Quiescence_InCheckPosition_SearchesEvasions()
    {
        var pos = new Position();
        pos.set("4k4/9/4r4/9/9/9/9/9/4K4 b - 1", new StateInfo());
        var searcher = new Searcher(new CheckPenaltyEvaluator());
        int nodes = 0;

        int score = InvokeQuiescence(searcher, pos, -30000, 30000, ref nodes);

        Assert.IsTrue(score > -10000, $"score={score}");
    }

    /// <summary>
    /// Null Move枝刈りが探索中に発生することを検証する。
    /// </summary>
    [TestMethod]
    public void AlphaBeta_WithNarrowWindow_TriggersNullMovePruning()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher(new ConstantEvaluator(300));
        int nodes = 0;

        int score = InvokeAlphaBeta(searcher, pos, 4, -11000, -10000, 0, ref nodes);

        Assert.IsTrue(nodes > 0);
        Assert.IsTrue(score >= -11000);
        Assert.IsTrue(searcher.LastNullMovePruningCount > 0);
    }

    /// <summary>
    /// 十分な深さの探索でLMRが適用されることを検証する。
    /// </summary>
    [TestMethod]
    public void Search_Depth4_TriggersLmrReduction()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher(new ConstantEvaluator(0));
        int nodes = 0;

        int score = InvokeAlphaBeta(searcher, pos, 4, -30000, 30000, 0, ref nodes);

        Assert.IsTrue(nodes > 0);
        Assert.IsTrue(score >= -30000);
        Assert.IsTrue(searcher.LastLmrReductionCount > 0);
    }

    /// <summary>
    /// 反復深化でAspiration Windowの再探索が発生することを検証する。
    /// </summary>
    [TestMethod]
    public void Search_IterativeDeepening_TriggersAspirationResearch()
    {
        var pos = new Position();
        pos.set(Position.StartSfen, new StateInfo());
        var searcher = new Searcher(new PlyScaledEvaluator(200));

        SearchResult result = searcher.Search(pos, new SearchLimits { Depth = 4 });

        Assert.AreNotEqual(Move.none().to_u32(), result.BestMove.to_u32());
        Assert.IsTrue(searcher.LastAspirationReSearchCount > 0);
    }

    /// <summary>
    /// 呼び出し回数検証用の差分更新評価器。
    /// </summary>
    private sealed class CountingIncrementalEvaluator : IEvaluator, IIncrementalEvaluator
    {
        public int ResetCount { get; private set; }
        public int MoveAppliedCount { get; private set; }
        public int MoveUndoneCount { get; private set; }

        public int Evaluate(Position position)
        {
            return 0;
        }

        public void ResetIncrementalState(Position position)
        {
            ResetCount++;
        }

        public void OnMoveApplied(Position positionAfterMove, Move move, Piece capturedPiece, Color movingSide)
        {
            MoveAppliedCount++;
        }

        public void OnMoveUndone(Position positionAfterUndo, Move move)
        {
            MoveUndoneCount++;
        }
    }

    /// <summary>
    /// 王手中の局面へ大きなペナルティを与える検証用評価器。
    /// </summary>
    private sealed class CheckPenaltyEvaluator : IEvaluator
    {
        public int Evaluate(Position position)
        {
            return position.in_check() ? -10000 : 0;
        }
    }

    /// <summary>
    /// 常に固定値を返す評価器。
    /// </summary>
    private sealed class ConstantEvaluator : IEvaluator
    {
        private readonly int score;

        public ConstantEvaluator(int score)
        {
            this.score = score;
        }

        public int Evaluate(Position position)
        {
            return score;
        }
    }

    /// <summary>
    /// 現在plyに比例した値を返す評価器。
    /// </summary>
    private sealed class PlyScaledEvaluator : IEvaluator
    {
        private readonly int scale;

        public PlyScaledEvaluator(int scale)
        {
            this.scale = scale;
        }

        public int Evaluate(Position position)
        {
            return position.state().pliesFromNull * scale;
        }
    }

    /// <summary>
    /// private静止探索をテストから呼び出す。
    /// </summary>
    private static int InvokeQuiescence(Searcher searcher, Position position, int alpha, int beta, ref int nodes)
    {
        MethodInfo? method = typeof(Searcher).GetMethod("Quiescence", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.IsNotNull(method);
        object?[] args = { position, alpha, beta, nodes };
        int score = (int)method.Invoke(searcher, args)!;
        nodes = (int)args[3]!;
        return score;
    }

    /// <summary>
    /// private alpha-beta探索をテストから呼び出す。
    /// </summary>
    private static int InvokeAlphaBeta(Searcher searcher, Position position, int depth, int alpha, int beta, int ply, ref int nodes)
    {
        MethodInfo? method = typeof(Searcher).GetMethod("AlphaBeta", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.IsNotNull(method);
        object?[] args = { position, depth, alpha, beta, ply, nodes };
        int score = (int)method.Invoke(searcher, args)!;
        nodes = (int)args[5]!;
        return score;
    }
}
