using Microsoft.VisualStudio.TestTools.UnitTesting;
using YaneuraOu.CSharp.Engine.Usi;

namespace YaneuraOu.CSharp.Engine.Tests.Usi;

/// <summary>
/// USIプロトコルの基本動作を検証するテストクラス。
/// </summary>
[TestClass]
public class UsiProtocolTests
{
    /// <summary>
    /// isreadyコマンドがreadyokを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_IsReady_ReturnsReadyOk()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string response = engine.HandleCommand("isready");

        Assert.AreEqual("readyok", response);
    }

    /// <summary>
    /// setoptionでDepthを設定した後にgoでbestmoveを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionDepth_ThenGo_ReturnsBestmove()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("setoption name Depth value 2");
        string response = engine.HandleCommand("go");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreNotEqual("bestmove resign", response);
    }

    /// <summary>
    /// position startpos moves適用後にgo depthでbestmoveを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_PositionStartposMoves_ThenGoDepth_ReturnsBestmove()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("position startpos moves 7g7f 3c3d");
        string response = engine.HandleCommand("go depth 1");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreNotEqual("bestmove resign", response);
    }

    /// <summary>
    /// positionコマンド中の違法手を無視することを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_PositionContainsIllegalMove_IgnoresIllegalMove()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("position startpos moves 3c3d");

        StringAssert.Contains(response, "info string ignore illegal move: 3c3d");
    }

    /// <summary>
    /// stopコマンドがbestmove応答を返すことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_Stop_ReturnsBestmoveResponse()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go depth 1");
        string response = engine.HandleCommand("stop");

        StringAssert.StartsWith(response, "bestmove ");
    }

    /// <summary>
    /// quitコマンドで終了フラグが立つことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_Quit_SetsShouldQuit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string response = engine.HandleCommand("quit");

        Assert.AreEqual(string.Empty, response);
        Assert.IsTrue(engine.ShouldQuit);
    }

    /// <summary>
    /// ucinewgameコマンドで開始局面に戻ることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_UciNewGame_ResetsPosition()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("position startpos moves 7g7f 3c3d");
        string beforeReset = engine.HandleCommand("go depth 1");

        engine.HandleCommand("ucinewgame");
        string afterReset = engine.HandleCommand("go depth 1");

        StringAssert.StartsWith(beforeReset, "bestmove ");
        StringAssert.StartsWith(afterReset, "bestmove ");
        Assert.AreNotEqual("bestmove resign", afterReset);
    }

    /// <summary>
    /// usinewgameコマンドでも開始局面に戻ることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_UsiNewGame_ResetsPosition()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("position startpos moves 7g7f 3c3d");
        engine.HandleCommand("usinewgame");
        string response = engine.HandleCommand("go depth 1");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreNotEqual("bestmove resign", response);
    }

    /// <summary>
    /// go depth指定が時間制御より優先されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoDepth_OverridesTimeControlDepth()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string response = engine.HandleCommand("go depth 2 movetime 10000");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreEqual(2, engine.LastSearchDepth);
    }

    /// <summary>
    /// go infiniteで既定深さが維持されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoInfinite_UsesDefaultDepth()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Depth value 1");

        string response = engine.HandleCommand("go infinite");

        Assert.AreEqual(string.Empty, response);
        Assert.AreEqual(1, engine.LastSearchDepth);

        string stopResponse = engine.HandleCommand("stop");
        StringAssert.StartsWith(stopResponse, "bestmove ");
    }

    /// <summary>
    /// setoption MoveTimeがgoの時間上限に反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionMoveTime_AffectsTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Depth value 1");
        engine.HandleCommand("setoption name MoveTime value 5000");

        string response = engine.HandleCommand("go");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreEqual(5000, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// 手番側の持ち時間に応じて探索時間上限が算出されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoTimeControl_UsesSideToMoveTimeBudget()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Depth value 1");
        engine.HandleCommand("position sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL w - 1");

        string response = engine.HandleCommand("go btime 100 wtime 200000");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreEqual(25000, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// 合法手がない局面でgoするとresignを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoNoLegalMove_ReturnsResign()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("position sfen 4k4/9/9/9/9/9/9/9/9 b - 1");

        string response = engine.HandleCommand("go depth 1");

        Assert.AreEqual("bestmove resign", response);
    }

    /// <summary>
    /// usi応答に主要オプションが含まれることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_Usi_IncludesEngineOptions()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string response = engine.HandleCommand("usi");

        StringAssert.Contains(response, "option name Depth type spin");
        StringAssert.Contains(response, "option name MoveTime type spin");
        StringAssert.Contains(response, "option name MoveOverhead type spin");
        StringAssert.Contains(response, "option name MinimumThinkingTime type spin");
        StringAssert.Contains(response, "option name SlowMover type spin");
        StringAssert.Contains(response, "option name RoundUpToFullSecond type check");
        StringAssert.Contains(response, "option name Threads type spin");
        StringAssert.Contains(response, "option name Hash type spin");
        StringAssert.Contains(response, "option name Ponder type check");
        StringAssert.Contains(response, "option name MultiPV type spin");
        StringAssert.Contains(response, "option name USI_AnalyseMode type check");
        StringAssert.Contains(response, "option name DebugLog type check");
        StringAssert.Contains(response, "option name NnueIncrementalStrict type check");
    }

    /// <summary>
    /// EvalFileに有効なモデルを設定するとNNUEが有効化されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionEvalFile_EnablesNnue()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            File.WriteAllText(modelPath, "42");
            var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

            engine.HandleCommand($"setoption name EvalFile value {modelPath}");

            Assert.IsTrue(engine.IsNnueEnabled);
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
    /// EvalFileに存在しないパスを設定するとNNUEが無効のままであることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionEvalFileMissing_KeepsNnueDisabled()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("setoption name EvalFile value C:/not-found/nn.bin");

        Assert.IsFalse(engine.IsNnueEnabled);
    }

    /// <summary>
    /// go movetime指定時に時間上限へ反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoMoveTime_SetsSearchTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go movetime 1500");

        Assert.AreEqual(1500, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// go byoyomiでdepth未指定時に既定深さ上限が使われることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoByoyomiWithoutDepth_UsesDefaultDepthCap()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go byoyomi 3000");

        Assert.AreEqual(64, engine.LastSearchDepth);
    }

    /// <summary>
    /// go byoyomi指定時に時間上限へ反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoByoyomi_SetsSearchTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go byoyomi 2000");

        Assert.AreEqual(1950, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// MoveOverhead指定時にgo byoyomiの時間上限が短縮されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionMoveOverhead_AffectsByoyomiTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name MoveOverhead value 100");

        engine.HandleCommand("go byoyomi 2000");

        Assert.AreEqual(1850, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// RoundUpToFullSecond有効時にgo byoyomiの時間上限が秒単位へ切り上がることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionRoundUpToFullSecond_AffectsByoyomiTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name RoundUpToFullSecond value true");

        engine.HandleCommand("go byoyomi 2000");

        Assert.AreEqual(2000, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// go mate指定時に通常探索フォールバックの警告を出さないことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoMate_DoesNotEmitFallbackInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("go mate 3");

        Assert.IsFalse(response.Contains("fallback to normal search", StringComparison.OrdinalIgnoreCase));
        Assert.IsTrue(response.Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .Any(line => line.StartsWith("bestmove ", StringComparison.Ordinal)));
    }

    /// <summary>
    /// go mate指定が探索条件へ反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoMate_UsesMateSearchPath()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        var outputs = new List<string>();
        engine.OutputSink = outputs.Add;

        string response = engine.HandleCommand("go mate 1");

        Assert.IsTrue(response.Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .Any(line => line.StartsWith("bestmove ", StringComparison.Ordinal)));
        Assert.IsTrue(outputs.Any(line => line.StartsWith("info depth ", StringComparison.Ordinal)));
    }

    /// <summary>
    /// 再現ログ局面でgo byoyomi 3000を実行した際に深さ情報が進行することを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_ReproPositionByoyomi3000_EmitsProgressiveDepthInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        var outputs = new List<string>();
        engine.OutputSink = outputs.Add;
        engine.HandleCommand("position startpos moves 3i3h 3c3d 2g2f 4a3b 7g7f 2b8h+ 7i8h 3a2b 5i5h 2b3c 8g8f 8c8d");

        string response = engine.HandleCommand("go btime 0 wtime 0 byoyomi 3000");

        Assert.IsTrue(response.Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .Any(line => line.StartsWith("bestmove ", StringComparison.Ordinal)));

        List<int> depths = outputs
            .Where(line => line.StartsWith("info depth ", StringComparison.Ordinal))
            .Select(ParseDepthFromInfoLine)
            .Where(depth => depth > 0)
            .ToList();

        Assert.IsTrue(depths.Count >= 2);
        Assert.IsTrue(depths[^1] >= 2);
        for (int i = 1; i < depths.Count; i++)
        {
            Assert.IsTrue(depths[i] >= depths[i - 1]);
        }
    }

    /// <summary>
    /// go持ち時間指定時に手番側の時間から上限を算出することを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoTimeControl_UsesSideToMoveBudget()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("position sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL w - 1");

        engine.HandleCommand("go btime 1000 wtime 3000 winc 300");

        Assert.AreEqual(400, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// go movestogo指定時に残り手数を考慮した時間上限が使われることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoWithMovesToGo_UsesShortHorizonTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go btime 3000 wtime 3000 binc 300 movestogo 10");

        Assert.AreEqual(600, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// Threadsオプション設定がgo実行時の探索設定へ反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionThreads_AppliesToGo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Threads value 4");

        engine.HandleCommand("go depth 1");

        Assert.AreEqual(4, engine.LastSearchThreads);
    }

    /// <summary>
    /// ponderhitコマンドを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_PonderHit_IsAccepted()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string response = engine.HandleCommand("ponderhit");

        Assert.AreEqual(string.Empty, response);
        Assert.IsFalse(engine.ShouldQuit);
    }

    /// <summary>
    /// go ponderで非同期思考を開始しstopでbestmoveを返すことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoPonder_ThenStop_ReturnsBestmove()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string ponderResponse = engine.HandleCommand("go ponder btime 10000 wtime 10000");
        string stopResponse = engine.HandleCommand("stop");

        Assert.AreEqual(string.Empty, ponderResponse);
        StringAssert.StartsWith(stopResponse, "bestmove ");
    }

    /// <summary>
    /// go infinite実行中にquitしても終了フラグが立つことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoInfinite_ThenQuit_SetsShouldQuit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string goResponse = engine.HandleCommand("go infinite");
        string quitResponse = engine.HandleCommand("quit");

        Assert.AreEqual(string.Empty, goResponse);
        Assert.AreEqual(string.Empty, quitResponse);
        Assert.IsTrue(engine.ShouldQuit);
    }

    /// <summary>
    /// go ponderでponderhit受理後にstopでbestmoveを返せることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoPonder_PonderHit_Stop_ReturnsBestmove()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string ponderResponse = engine.HandleCommand("go ponder btime 1000 wtime 1000 byoyomi 300");
        string ponderHitResponse = engine.HandleCommand("ponderhit");
        string stopResponse = engine.HandleCommand("stop");

        Assert.AreEqual(string.Empty, ponderResponse);
        Assert.AreEqual(string.Empty, ponderHitResponse);
        StringAssert.StartsWith(stopResponse, "bestmove ");
    }

    /// <summary>
    /// DebugLog有効時にsetoption適用ログがinfo stringで出力されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_DebugLogEnabled_ReturnsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name Hash value 128");

        StringAssert.Contains(response, "info string Hash=128");
    }

    /// <summary>
    /// NnueIncrementalStrictオプション設定を受理してinfo stringに反映することを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionNnueIncrementalStrict_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name NnueIncrementalStrict value true");

        StringAssert.Contains(response, "info string NnueIncrementalStrict=true");
    }

    /// <summary>
    /// DebugLog有効時にgo応答でNNUE差分統計infoが出力されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_DebugLogAndEvalFileEnabled_EmitsNnueStatsInfo()
    {
        string modelPath = Path.GetTempFileName();
        try
        {
            File.WriteAllText(modelPath, "42");
            var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
            var outputs = new List<string>();
            engine.OutputSink = outputs.Add;

            engine.HandleCommand("setoption name DebugLog value true");
            engine.HandleCommand($"setoption name EvalFile value {modelPath}");
            string response = engine.HandleCommand("go depth 1");

            Assert.IsTrue(response.Split('\n', StringSplitOptions.RemoveEmptyEntries)
                .Any(line => line.StartsWith("bestmove ", StringComparison.Ordinal)));
            Assert.IsTrue(outputs.Any(line => line.StartsWith("info string nnue stats ", StringComparison.Ordinal)));
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
    /// DebugLog有効時に探索停止理由がinfo stringで出力されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_DebugLogEnabled_EmitsStopReasonInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("go byoyomi 100");

        StringAssert.Contains(response, "info string stop reason=");
    }

    /// <summary>
    /// 反復深化の各depth完了時にinfoが出力されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoDepth_EmitsInfoPerDepth()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        var outputs = new List<string>();
        engine.OutputSink = outputs.Add;

        string response = engine.HandleCommand("go depth 2");

        StringAssert.StartsWith(response, "bestmove ");
        int infoCount = outputs.Count(line => line.StartsWith("info depth ", StringComparison.Ordinal));
        Assert.IsTrue(infoCount >= 2);
        Assert.IsTrue(outputs.Any(line => line.Contains(" pv ", StringComparison.Ordinal)));
        Assert.IsTrue(outputs.Any(line => line.Contains(" currmove ", StringComparison.Ordinal)));
    }

    /// <summary>
    /// mate閾値以上の評価でscore mateが出力されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoNoLegalMove_EmitsMateScoreInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        var outputs = new List<string>();
        engine.OutputSink = outputs.Add;
        engine.HandleCommand("position sfen 9/9/9/9/9/9/9/9/4k4 b - 1");

        string response = engine.HandleCommand("go depth 1");

        Assert.AreEqual("bestmove resign", response);
        Assert.IsTrue(outputs.Any(line => line.Contains("score mate", StringComparison.Ordinal)));
    }

    /// <summary>
    /// gameoverコマンドを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GameOver_IsAccepted()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        string response = engine.HandleCommand("gameover lose");

        Assert.AreEqual(string.Empty, response);
        Assert.IsFalse(engine.ShouldQuit);
    }

    /// <summary>
    /// info depth行から深さを抽出する。
    /// </summary>
    private static int ParseDepthFromInfoLine(string line)
    {
        string[] parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 3)
        {
            return 0;
        }

        return int.TryParse(parts[2], out int depth) ? depth : 0;
    }
}
