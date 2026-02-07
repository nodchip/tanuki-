using System.Buffers.Binary;
using System.Text;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUEモデルを読み込んでバックエンドを生成するクラス。
/// </summary>
public sealed class NnueModelLoader
{
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

            if (TryReadScoreFromBinary(data, out int binaryScore))
            {
                return new FileNnueBackend(binaryScore);
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
    /// バイナリ形式の評価値を読み取る。
    /// </summary>
    private static bool TryReadScoreFromBinary(byte[] data, out int score)
    {
        score = 0;
        if (data.Length < sizeof(int))
        {
            return false;
        }

        score = BinaryPrimitives.ReadInt32LittleEndian(data.AsSpan(0, sizeof(int)));
        return true;
    }
}
