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
    /// 初期局面で変換後特徴量が512次元で生成されることを検証する。
    /// </summary>
    [TestMethod]
    public void Transform_StartPosition_Returns512Features()
    {
        var transformer = new NnueFeatureTransformer();
        var model = CreateDummyModel();
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());

        byte[] transformed = transformer.Transform(position, model);

        Assert.AreEqual(512, transformed.Length);
    }

    /// <summary>
    /// 変換後特徴量が0..127に収まることを検証する。
    /// </summary>
    [TestMethod]
    public void Transform_OutputValues_AreClampedToByteRange()
    {
        var transformer = new NnueFeatureTransformer();
        var model = CreateDummyModel();
        var position = new Position();
        position.set("4k4/9/9/9/4P4/9/9/9/4K4 b - 1", new StateInfo());

        byte[] features = transformer.Transform(position, model);

        Assert.IsTrue(features.All(v => v <= 127));
    }

    /// <summary>
    /// ダミーモデルを生成する。
    /// </summary>
    private static NnueModel CreateDummyModel()
    {
        var model = new NnueModel();
        for (int i = 0; i < model.FtBiases.Length; i++)
        {
            model.FtBiases[i] = (short)(i % 8);
        }

        return model;
    }
}
