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

            if (TryReadNnueHeader(data, out NnueModelMetadata metadata)
                && metadata.Version == ExpectedNnueVersion
                && metadata.Architecture.Contains("Features=HalfKP", StringComparison.Ordinal))
            {
                byte[] payload = data.AsSpan(metadata.HeaderByteLength).ToArray();
                if (payload.Length == 0)
                {
                    return new NullNnueBackend();
                }

                return new FileNnueBackend(payload);
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
    /// モデルファイルからNNUEヘッダメタ情報を取得する。
    /// </summary>
    public bool TryLoadMetadata(string modelPath, out NnueModelMetadata metadata)
    {
        metadata = new NnueModelMetadata();
        if (string.IsNullOrWhiteSpace(modelPath) || !File.Exists(modelPath))
        {
            return false;
        }

        try
        {
            byte[] data = File.ReadAllBytes(modelPath);
            return TryReadNnueHeader(data, out metadata);
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
        }
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
    private static bool TryReadNnueHeader(byte[] data, out NnueModelMetadata metadata)
    {
        metadata = new NnueModelMetadata();
        if (data.Length < HeaderPrefixSize)
        {
            return false;
        }

        ReadOnlySpan<byte> span = data;
        uint version = BinaryPrimitives.ReadUInt32LittleEndian(span.Slice(0, 4));
        uint hashValue = BinaryPrimitives.ReadUInt32LittleEndian(span.Slice(4, 4));
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

        string architecture = Encoding.ASCII.GetString(data, HeaderPrefixSize, (int)archSize);
        metadata = new NnueModelMetadata
        {
            Version = version,
            HashValue = hashValue,
            Architecture = architecture,
            HeaderByteLength = end,
        };

        return true;
    }
}
