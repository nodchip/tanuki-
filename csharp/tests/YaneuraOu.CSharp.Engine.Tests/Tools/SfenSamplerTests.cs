using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Tools;

namespace YaneuraOu.CSharp.Engine.Tests.Tools;

/// <summary>
/// SFENサンプラーの動作を検証するテストクラス。
/// </summary>
[TestClass]
public class SfenSamplerTests
{
    /// <summary>
    /// 指定件数ぶんのSFENが生成されることを検証する。
    /// </summary>
    [TestMethod]
    public void Generate_ReturnsRequestedCount()
    {
        var sampler = new SfenSampler();

        IReadOnlyList<string> sfens = sampler.Generate(count: 10, seed: 1234, minPlies: 8, maxPlies: 12);

        Assert.AreEqual(10, sfens.Count);
    }

    /// <summary>
    /// 同じシードで同一のSFEN列が生成されることを検証する。
    /// </summary>
    [TestMethod]
    public void Generate_WithSameSeed_IsDeterministic()
    {
        var sampler = new SfenSampler();

        IReadOnlyList<string> first = sampler.Generate(count: 10, seed: 2026, minPlies: 8, maxPlies: 12);
        IReadOnlyList<string> second = sampler.Generate(count: 10, seed: 2026, minPlies: 8, maxPlies: 12);

        CollectionAssert.AreEqual(first.ToList(), second.ToList());
    }

    /// <summary>
    /// 生成したSFENがPositionで読み込めることを検証する。
    /// </summary>
    [TestMethod]
    public void Generate_OutputSfen_CanBeParsedByPosition()
    {
        var sampler = new SfenSampler();
        IReadOnlyList<string> sfens = sampler.Generate(count: 5, seed: 1, minPlies: 8, maxPlies: 10);

        foreach (string sfen in sfens)
        {
            var pos = new Position();
            pos.set(sfen, new StateInfo());
            Assert.IsFalse(string.IsNullOrWhiteSpace(pos.sfen()));
        }
    }
}
