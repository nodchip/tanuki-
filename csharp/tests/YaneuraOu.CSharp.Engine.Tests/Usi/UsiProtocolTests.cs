using Microsoft.VisualStudio.TestTools.UnitTesting;
using System.Diagnostics;
using System.Reflection;
using System.Threading;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Search;
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
    /// 同期探索完了後のstopコマンドは応答を返さないことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_StopAfterSyncGo_ReturnsEmptyResponse()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go depth 1");
        string response = engine.HandleCommand("stop");

        Assert.AreEqual(string.Empty, response);
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
    /// MaxMovesToDrawを小さくすると同条件で思考時間上限が増えることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionMaxMovesToDraw_AffectsTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go btime 60000 wtime 60000");
        int baseline = engine.LastSearchTimeLimitMs;

        engine.HandleCommand("setoption name MaxMovesToDraw value 32");
        engine.HandleCommand("go btime 60000 wtime 60000");
        int updated = engine.LastSearchTimeLimitMs;

        Assert.IsTrue(updated > baseline, $"baseline={baseline} updated={updated}");
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
        Assert.IsTrue(engine.LastSearchTimeLimitMs > 0);
        Assert.IsTrue(engine.LastSearchTimeLimitMs <= 200000);
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
        StringAssert.Contains(response, "option name MaxMovesToDraw type spin");
        StringAssert.Contains(response, "option name MoveOverhead type spin");
        StringAssert.Contains(response, "option name MinimumThinkingTime type spin");
        StringAssert.Contains(response, "option name SlowMover type spin");
        StringAssert.Contains(response, "option name RoundUpToFullSecond type check");
        StringAssert.Contains(response, "option name NetworkDelay type spin");
        StringAssert.Contains(response, "option name NetworkDelay2 type spin");
        StringAssert.Contains(response, "option name UseNullMovePruning type check");
        StringAssert.Contains(response, "option name UseLmr type check");
        StringAssert.Contains(response, "option name UseAspirationWindow type check");
        StringAssert.Contains(response, "option name Threads type spin");
        StringAssert.Contains(response, "option name USI_Hash type spin");
        StringAssert.Contains(response, "option name Clear Hash type button");
        StringAssert.Contains(response, "option name USI_Ponder type check");
        StringAssert.Contains(response, "option name MultiPV type spin");
        StringAssert.Contains(response, "option name USI_AnalyseMode type check");
        StringAssert.Contains(response, "option name USI_ShowCurrLine type check");
        StringAssert.Contains(response, "option name USI_ShowRefutations type check");
        StringAssert.Contains(response, "option name USI_LimitStrength type check");
        StringAssert.Contains(response, "option name USI_Elo type spin");
        StringAssert.Contains(response, "option name USI_OwnBook type check");
        StringAssert.Contains(response, "option name BookFile type string");
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

        Assert.AreEqual(2000, engine.LastSearchTimeLimitMs);
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

        Assert.AreEqual(1900, engine.LastSearchTimeLimitMs);
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
    /// NetworkDelay指定時にgo byoyomiの時間上限が短縮されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionNetworkDelay_AffectsByoyomiTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name NetworkDelay value 120");

        engine.HandleCommand("go byoyomi 2000");

        Assert.AreEqual(1880, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// NetworkDelay2指定時にgo byoyomiの時間上限が短縮されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionNetworkDelay2_AffectsByoyomiTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name NetworkDelay2 value 300");

        engine.HandleCommand("go byoyomi 2000");

        Assert.AreEqual(1700, engine.LastSearchTimeLimitMs);
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

        Assert.AreEqual(2000, engine.LastSearchTimeLimitMs);
    }

    /// <summary>
    /// go movestogo指定時に残り手数を考慮した時間上限が使われることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoWithMovesToGo_UsesShortHorizonTimeLimit()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");

        engine.HandleCommand("go btime 3000 wtime 3000 binc 300 movestogo 10");

        Assert.AreEqual(2000, engine.LastSearchTimeLimitMs);
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
    /// Threadsが2以上のときにLazySMPが有効化されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoWithMultipleThreads_EmitsLazySmpInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");
        engine.HandleCommand("setoption name Threads value 3");

        string response = engine.HandleCommand("go depth 1");

        StringAssert.Contains(response, "info string lazysmp workers=3");
        StringAssert.Contains(response, "bestmove ");
    }

    /// <summary>
    /// LazySMP winner選出が1位手の投票と深さ重みを優先することを検証する。
    /// </summary>
    [TestMethod]
    public void LazySmpWinnerSelection_UsesVoteAndDepthWeight()
    {
        MethodInfo? method = typeof(UsiEngine).GetMethod(
            "SelectLazySmpWinnerIndex",
            BindingFlags.Static | BindingFlags.NonPublic);
        Assert.IsNotNull(method);

        SearchResult[] workers =
        [
            new SearchResult(new Move(1), 100, 1000, 1),
            new SearchResult(new Move(2), 110, 1200, 1),
            new SearchResult(new Move(1), 105, 3000, 4),
        ];
        int[] pvLengths = [3, 3, 3];
        int[] completedDepths = [1, 1, 4];

        object? result = method.Invoke(null, [workers, pvLengths, completedDepths]);
        Assert.IsNotNull(result);
        Assert.AreEqual(2, (int)result);
    }

    /// <summary>
    /// LazySMPの同票比較でPVが短い候補を選ばないことを検証する。
    /// </summary>
    [TestMethod]
    public void LazySmpWinnerSelection_ShortPvDoesNotWinTieBreak()
    {
        MethodInfo? method = typeof(UsiEngine).GetMethod(
            "SelectLazySmpWinnerIndex",
            BindingFlags.Static | BindingFlags.NonPublic);
        Assert.IsNotNull(method);

        SearchResult[] workers =
        [
            new SearchResult(new Move(1), 100, 1000, 4),
            new SearchResult(new Move(1), 105, 1200, 5),
            new SearchResult(new Move(2), 90, 500, 1),
        ];
        int[] pvLengths = [3, 1, 3];
        int[] completedDepths = [4, 5, 1];

        object? result = method.Invoke(null, [workers, pvLengths, completedDepths]);
        Assert.IsNotNull(result);
        Assert.AreEqual(0, (int)result);
    }

    /// <summary>
    /// LazySMP winner選出で未探索の最小値スコアを必敗扱いしないことを検証する。
    /// </summary>
    [TestMethod]
    public void LazySmpWinnerSelection_IgnoresSentinelLossScore()
    {
        MethodInfo? method = typeof(UsiEngine).GetMethod(
            "SelectLazySmpWinnerIndex",
            BindingFlags.Static | BindingFlags.NonPublic);
        Assert.IsNotNull(method);

        SearchResult[] workers =
        [
            new SearchResult(new Move(0x1234), 120, 1000, 3),
            new SearchResult(Move.none(), int.MinValue, 0, 1),
            new SearchResult(Move.none(), int.MinValue, 0, 1),
        ];
        int[] pvLengths = [3, 0, 0];
        int[] completedDepths = [3, 1, 1];

        object? result = method.Invoke(null, [workers, pvLengths, completedDepths]);
        Assert.IsNotNull(result);
        Assert.AreEqual(0, (int)result);
    }

    /// <summary>
    /// LazySMPで選出結果がnoneのときに非none候補へフォールバックすることを検証する。
    /// </summary>
    [TestMethod]
    public void ResolveLazySmpResult_SelectedNone_FallsBackToNonNone()
    {
        MethodInfo? method = typeof(UsiEngine).GetMethod(
            "ResolveLazySmpResult",
            BindingFlags.Static | BindingFlags.NonPublic);
        Assert.IsNotNull(method);

        SearchResult[] workers =
        [
            new SearchResult(Move.none(), int.MinValue, 0, 1),
            new SearchResult(new Move(0x2345), 50, 500, 2),
            new SearchResult(Move.none(), int.MinValue, 0, 1),
        ];
        SearchResult selected = workers[0];

        object? result = method.Invoke(null, [workers, selected]);
        Assert.IsNotNull(result);
        SearchResult resolved = (SearchResult)result;
        Assert.AreEqual(workers[1].BestMove.to_u32(), resolved.BestMove.to_u32());
    }

    /// <summary>
    /// bestmove候補がnoneでも合法手がある局面ではフォールバックできることを検証する。
    /// </summary>
    [TestMethod]
    public void EnsureSafeBestMove_NoneCandidate_FallsBackToLegalMove()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("position startpos");

        MethodInfo? method = typeof(UsiEngine).GetMethod(
            "EnsureSafeBestMove",
            BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.IsNotNull(method);

        object? result = method.Invoke(engine, [Move.none()]);
        Assert.IsNotNull(result);
        Move move = (Move)result;
        Assert.AreNotEqual(Move.none().to_u32(), move.to_u32());
    }

    /// <summary>
    /// 複雑局面のposition適用後に手番がずれないことを検証する。
    /// </summary>
    [TestMethod]
    public void ApplyPosition_ComplexSequence_KeepsExpectedSideToMove()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        string[] moves = "2g2f 3c3d 7g7f 2b8h+ 7i8h 3a2b B*5b 6a5b 1g1f 2b3c 6i5h 4a3b 4i4h B*5e 8h7g 8c8d 2f2e 8d8e 2e2d 2c2d 5i6i 7a7b 6i7h 5a4b 5g5f 5e6d 6g6f 4b3a 2h2d 3c2d 5f5e R*2g 3i3h 2g2h+ P*2e 2d2e 5h6g 2h1i"
            .Split(' ', StringSplitOptions.RemoveEmptyEntries);

        FieldInfo? field = typeof(UsiEngine).GetField("position", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.IsNotNull(field);
        for (int i = 1; i <= moves.Length; i++)
        {
            string command = $"position startpos moves {string.Join(' ', moves.Take(i))}";
            engine.HandleCommand(command);
            var pos = field.GetValue(engine) as Position;
            Assert.IsNotNull(pos);

            Color expected = i % 2 == 0 ? Color.BLACK : Color.WHITE;
            Assert.AreEqual(expected, pos.side_to_move(), $"手番がずれています i={i} move={moves[i - 1]}");
        }
    }

    /// <summary>
    /// LazySMP有効時でも秒読み超過が過大にならないことを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoByoyomiWithThreads_DoesNotOverrunSeverely()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name Threads value 4");

        var stopwatch = Stopwatch.StartNew();
        string response = engine.HandleCommand("go btime 0 wtime 0 byoyomi 3000");
        stopwatch.Stop();

        StringAssert.Contains(response, "bestmove ");
        Assert.IsTrue(stopwatch.ElapsedMilliseconds <= 4500, $"elapsed={stopwatch.ElapsedMilliseconds}");
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
    /// go infiniteでThreadsが2以上のときに非同期探索でもLazySMP情報が出力されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_GoInfiniteWithThreads_ThenStop_EmitsLazySmpInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");
        engine.HandleCommand("setoption name Threads value 3");

        string goResponse = engine.HandleCommand("go infinite");
        Thread.Sleep(100);
        string stopResponse = engine.HandleCommand("stop");

        Assert.IsTrue(goResponse.Contains("info string start thinking", StringComparison.Ordinal));
        StringAssert.Contains(stopResponse, "info string lazysmp workers=3");
        StringAssert.Contains(stopResponse, "bestmove ");
    }

    /// <summary>
    /// DebugLog有効時にsetoption適用ログがinfo stringで出力されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_DebugLogEnabled_ReturnsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name USI_Hash value 128");

        StringAssert.Contains(response, "info string USI_Hash=128");
    }

    /// <summary>
    /// 互換性のため旧Hashオプション名も受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionLegacyHash_AcceptsAlias()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name Hash value 256");

        StringAssert.Contains(response, "info string USI_Hash=256");
    }

    /// <summary>
    /// USI_Hash設定が探索器の置換表容量へ反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionHash_AppliesTranspositionCapacity()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        FieldInfo? searcherField = typeof(UsiEngine).GetField("searcher", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.IsNotNull(searcherField);
        var searcher = searcherField.GetValue(engine) as Searcher;
        Assert.IsNotNull(searcher);

        int before = searcher.TranspositionCapacityEntries;
        engine.HandleCommand("setoption name USI_Hash value 1");
        int after = searcher.TranspositionCapacityEntries;

        Assert.IsTrue(after < before, $"before={before} after={after}");
    }

    /// <summary>
    /// Clear Hashオプションで置換表をクリアできることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionClearHash_ClearsTranspositionTable()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");
        engine.HandleCommand("go depth 2");

        FieldInfo? searcherField = typeof(UsiEngine).GetField("searcher", BindingFlags.Instance | BindingFlags.NonPublic);
        Assert.IsNotNull(searcherField);
        var searcher = searcherField.GetValue(engine) as Searcher;
        Assert.IsNotNull(searcher);
        Assert.IsTrue(searcher.TranspositionEntryCount > 0);

        string response = engine.HandleCommand("setoption name Clear Hash");

        Assert.AreEqual(0, searcher.TranspositionEntryCount);
        StringAssert.Contains(response, "info string Clear Hash=ok");
    }

    /// <summary>
    /// 互換性のため旧Ponderオプション名も受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionLegacyPonder_AcceptsAlias()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name Ponder value true");

        StringAssert.Contains(response, "info string USI_Ponder=true");
    }

    /// <summary>
    /// USI_OwnBookオプションを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionUsiOwnBook_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name USI_OwnBook value true");

        StringAssert.Contains(response, "info string USI_OwnBook=true");
    }

    /// <summary>
    /// BookFileオプションを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionBookFile_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name BookFile value book/standard.db");

        StringAssert.Contains(response, "info string BookFile=book/standard.db");
    }

    /// <summary>
    /// MaxMovesToDrawオプションを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionMaxMovesToDraw_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name MaxMovesToDraw value 320");

        StringAssert.Contains(response, "info string MaxMovesToDraw=320");
    }

    /// <summary>
    /// USI_LimitStrengthオプションを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionUsiLimitStrength_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name USI_LimitStrength value true");

        StringAssert.Contains(response, "info string USI_LimitStrength=true");
    }

    /// <summary>
    /// USI_Eloオプションを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionUsiElo_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name USI_Elo value 1800");

        StringAssert.Contains(response, "info string USI_Elo=1800");
    }

    /// <summary>
    /// USI_ShowCurrLineオプションを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionUsiShowCurrLine_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name USI_ShowCurrLine value true");

        StringAssert.Contains(response, "info string USI_ShowCurrLine=true");
    }

    /// <summary>
    /// USI_ShowCurrLine有効時にinfo currlineを出力することを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_UsiShowCurrLineEnabled_EmitsCurrLineInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        var outputs = new List<string>();
        engine.OutputSink = outputs.Add;

        engine.HandleCommand("setoption name USI_ShowCurrLine value true");
        engine.HandleCommand("go depth 1");

        Assert.IsTrue(outputs.Any(line => line.StartsWith("info currline 1 ", StringComparison.Ordinal)));
    }

    /// <summary>
    /// USI_ShowRefutationsオプションを受理できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionUsiShowRefutations_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("setoption name USI_ShowRefutations value true");

        StringAssert.Contains(response, "info string USI_ShowRefutations=true");
    }

    /// <summary>
    /// USI_ShowRefutations有効時にinfo refutationを出力することを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_UsiShowRefutationsEnabled_EmitsRefutationInfo()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        var outputs = new List<string>();
        engine.OutputSink = outputs.Add;

        engine.HandleCommand("setoption name USI_ShowRefutations value true");
        engine.HandleCommand("go depth 1");

        Assert.IsTrue(outputs.Any(line => line.StartsWith("info refutation ", StringComparison.Ordinal)));
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
    /// 探索機能フラグをsetoptionで切替できることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_SetOptionSearchFeatureFlags_EmitsInfoString()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response1 = engine.HandleCommand("setoption name UseNullMovePruning value false");
        string response2 = engine.HandleCommand("setoption name UseLmr value false");
        string response3 = engine.HandleCommand("setoption name UseAspirationWindow value false");

        StringAssert.Contains(response1, "info string UseNullMovePruning=false");
        StringAssert.Contains(response2, "info string UseLmr=false");
        StringAssert.Contains(response3, "info string UseAspirationWindow=false");
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
    /// DebugLog有効時に通常ケースでtmログがstatus=okを出力することを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_DebugLogEnabled_EmitsTmStatusOk()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");

        string response = engine.HandleCommand("go byoyomi 1000");

        StringAssert.Contains(response, "info string tm status=ok");
    }

    /// <summary>
    /// 時間予算不足時にフォールバックし、tmログへ反映されることを検証する。
    /// </summary>
    [TestMethod]
    public void HandleCommand_TimeBudgetUnderflow_UsesFallbackAndLogsTmStatus()
    {
        var engine = new UsiEngine("YaneuraOu.CSharp", "hakubishin");
        engine.HandleCommand("setoption name DebugLog value true");
        engine.HandleCommand("setoption name MoveOverhead value 5000");

        string response = engine.HandleCommand("go byoyomi 1000");

        StringAssert.Contains(response, "info string tm status=fallback:");
        Assert.AreEqual(1000, engine.LastSearchTimeLimitMs);
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
        Assert.IsTrue(outputs.Any(line => line.Contains(" multipv 1 ", StringComparison.Ordinal)));
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





