using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Search;

namespace YaneuraOu.CSharp.Engine.Tests.Search;

/// <summary>
/// 置換表境界の利用判定を検証するテストクラス。
/// </summary>
[TestClass]
public class TranspositionPolicyTests
{
    /// <summary>
    /// 完全一致値は探索窓に関係なく利用できることを検証する。
    /// </summary>
    [TestMethod]
    public void TryResolve_ExactEntry_IsUsable()
    {
        bool ok = TranspositionPolicy.TryResolve(
            storedDepth: 6,
            requiredDepth: 4,
            storedScore: 120,
            bound: TranspositionBound.Exact,
            alpha: -50,
            beta: 50,
            storedMove: Move.none(),
            out int score,
            out _);

        Assert.IsTrue(ok);
        Assert.AreEqual(120, score);
    }

    /// <summary>
    /// 下限値はbeta以上のときのみ利用できることを検証する。
    /// </summary>
    [TestMethod]
    public void TryResolve_LowerBound_RequiresBetaCutCondition()
    {
        bool usable = TranspositionPolicy.TryResolve(
            storedDepth: 6,
            requiredDepth: 4,
            storedScore: 80,
            bound: TranspositionBound.Lower,
            alpha: -10,
            beta: 60,
            storedMove: Move.none(),
            out int score,
            out _);
        bool blocked = TranspositionPolicy.TryResolve(
            storedDepth: 6,
            requiredDepth: 4,
            storedScore: 40,
            bound: TranspositionBound.Lower,
            alpha: -10,
            beta: 60,
            storedMove: Move.none(),
            out _,
            out _);

        Assert.IsTrue(usable);
        Assert.AreEqual(80, score);
        Assert.IsFalse(blocked);
    }

    /// <summary>
    /// 上限値はalpha以下のときのみ利用できることを検証する。
    /// </summary>
    [TestMethod]
    public void TryResolve_UpperBound_RequiresAlphaCutCondition()
    {
        bool usable = TranspositionPolicy.TryResolve(
            storedDepth: 6,
            requiredDepth: 4,
            storedScore: -40,
            bound: TranspositionBound.Upper,
            alpha: -20,
            beta: 80,
            storedMove: Move.none(),
            out int score,
            out _);
        bool blocked = TranspositionPolicy.TryResolve(
            storedDepth: 6,
            requiredDepth: 4,
            storedScore: 10,
            bound: TranspositionBound.Upper,
            alpha: -20,
            beta: 80,
            storedMove: Move.none(),
            out _,
            out _);

        Assert.IsTrue(usable);
        Assert.AreEqual(-40, score);
        Assert.IsFalse(blocked);
    }
}
