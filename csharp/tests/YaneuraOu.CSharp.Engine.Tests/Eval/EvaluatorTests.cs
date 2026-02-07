using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// 評価関数の基本動作を検証するテストクラス。
/// </summary>
[TestClass]
public class EvaluatorTests
{
    /// <summary>
    /// 同一局面の評価値が再現することを検証する。
    /// </summary>
    [TestMethod]
    public void MaterialEvaluator_SamePosition_ReturnsDeterministicScore()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var evaluator = new MaterialEvaluator();

        int score1 = evaluator.Evaluate(position);
        int score2 = evaluator.Evaluate(position);

        Assert.AreEqual(score1, score2);
    }

    /// <summary>
    /// 持ち駒差が評価値に反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void MaterialEvaluator_BlackHasPawnInHand_ReturnsPositive()
    {
        var position = new Position();
        position.set("lnsgkgsnl/1r5b1/p1pppp1pp/6p2/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL b P 1", new StateInfo());
        var evaluator = new MaterialEvaluator();

        int score = evaluator.Evaluate(position);

        Assert.IsTrue(score > 0);
    }

    /// <summary>
    /// NNUEバックエンドが有効な場合はその評価値を返すことを検証する。
    /// </summary>
    [TestMethod]
    public void NnueEvaluator_BackendEnabled_ReturnsBackendScore()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var backend = new FixedNnueBackend(321, true);
        var evaluator = new NnueEvaluator(backend, new MaterialEvaluator());

        int score = evaluator.Evaluate(position);

        Assert.AreEqual(321, score);
    }

    /// <summary>
    /// NNUEバックエンドが無効な場合はフォールバック評価値と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void NnueEvaluator_BackendDisabled_UsesFallback()
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        var backend = new FixedNnueBackend(321, false);
        var evaluator = new NnueEvaluator(backend, new MaterialEvaluator());

        int score = evaluator.Evaluate(position);

        Assert.AreEqual(new MaterialEvaluator().Evaluate(position), score);
    }

    /// <summary>
    /// テスト用の固定NNUEバックエンド。
    /// </summary>
    private sealed class FixedNnueBackend : INnueBackend
    {
        private readonly int score;

        /// <summary>
        /// 固定NNUEバックエンドを初期化する。
        /// </summary>
        public FixedNnueBackend(int score, bool isEnabled)
        {
            this.score = score;
            IsEnabled = isEnabled;
        }

        /// <summary>
        /// NNUEが有効かどうかを返す。
        /// </summary>
        public bool IsEnabled { get; }

        /// <summary>
        /// 固定評価値を返す。
        /// </summary>
        public int Evaluate(Position position)
        {
            return score;
        }
    }
}
