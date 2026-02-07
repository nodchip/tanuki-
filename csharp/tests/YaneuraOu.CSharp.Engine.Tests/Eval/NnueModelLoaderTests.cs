using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// NNUEモデル読み込みの動作を検証するテストクラス。
/// </summary>
[TestClass]
public class NnueModelLoaderTests
{
    private const uint NnueHeaderVersion = 0x7AF32F16u;
    private static readonly string RepoNnBinPath = ResolveRepoNnBinPath();

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

    /// <summary>
    /// strict指定で読み込んだNNUEバックエンドが照合統計を更新することを検証する。
    /// </summary>
    [TestMethod]
    public void Load_ExistingFileWithStrictEnabled_CollectsVerificationStats()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            File.WriteAllBytes(modelPath, BuildValidMinimalNnueBinary());
            var loader = new NnueModelLoader();
            var position = new Position();
            position.set(Position.StartSfen, new StateInfo());

            INnueBackend backend = loader.Load(modelPath, true);
            Assert.IsTrue(backend.IsEnabled);
            Assert.IsInstanceOfType<IIncrementalNnueBackend>(backend);

            var incremental = (IIncrementalNnueBackend)backend;
            incremental.ResetIncrementalState(position);
            _ = backend.Evaluate(position);
            NnueIncrementalStats stats = incremental.GetStats();

            Assert.IsTrue(stats.VerificationCount > 0);
        }
        finally
        {
            if (File.Exists(modelPath))
            {
                File.Delete(modelPath);
            }
        }
    }

    /// <summary>
    /// NNUEヘッダのみの不完全バイナリを読み込んだ場合に無効バックエンドを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void Load_BinaryFileWithHeaderSignature_ReturnsDisabledBackend()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            byte[] bytes = CreateSignedBinaryModelBytes(NnueHeaderVersion, 0x1234);
            File.WriteAllBytes(modelPath, bytes);
            var loader = new NnueModelLoader();

            INnueBackend backend = loader.Load(modelPath);

            Assert.IsFalse(backend.IsEnabled);
        }
        finally
        {
            if (File.Exists(modelPath))
            {
                File.Delete(modelPath);
            }
        }
    }

    /// <summary>
    /// NNUEヘッダ署名を持たないバイナリ形式モデルを読み込んだ場合に無効バックエンドを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void Load_BinaryFileWithoutHeaderSignature_ReturnsDisabledBackend()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            byte[] bytes = { 0x34, 0x12, 0x00, 0x00, 0xFE, 0xED };
            File.WriteAllBytes(modelPath, bytes);
            var loader = new NnueModelLoader();

            INnueBackend backend = loader.Load(modelPath);

            Assert.IsFalse(backend.IsEnabled);
        }
        finally
        {
            if (File.Exists(modelPath))
            {
                File.Delete(modelPath);
            }
        }
    }

    /// <summary>
    /// NNUEヘッダのバージョンが不一致のバイナリ形式モデルを読み込んだ場合に無効バックエンドを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void Load_BinaryFileWithInvalidHeaderVersion_ReturnsDisabledBackend()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            byte[] bytes = CreateSignedBinaryModelBytes(0x00000001u, 0x1234);
            File.WriteAllBytes(modelPath, bytes);
            var loader = new NnueModelLoader();

            INnueBackend backend = loader.Load(modelPath);

            Assert.IsFalse(backend.IsEnabled);
        }
        finally
        {
            if (File.Exists(modelPath))
            {
                File.Delete(modelPath);
            }
        }
    }

    /// <summary>
    /// eval/nn.bin を読み込んだ場合に有効バックエンドを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void Load_RepoNnBin_ReturnsEnabledBackend()
    {
        if (string.IsNullOrEmpty(RepoNnBinPath))
        {
            Assert.Inconclusive("eval/nn.bin が見つからないためスキップ");
            return;
        }

        var loader = new NnueModelLoader();

        INnueBackend backend = loader.Load(RepoNnBinPath);

        Assert.IsTrue(backend.IsEnabled);
    }

    /// <summary>
    /// eval/nn.bin を読み込んだバックエンドが局面依存の評価値を返すことを検証する。
    /// </summary>
    [TestMethod]
    public void Load_RepoNnBin_EvaluateDependsOnPosition()
    {
        if (string.IsNullOrEmpty(RepoNnBinPath))
        {
            Assert.Inconclusive("eval/nn.bin が見つからないためスキップ");
            return;
        }

        var loader = new NnueModelLoader();
        INnueBackend backend = loader.Load(RepoNnBinPath);
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());

        int before = backend.Evaluate(position);

        Move move = ShogiTypes.make_move(Square.SQ_77, Square.SQ_76, Piece.B_PAWN);
        position.do_move(move, new StateInfo(), position.gives_check(move));
        int after = backend.Evaluate(position);

        Assert.AreNotEqual(before, after);
    }

    /// <summary>
    /// 署名付きバイナリからNNUEメタ情報を取得できることを検証する。
    /// </summary>
    [TestMethod]
    public void TryLoadMetadata_SignedBinary_ReturnsParsedMetadata()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            byte[] bytes = CreateSignedBinaryModelBytes(NnueHeaderVersion, 0x11223344u);
            File.WriteAllBytes(modelPath, bytes);
            var loader = new NnueModelLoader();

            bool ok = loader.TryLoadMetadata(modelPath, out NnueModelMetadata metadata);

            Assert.IsTrue(ok);
            Assert.AreEqual(NnueHeaderVersion, metadata.Version);
            Assert.AreEqual(0x11223344u, metadata.HashValue);
            Assert.AreEqual("Features=HalfKP", metadata.Architecture);
            Assert.AreEqual(12 + "Features=HalfKP".Length, metadata.HeaderByteLength);
        }
        finally
        {
            if (File.Exists(modelPath))
            {
                File.Delete(modelPath);
            }
        }
    }

    /// <summary>
    /// eval/nn.bin からNNUEメタ情報を取得できることを検証する。
    /// </summary>
    [TestMethod]
    public void TryLoadMetadata_RepoNnBin_ReturnsParsedMetadata()
    {
        if (string.IsNullOrEmpty(RepoNnBinPath))
        {
            Assert.Inconclusive("eval/nn.bin が見つからないためスキップ");
            return;
        }

        var loader = new NnueModelLoader();

        bool ok = loader.TryLoadMetadata(RepoNnBinPath, out NnueModelMetadata metadata);

        Assert.IsTrue(ok);
        Assert.AreEqual(NnueHeaderVersion, metadata.Version);
        Assert.IsFalse(string.IsNullOrWhiteSpace(metadata.Architecture));
        Assert.IsTrue(metadata.HeaderByteLength > 12);
    }

    /// <summary>
    /// 実行ディレクトリから遡って eval/nn.bin の絶対パスを解決する。
    /// </summary>
    private static string ResolveRepoNnBinPath()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null)
        {
            string candidate = Path.Combine(dir.FullName, "eval", "nn.bin");
            if (File.Exists(candidate))
            {
                return candidate;
            }

            dir = dir.Parent;
        }

        return string.Empty;
    }

    /// <summary>
    /// NNUEヘッダ署名を含む最小バイナリモデルを生成する。
    /// </summary>
    private static byte[] CreateSignedBinaryModelBytes(uint version, uint hash)
    {
        const string arch = "Features=HalfKP";
        byte[] archBytes = System.Text.Encoding.ASCII.GetBytes(arch);
        byte[] bytes = new byte[128];

        BitConverter.GetBytes(version).CopyTo(bytes, 0);
        BitConverter.GetBytes(hash).CopyTo(bytes, 4);
        BitConverter.GetBytes((uint)archBytes.Length).CopyTo(bytes, 8);
        archBytes.CopyTo(bytes, 12);

        return bytes;
    }

    /// <summary>
    /// strictモードの単体テスト用に最小限の有効NNUEバイナリを生成する。
    /// </summary>
    private static byte[] BuildValidMinimalNnueBinary()
    {
        const string architecture = "Features=HalfKP(Friend)[125388 -> 256x2],Network=AffineTransform[32<-512](ClippedReLU[32](AffineTransform[32<-32](ClippedReLU[32](AffineTransform[32<-512](InputSlice[512(0:512)])))))";
        byte[] archBytes = System.Text.Encoding.ASCII.GetBytes(architecture);

        var model = new NnueModel();
        var bytes = new List<byte>(2_700_000);

        bytes.AddRange(BitConverter.GetBytes(NnueHeaderVersion));
        bytes.AddRange(BitConverter.GetBytes(0x12345678u));
        bytes.AddRange(BitConverter.GetBytes((uint)archBytes.Length));
        bytes.AddRange(archBytes);

        bytes.AddRange(new byte[4]);
        foreach (short value in model.FtBiases)
        {
            bytes.AddRange(BitConverter.GetBytes(value));
        }

        foreach (short value in model.FtWeights)
        {
            bytes.AddRange(BitConverter.GetBytes(value));
        }

        bytes.AddRange(new byte[4]);
        foreach (int value in model.Layer1Biases)
        {
            bytes.AddRange(BitConverter.GetBytes(value));
        }

        foreach (sbyte value in model.Layer1Weights)
        {
            bytes.Add(unchecked((byte)value));
        }
        foreach (int value in model.Layer2Biases)
        {
            bytes.AddRange(BitConverter.GetBytes(value));
        }

        foreach (sbyte value in model.Layer2Weights)
        {
            bytes.Add(unchecked((byte)value));
        }
        bytes.AddRange(BitConverter.GetBytes(model.OutputBias));
        foreach (sbyte value in model.OutputWeights)
        {
            bytes.Add(unchecked((byte)value));
        }

        return bytes.ToArray();
    }
}
