using System.Text.Json;
using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Eval;

namespace YaneuraOu.CSharp.Engine.Tests.Eval;

/// <summary>
/// C++基準値とのNNUE評価一致を検証するテストクラス。
/// </summary>
[TestClass]
public class NnueParityTests
{
    private const int MinimumStrictCases = 500;
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
    };

    /// <summary>
    /// JSONLの基準局面でNNUE評価値が期待値と一致することを検証する。
    /// </summary>
    [TestMethod]
    public void Evaluate_ParityCases_MatchesExpectedScores()
    {
        if (!string.Equals(Environment.GetEnvironmentVariable("NNUE_PARITY_STRICT"), "1", StringComparison.Ordinal))
        {
            Assert.Inconclusive("NNUE_PARITY_STRICT=1 未設定のためスキップ");
            return;
        }

        string modelPath = ResolveRepoFilePath("eval", "nn.bin");
        string casesPath = ResolveRepoFilePath("eval", "nnue-parity-cases.jsonl");
        if (string.IsNullOrEmpty(modelPath))
        {
            Assert.Inconclusive("eval/nn.bin が見つからないためスキップ");
            return;
        }

        if (string.IsNullOrEmpty(casesPath))
        {
            Assert.Inconclusive("eval/nnue-parity-cases.jsonl が見つからないためスキップ");
            return;
        }

        var loader = new NnueModelLoader();
        INnueBackend backend = loader.Load(modelPath);
        Assert.IsTrue(backend.IsEnabled);

        int checkedCases = 0;
        foreach (string line in File.ReadLines(casesPath))
        {
            string trimmed = line.Trim();
            if (string.IsNullOrEmpty(trimmed))
            {
                continue;
            }

            var parity = JsonSerializer.Deserialize<ParityCase>(trimmed, JsonOptions);
            if (parity is null || string.IsNullOrWhiteSpace(parity.Sfen))
            {
                continue;
            }

            var pos = new Position();
            pos.set(parity.Sfen, new StateInfo());
            int actual = backend.Evaluate(pos);
            Assert.AreEqual(parity.ExpectedScore, actual, $"SFEN: {parity.Sfen}");
            checkedCases++;
        }

        Assert.IsTrue(checkedCases >= MinimumStrictCases, $"検証対象の局面数が不足しています。expected>={MinimumStrictCases} actual={checkedCases}");
    }

    /// <summary>
    /// 実行ディレクトリから遡ってリポジトリ内ファイルの絶対パスを解決する。
    /// </summary>
    private static string ResolveRepoFilePath(params string[] relativeSegments)
    {
        string currentCandidate = Path.Combine(new[] { Environment.CurrentDirectory }.Concat(relativeSegments).ToArray());
        if (File.Exists(currentCandidate))
        {
            return currentCandidate;
        }

        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null)
        {
            string candidate = Path.Combine(new[] { dir.FullName }.Concat(relativeSegments).ToArray());
            if (File.Exists(candidate))
            {
                return candidate;
            }

            dir = dir.Parent;
        }

        return string.Empty;
    }

    /// <summary>
    /// JSONLの1行を表す評価一致ケース。
    /// </summary>
    private sealed class ParityCase
    {
        /// <summary>
        /// 検証対象のSFEN。
        /// </summary>
        public string Sfen { get; set; } = string.Empty;

        /// <summary>
        /// C++基準の期待評価値。
        /// </summary>
        public int ExpectedScore { get; set; }
    }
}
