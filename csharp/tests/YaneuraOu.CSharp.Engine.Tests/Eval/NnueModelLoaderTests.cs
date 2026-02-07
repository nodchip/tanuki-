using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// NNUEモデル読み込みの動作を検証するテストクラス。
/// </summary>
[TestClass]
public class NnueModelLoaderTests
{
    /// <summary>
    /// モデルファイルが存在しない場合に無効バックエンドを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void Load_MissingFile_ReturnsDisabledBackend()
    {
        var loader = new NnueModelLoader();

        INnueBackend backend = loader.Load("C:/not-found/nn.bin");

        Assert.IsFalse(backend.IsEnabled);
    }

    /// <summary>
    /// モデルファイルが存在する場合に有効バックエンドを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void Load_ExistingFile_ReturnsEnabledBackend()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            File.WriteAllText(modelPath, "123");
            var loader = new NnueModelLoader();
            var position = new Position();
            position.set(Position.StartSfen, new StateInfo());

            INnueBackend backend = loader.Load(modelPath);

            Assert.IsTrue(backend.IsEnabled);
            Assert.AreEqual(123, backend.Evaluate(position));
        }
        finally
        {
            if (File.Exists(modelPath))
            {
                File.Delete(modelPath);
            }
        }
    }
}
