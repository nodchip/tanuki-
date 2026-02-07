using System.Buffers.Binary;
using System.Text;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEモデルを読み込んでバックエンドを生成するクラス。
/// </summary>
public sealed class NnueModelLoader
{
    private const uint ExpectedNnueVersion = 0x7AF32F16u;
    private const int HeaderPrefixSize = 12;

    /// <summary>
    /// モデルファイルを読み込んでNNUEバックエンドを返す。
    /// </summary>
    public INnueBackend Load(string modelPath)
    {
        if (string.IsNullOrWhiteSpace(modelPath) || !File.Exists(modelPath))
        {
            return new NullNnueBackend();
        }

        try
        {
            byte[] data = File.ReadAllBytes(modelPath);
            if (TryReadScoreFromText(data, out int textScore))
            {
                return new FileNnueBackend(textScore);
            }

            if (TryReadNnueHeader(data, out uint version, out string architecture)
                && version == ExpectedNnueVersion
                && architecture.Contains("Features=HalfKP", StringComparison.Ordinal))
            {
                return new FileNnueBackend(data);
            }
        }
        catch (IOException)
        {
            return new NullNnueBackend();
        }
        catch (UnauthorizedAccessException)
        {
            return new NullNnueBackend();
        }

        return new NullNnueBackend();
    }

    /// <summary>
    /// テキスト形式の評価値を読み取る。
    /// </summary>
    private static bool TryReadScoreFromText(byte[] data, out int score)
    {
        score = 0;
        if (data.Length == 0)
        {
            return false;
        }

        string text = Encoding.UTF8.GetString(data).Trim();
        return int.TryParse(text, out score);
    }

    /// <summary>
    /// NNUEヘッダを読み取り、バージョンとアーキテクチャ文字列を返す。
    /// </summary>
    private static bool TryReadNnueHeader(byte[] data, out uint version, out string architecture)
    {
        version = 0;
        architecture = string.Empty;
        if (data.Length < HeaderPrefixSize)
        {
            return false;
        }

        ReadOnlySpan<byte> span = data;
        version = BinaryPrimitives.ReadUInt32LittleEndian(span.Slice(0, 4));
        uint archSize = BinaryPrimitives.ReadUInt32LittleEndian(span.Slice(8, 4));
        if (archSize == 0 || archSize > 1024)
        {
            return false;
        }

        int end = HeaderPrefixSize + (int)archSize;
        if (end > data.Length)
        {
            return false;
        }

        architecture = Encoding.ASCII.GetString(data, HeaderPrefixSize, (int)archSize);
        return true;
    }
}
