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

    /// <summary>
    /// 注釈付き生成で玉移動・成り・打ちの各ケースが含まれることを検証する。
    /// </summary>
    [TestMethod]
    public void GenerateAnnotated_ContainsKingMovePromotionDropCaptureCoverage()
    {
        var sampler = new SfenSampler();

        IReadOnlyList<SfenSample> samples = sampler.GenerateAnnotated(count: 60, seed: 20260207, minPlies: 24, maxPlies: 60);

        Assert.IsTrue(samples.Any(s => (s.Features & SfenSampleFeatures.KingMove) != 0), "king move coverage missing");
        Assert.IsTrue(samples.Any(s => (s.Features & SfenSampleFeatures.Promotion) != 0), "promotion coverage missing");
        Assert.IsTrue(samples.Any(s => (s.Features & SfenSampleFeatures.Drop) != 0), "drop coverage missing");
        Assert.IsTrue(samples.Any(s => (s.Features & SfenSampleFeatures.Capture) != 0), "capture coverage missing");
    }

    /// <summary>
    /// 注釈付き生成も同一seedで決定的であることを検証する。
    /// </summary>
    [TestMethod]
    public void GenerateAnnotated_WithSameSeed_IsDeterministic()
    {
        var sampler = new SfenSampler();

        IReadOnlyList<SfenSample> first = sampler.GenerateAnnotated(count: 20, seed: 20260208, minPlies: 20, maxPlies: 40);
        IReadOnlyList<SfenSample> second = sampler.GenerateAnnotated(count: 20, seed: 20260208, minPlies: 20, maxPlies: 40);

        CollectionAssert.AreEqual(first.Select(s => $"{s.Sfen}|{(int)s.Features}").ToList(), second.Select(s => $"{s.Sfen}|{(int)s.Features}").ToList());
    }
}
