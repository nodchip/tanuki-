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
    /// go infiniteで既定深さより深い探索深さが選ばれることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoInfinite_SelectsExtendedDepth()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Depth value 1");

        string response = engine.HandleCommand("go infinite");

        Assert.AreEqual(string.Empty, response);
        Assert.AreEqual(3, engine.LastSearchDepth);

        string stopResponse = engine.HandleCommand("stop");
        StringAssert.StartsWith(stopResponse, "bestmove ");
    }

    /// <summary>
    /// setoption MoveTimeがgoの探索深さ選択に反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionMoveTime_AffectsGoDepth()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Depth value 1");
        engine.HandleCommand("setoption name MoveTime value 5000");

        string response = engine.HandleCommand("go");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreEqual(3, engine.LastSearchDepth);
    }

    /// <summary>
    /// 手番側の持ち時間に応じて探索深さが選ばれることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoTimeControl_UsesSideToMoveTime()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Depth value 1");
        engine.HandleCommand("position sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL w - 1");

        string response = engine.HandleCommand("go btime 100 wtime 200000");

        StringAssert.StartsWith(response, "bestmove ");
        Assert.AreEqual(3, engine.LastSearchDepth);
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
        StringAssert.Contains(response, "option name Threads type spin");
        StringAssert.Contains(response, "option name Hash type spin");
        StringAssert.Contains(response, "option name Ponder type check");
        StringAssert.Contains(response, "option name MultiPV type spin");
        StringAssert.Contains(response, "option name USI_AnalyseMode type check");
        StringAssert.Contains(response, "option name DebugLog type check");
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
    /// go byoyomi指定時に時間上限へ反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoByoyomi_SetsSearchTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go byoyomi 2000");

        Assert.AreEqual(2000, engine.LastSearchTimeLimitMs);
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

        Assert.AreEqual(110, engine.LastSearchTimeLimitMs);
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
}
