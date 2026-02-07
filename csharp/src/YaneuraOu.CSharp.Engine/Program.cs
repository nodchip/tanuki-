using YaneuraOu.CSharp.Engine.Usi;

namespace YaneuraOu.CSharp.Engine;

/// <summary>
/// エントリーポイントを提供するクラス。
/// </summary>
public static class Program
{
    /// <summary>
    /// USIコマンドループを開始する。
    /// </summary>
    public static void Main(string[] args)
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        while (!engine.ShouldQuit)
        {
            string? line = Console.ReadLine();
            if (line is null)
            {
                break;
            }

            string response = engine.HandleCommand(line);
            if (string.IsNullOrEmpty(response))
            {
                continue;
            }

            string[] outputs = response.Split('\n', StringSplitOptions.RemoveEmptyEntries);
            foreach (string output in outputs)
            {
                Console.WriteLine(output);
            }
        }
    }
}
