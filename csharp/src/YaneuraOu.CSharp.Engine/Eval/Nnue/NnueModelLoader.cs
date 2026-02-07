using System.Buffers.Binary;
using System.Runtime.InteropServices;
using System.Text;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEモデルを読み込んでバックエンドを生成するクラス。
/// </summary>
public sealed class NnueModelLoader
{
    private const uint ExpectedNnueVersion = 0x7AF32F16u;
    private const int HeaderPrefixSize = 12;
    private const int TransformerHashSize = 4;
    private const int NetworkHashSize = 4;

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

            if (!TryReadNnueHeader(data, out NnueModelMetadata metadata))
            {
                return new NullNnueBackend();
            }

            if (metadata.Version != ExpectedNnueVersion
                || !metadata.Architecture.Contains("Features=HalfKP", StringComparison.Ordinal)
                || !metadata.Architecture.Contains("Network=", StringComparison.Ordinal))
            {
                return new NullNnueBackend();
            }

            if (!TryParseHalfKpModel(data.AsSpan(metadata.HeaderByteLength), out NnueModel? model))
            {
                return new NullNnueBackend();
            }

            if (model is null)
            {
                return new NullNnueBackend();
            }

            return new FileNnueBackend(model);
        }
        catch (IOException)
        {
            return new NullNnueBackend();
        }
        catch (UnauthorizedAccessException)
        {
            return new NullNnueBackend();
        }
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

    /// <summary>
    /// HalfKP 256x2-32-32 形式のNNUE本体を読み取る。
    /// </summary>
    private static bool TryParseHalfKpModel(ReadOnlySpan<byte> payload, out NnueModel? model)
    {
        model = null;
        try
        {
            int cursor = 0;
            if (payload.Length < TransformerHashSize + NetworkHashSize)
            {
                return false;
            }

            var parsed = new NnueModel();

            cursor += TransformerHashSize;
            int ftBiasBytes = parsed.FtBiases.Length * sizeof(short);
            NnueModel.ReadInt16Array(payload.Slice(cursor, ftBiasBytes), parsed.FtBiases);
            cursor += ftBiasBytes;

            int ftWeightBytes = parsed.FtWeights.Length * sizeof(short);
            NnueModel.ReadInt16Array(payload.Slice(cursor, ftWeightBytes), parsed.FtWeights);
            cursor += ftWeightBytes;

            cursor += NetworkHashSize;

            int l1BiasBytes = parsed.Layer1Biases.Length * sizeof(int);
            NnueModel.ReadInt32Array(payload.Slice(cursor, l1BiasBytes), parsed.Layer1Biases);
            cursor += l1BiasBytes;

            int l1WeightBytes = parsed.Layer1Weights.Length;
            payload.Slice(cursor, l1WeightBytes).CopyTo(MemoryMarshal.AsBytes(parsed.Layer1Weights.AsSpan()));
            cursor += l1WeightBytes;

            int l2BiasBytes = parsed.Layer2Biases.Length * sizeof(int);
            NnueModel.ReadInt32Array(payload.Slice(cursor, l2BiasBytes), parsed.Layer2Biases);
            cursor += l2BiasBytes;

            int l2WeightBytes = parsed.Layer2Weights.Length;
            payload.Slice(cursor, l2WeightBytes).CopyTo(MemoryMarshal.AsBytes(parsed.Layer2Weights.AsSpan()));
            cursor += l2WeightBytes;

            parsed.OutputBias = BinaryPrimitives.ReadInt32LittleEndian(payload.Slice(cursor, 4));
            cursor += 4;

            int outputWeightBytes = parsed.OutputWeights.Length;
            payload.Slice(cursor, outputWeightBytes).CopyTo(MemoryMarshal.AsBytes(parsed.OutputWeights.AsSpan()));

            model = parsed;
            return true;
        }
        catch (ArgumentOutOfRangeException)
        {
            return false;
        }
        catch (InvalidDataException)
        {
            return false;
        }
    }
}
