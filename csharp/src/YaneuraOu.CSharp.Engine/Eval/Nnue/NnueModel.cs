using System.Buffers.Binary;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEの推論に必要な重み群を保持するクラス。
/// </summary>
public sealed class NnueModel
{
    /// <summary>
    /// FeatureTransformerの出力次元数(片側)。
    /// </summary>
    public const int HalfDimensions = 256;

    /// <summary>
    /// HalfKPの入力次元数。
    /// </summary>
    public const int InputDimensions = 81 * 1548;

    private const int TotalDimensions = HalfDimensions * 2;

    /// <summary>
    /// FeatureTransformerのバイアス。
    /// </summary>
    public short[] FtBiases { get; } = new short[HalfDimensions];

    /// <summary>
    /// FeatureTransformerの重み(行優先)。
    /// </summary>
    public short[] FtWeights { get; } = new short[HalfDimensions * InputDimensions];

    /// <summary>
    /// 第1層(512->32)のバイアス。
    /// </summary>
    public int[] Layer1Biases { get; } = new int[32];

    /// <summary>
    /// 第1層(512->32)の重み(行優先)。
    /// </summary>
    public sbyte[] Layer1Weights { get; } = new sbyte[32 * TotalDimensions];

    /// <summary>
    /// 第2層(32->32)のバイアス。
    /// </summary>
    public int[] Layer2Biases { get; } = new int[32];

    /// <summary>
    /// 第2層(32->32)の重み(行優先)。
    /// </summary>
    public sbyte[] Layer2Weights { get; } = new sbyte[32 * 32];

    /// <summary>
    /// 出力層(32->1)のバイアス。
    /// </summary>
    public int OutputBias { get; set; }

    /// <summary>
    /// 出力層(32->1)の重み。
    /// </summary>
    public sbyte[] OutputWeights { get; } = new sbyte[32];

    /// <summary>
    /// 学習済みモデルの推論結果を返す。
    /// </summary>
    public int Evaluate(ReadOnlySpan<byte> transformedFeatures)
    {
        Span<int> h1 = stackalloc int[32];
        Span<byte> a1 = stackalloc byte[32];
        Span<int> h2 = stackalloc int[32];
        Span<byte> a2 = stackalloc byte[32];

        Affine(Layer1Biases, Layer1Weights, TotalDimensions, transformedFeatures, h1);
        ClippedRelu(h1, a1);
        Affine(Layer2Biases, Layer2Weights, 32, a1, h2);
        ClippedRelu(h2, a2);

        int output = OutputBias;
        for (int i = 0; i < 32; i++)
        {
            output += OutputWeights[i] * a2[i];
        }

        return output / 16;
    }

    /// <summary>
    /// little-endian int16配列を展開する。
    /// </summary>
    internal static void ReadInt16Array(ReadOnlySpan<byte> src, short[] dst)
    {
        int required = dst.Length * sizeof(short);
        if (src.Length < required)
        {
            throw new InvalidDataException("NNUE int16配列の長さが不足しています。");
        }

        for (int i = 0; i < dst.Length; i++)
        {
            dst[i] = BinaryPrimitives.ReadInt16LittleEndian(src.Slice(i * 2, 2));
        }
    }

    /// <summary>
    /// little-endian int32配列を展開する。
    /// </summary>
    internal static void ReadInt32Array(ReadOnlySpan<byte> src, int[] dst)
    {
        int required = dst.Length * sizeof(int);
        if (src.Length < required)
        {
            throw new InvalidDataException("NNUE int32配列の長さが不足しています。");
        }

        for (int i = 0; i < dst.Length; i++)
        {
            dst[i] = BinaryPrimitives.ReadInt32LittleEndian(src.Slice(i * 4, 4));
        }
    }

    /// <summary>
    /// アフィン層を計算する。
    /// </summary>
    private static void Affine(int[] biases, sbyte[] weights, int inputDimensions, ReadOnlySpan<byte> input, Span<int> output)
    {
        for (int row = 0; row < output.Length; row++)
        {
            int sum = biases[row];
            int offset = row * inputDimensions;
            for (int col = 0; col < inputDimensions; col++)
            {
                sum += weights[offset + col] * input[col];
            }

            output[row] = sum;
        }
    }

    /// <summary>
    /// ClippedReLU(0..127, shift=6)を適用する。
    /// </summary>
    private static void ClippedRelu(ReadOnlySpan<int> input, Span<byte> output)
    {
        for (int i = 0; i < input.Length; i++)
        {
            int value = input[i] >> 6;
            output[i] = (byte)Math.Clamp(value, 0, 127);
        }
    }
}
