using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// NNUE特徴量変換の動作を検証するテストクラス。
/// </summary>
[TestClass]
public class NnueFeatureTransformerTests
{
    /// <summary>
    /// 開始局面で特徴量が生成されることを検証する。
    /// </summary>
    [TestMethod]
    public void Transform_StartPosition_ReturnsNonEmptyFeatures()
    {
        var transformer = new NnueFeatureTransformer();
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());

        int[] features = transformer.Transform(position);

        Assert.IsTrue(features.Length > 0);
    }

    /// <summary>
    /// 手番を反転した同一局面で特徴量が一致することを検証する。
    /// </summary>
    [TestMethod]
    public void Transform_SideToMoveFlippedPosition_ReturnsSameFeatures()
    {
        var transformer = new NnueFeatureTransformer();
        var black = new Position();
        var white = new Position();
        black.set("lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1", new StateInfo());
        white.set("lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL w - 1", new StateInfo());

        int[] blackFeatures = transformer.Transform(black);
        int[] whiteFeatures = transformer.Transform(white);

        CollectionAssert.AreEqual(blackFeatures, whiteFeatures);
    }

    /// <summary>
    /// 手駒が増えると特徴量数が増えることを検証する。
    /// </summary>
    [TestMethod]
    public void Transform_PositionWithHands_IncreasesFeatureCount()
    {
        var transformer = new NnueFeatureTransformer();
        var withoutHands = new Position();
        var withHands = new Position();
        withoutHands.set("4k4/9/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
        withHands.set("4k4/9/9/9/9/9/9/9/4K4 b RP2p 1", new StateInfo());

        int[] baseline = transformer.Transform(withoutHands);
        int[] hands = transformer.Transform(withHands);

        Assert.IsTrue(hands.Length > baseline.Length);
    }
}
