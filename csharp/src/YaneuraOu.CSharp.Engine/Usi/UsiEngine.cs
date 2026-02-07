using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;
using YaneuraOu.CSharp.Engine.Search;
using YaneuraOu.CSharp.Engine.Search.Time;

namespace YaneuraOu.CSharp.Engine.Usi;

/// <summary>
/// USIコマンドを処理するエンジンクラス。
/// </summary>
public sealed class UsiEngine
{
    private const int MateScore = 100_000;
    private const int MateScoreThreshold = MateScore - 512;

    private readonly string name;
    private readonly string author;
    private readonly UsiOptions options = new();
    private readonly NnueModelLoader nnueLoader = new();
    private readonly Position position = new();

    private Searcher searcher;
    private INnueBackend nnueBackend = new NullNnueBackend();
    private Move lastBestMove = Move.none();
    private Task<SearchResult>? thinkingTask;
    private CancellationTokenSource? thinkingCts;
    private readonly object thinkingLock = new();
    private readonly List<string> infoMessages = new();
    private readonly TimeManagement timeManagement = new();
    private bool isPondering;
    private LimitsType? lastLimits;

    /// <summary>
    /// UsiEngineのインスタンスを初期化する。
    /// </summary>
    public UsiEngine(string name, string author)
    {
        this.name = name;
        this.author = author;
        searcher = CreateSearcher();
        position.set(Position.StartSfen, new StateInfo());
    }

    /// <summary>
    /// 終了要求を受け取ったかどうかを返す。
    /// </summary>
    public bool ShouldQuit { get; private set; }

    /// <summary>
    /// 直近の探索で使用した深さを返す。
    /// </summary>
    public int LastSearchDepth { get; private set; }

    /// <summary>
    /// 直近の探索で使用した時間上限(ミリ秒)を返す。
    /// </summary>
    public int LastSearchTimeLimitMs { get; private set; }

    /// <summary>
    /// 直近の探索で使用したスレッド数を返す。
    /// </summary>
    public int LastSearchThreads { get; private set; }

    /// <summary>
    /// NNUE評価が有効かどうかを返す。
    /// </summary>
    public bool IsNnueEnabled => nnueBackend.IsEnabled;

    /// <summary>
    /// 即時出力の受け取り先を返す。
    /// </summary>
    public Action<string>? OutputSink { get; set; }

    /// <summary>
    /// USIコマンドを処理して応答文字列を返す。
    /// </summary>
    public string HandleCommand(string command)
    {
        string trimmed = command.Trim();
        if (string.IsNullOrEmpty(trimmed))
        {
            return string.Empty;
        }

        string response;
        if (string.Equals(trimmed, "usi", StringComparison.OrdinalIgnoreCase))
        {
            response =
                $"id name {name}\n" +
                $"id author {author}\n" +
                "option name Depth type spin default 64 min 1 max 128\n" +
                "option name MoveTime type spin default 1000 min 1 max 600000\n" +
                "option name Threads type spin default 1 min 1 max 256\n" +
                "option name Hash type spin default 64 min 1 max 8192\n" +
                "option name Ponder type check default false\n" +
                "option name MultiPV type spin default 1 min 1 max 16\n" +
                "option name USI_AnalyseMode type check default false\n" +
                "option name DebugLog type check default false\n" +
                "option name EvalFile type string default \n" +
                "usiok";
            return FlushInfo(response);
        }

        if (string.Equals(trimmed, "isready", StringComparison.OrdinalIgnoreCase))
        {
            response = "readyok";
            return FlushInfo(response);
        }

        if (string.Equals(trimmed, "ucinewgame", StringComparison.OrdinalIgnoreCase)
            || string.Equals(trimmed, "usinewgame", StringComparison.OrdinalIgnoreCase))
        {
            ResetToStartPosition();
            return FlushInfo(string.Empty);
        }

        if (string.Equals(trimmed, "stop", StringComparison.OrdinalIgnoreCase))
        {
            CompleteThinkingIfNeeded();
            response = $"bestmove {FormatBestMove(lastBestMove)}";
            return FlushInfo(response);
        }

        if (string.Equals(trimmed, "ponderhit", StringComparison.OrdinalIgnoreCase))
        {
            isPondering = false;
            timeManagement.NotifyPonderHit();
            AddInfo("ponderhit accepted");
            return FlushInfo(string.Empty);
        }

        if (trimmed.StartsWith("gameover", StringComparison.OrdinalIgnoreCase))
        {
            CompleteThinkingIfNeeded();
            isPondering = false;
            AddInfo($"gameover {trimmed[8..].Trim()}");
            return FlushInfo(string.Empty);
        }

        if (string.Equals(trimmed, "quit", StringComparison.OrdinalIgnoreCase))
        {
            CompleteThinkingIfNeeded();
            ShouldQuit = true;
            return FlushInfo(string.Empty);
        }

        if (trimmed.StartsWith("setoption", StringComparison.OrdinalIgnoreCase))
        {
            ApplySetOption(trimmed);
            return FlushInfo(string.Empty);
        }

        if (trimmed.StartsWith("position", StringComparison.OrdinalIgnoreCase))
        {
            ApplyPosition(trimmed);
            return FlushInfo(string.Empty);
        }

        if (trimmed.StartsWith("go", StringComparison.OrdinalIgnoreCase))
        {
            SearchLimits limits = ParseGoLimits(trimmed);
            LastSearchDepth = limits.Depth;
            LastSearchTimeLimitMs = limits.StopPolicy?.MaximumTimeMs ?? limits.MaxTimeMs;
            LastSearchThreads = limits.Threads;
            bool infinite = lastLimits?.Infinite ?? false;
            bool ponder = lastLimits?.Ponder ?? false;

            if (infinite || ponder)
            {
                isPondering = ponder;
                StartThinking(limits);
                return FlushInfo(string.Empty);
            }

            SearchResult result = searcher.Search(position, limits, OnSearchProgress);
            lastBestMove = result.BestMove;
            AddInfo($"search depth {limits.Depth} time {limits.MaxTimeMs} nodes {result.Nodes} score cp {result.Score}");
            AddNnueStatsInfo();
            response = $"bestmove {FormatBestMove(result.BestMove)}";
            return FlushInfo(response);
        }

        AddInfo($"ignore command: {trimmed}");
        return FlushInfo(string.Empty);
    }

    /// <summary>
    /// 対局状態を開始局面に戻す。
    /// </summary>
    private void ResetToStartPosition()
    {
        CompleteThinkingIfNeeded();
        position.set(Position.StartSfen, new StateInfo());
        lastBestMove = Move.none();
        isPondering = false;
    }

    /// <summary>
    /// setoptionコマンドを適用する。
    /// </summary>
    private void ApplySetOption(string command)
    {
        CompleteThinkingIfNeeded();
        string[] parts = command.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 5)
        {
            return;
        }

        int nameIndex = Array.FindIndex(parts, p => p.Equals("name", StringComparison.OrdinalIgnoreCase));
        int valueIndex = Array.FindIndex(parts, p => p.Equals("value", StringComparison.OrdinalIgnoreCase));
        if (nameIndex < 0 || valueIndex < 0 || valueIndex <= nameIndex + 1)
        {
            return;
        }

        string optionName = string.Join(' ', parts[(nameIndex + 1)..valueIndex]);
        string optionValue = string.Join(' ', parts[(valueIndex + 1)..]);

        if (optionName.Equals("Depth", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int depth)
            && depth > 0)
        {
            options.DefaultDepth = depth;
            AddInfo($"Depth={depth}");
        }

        if (optionName.Equals("MoveTime", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int moveTime)
            && moveTime > 0)
        {
            options.DefaultMoveTimeMs = moveTime;
            AddInfo($"MoveTime={moveTime}");
        }

        if (optionName.Equals("Threads", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int threads)
            && threads > 0)
        {
            options.Threads = threads;
            AddInfo($"Threads={threads}");
        }

        if (optionName.Equals("Hash", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int hashMb)
            && hashMb > 0)
        {
            options.HashSizeMb = hashMb;
            AddInfo($"Hash={hashMb}");
        }

        if (optionName.Equals("MultiPV", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int multiPv)
            && multiPv > 0)
        {
            options.MultiPv = multiPv;
            AddInfo($"MultiPV={multiPv}");
        }

        if (optionName.Equals("Ponder", StringComparison.OrdinalIgnoreCase))
        {
            options.PonderEnabled = ParseBooleanOption(optionValue);
            AddInfo($"Ponder={options.PonderEnabled.ToString().ToLowerInvariant()}");
        }

        if (optionName.Equals("USI_AnalyseMode", StringComparison.OrdinalIgnoreCase))
        {
            options.AnalyseMode = ParseBooleanOption(optionValue);
            AddInfo($"USI_AnalyseMode={options.AnalyseMode.ToString().ToLowerInvariant()}");
        }

        if (optionName.Equals("DebugLog", StringComparison.OrdinalIgnoreCase))
        {
            options.DebugLog = ParseBooleanOption(optionValue);
            AddInfo($"DebugLog={options.DebugLog.ToString().ToLowerInvariant()}");
        }

        if (optionName.Equals("EvalFile", StringComparison.OrdinalIgnoreCase))
        {
            options.EvalFilePath = optionValue;
            nnueBackend = nnueLoader.Load(optionValue);
            searcher = CreateSearcher();
            AddInfo(nnueBackend.IsEnabled
                ? $"NNUE enabled: {optionValue}"
                : $"NNUE disabled: failed to load {optionValue}");
        }
    }

    /// <summary>
    /// positionコマンドを適用する。
    /// </summary>
    private void ApplyPosition(string command)
    {
        CompleteThinkingIfNeeded();
        string[] parts = command.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 2)
        {
            return;
        }

        int movesIndex = Array.FindIndex(parts, p => p.Equals("moves", StringComparison.OrdinalIgnoreCase));

        if (parts[1].Equals("startpos", StringComparison.OrdinalIgnoreCase))
        {
            ResetToStartPosition();
        }
        else if (parts[1].Equals("sfen", StringComparison.OrdinalIgnoreCase))
        {
            int sfenEnd = movesIndex >= 0 ? movesIndex : parts.Length;
            if (sfenEnd > 2)
            {
                string sfen = string.Join(' ', parts.Skip(2).Take(sfenEnd - 2));
                position.set(sfen, new StateInfo());
                lastBestMove = Move.none();
            }
        }

        if (movesIndex < 0)
        {
            return;
        }

        for (int i = movesIndex + 1; i < parts.Length; i++)
        {
            Move move = ParseUsiMove(parts[i]);
            if (!move.is_ok())
            {
                continue;
            }

            position.do_move(move, new StateInfo(), position.gives_check(move));
        }

        lastBestMove = Move.none();
    }

    /// <summary>
    /// goコマンドから探索条件を取得する。
    /// </summary>
    private SearchLimits ParseGoLimits(string command)
    {
        string[] parts = command.Split(' ', StringSplitOptions.RemoveEmptyEntries);

        var limits = new LimitsType
        {
            StartTime = DateTimeOffset.UtcNow,
            Depth = options.DefaultDepth,
        };

        for (int i = 1; i < parts.Length; i++)
        {
            string token = parts[i].ToLowerInvariant();
            if (token == "infinite")
            {
                limits.Infinite = true;
                continue;
            }

            if (token == "ponder")
            {
                limits.Ponder = true;
                continue;
            }

            if (i + 1 >= parts.Length)
            {
                continue;
            }

            if (!int.TryParse(parts[i + 1], out int value))
            {
                continue;
            }

            switch (token)
            {
                case "depth":
                    limits.Depth = Math.Max(1, value);
                    i++;
                    break;
                case "movetime":
                    limits.MoveTimeMs = Math.Max(0, value);
                    i++;
                    break;
                case "byoyomi":
                    limits.ByoyomiMs = Math.Max(0, value);
                    i++;
                    break;
                case "btime":
                    limits.SetTime(Color.BLACK, value);
                    i++;
                    break;
                case "wtime":
                    limits.SetTime(Color.WHITE, value);
                    i++;
                    break;
                case "binc":
                    limits.SetIncrement(Color.BLACK, value);
                    i++;
                    break;
                case "winc":
                    limits.SetIncrement(Color.WHITE, value);
                    i++;
                    break;
                case "nodes":
                    limits.Nodes = Math.Max(0, value);
                    i++;
                    break;
                case "mate":
                    limits.Mate = Math.Max(0, value);
                    i++;
                    break;
            }
        }

        if (!limits.Infinite
            && limits.MoveTimeMs <= 0
            && limits.ByoyomiMs <= 0
            && limits.TimeMs[0] <= 0
            && limits.TimeMs[1] <= 0
            && limits.IncMs[0] <= 0
            && limits.IncMs[1] <= 0)
        {
            limits.MoveTimeMs = options.DefaultMoveTimeMs;
        }

        timeManagement.Init(limits, position.side_to_move());
        SearchStopPolicy stopPolicy = new(timeManagement, limits.Infinite, limits.Nodes, null);
        lastLimits = limits;
        if (limits.Mate > 0)
        {
            AddInfo($"go mate {limits.Mate} is not fully implemented; fallback to normal search");
        }

        return new SearchLimits
        {
            Depth = Math.Max(1, limits.Depth),
            MaxTimeMs = Math.Max(0, timeManagement.MaximumTimeMs),
            NodesLimit = limits.Nodes,
            Threads = Math.Max(1, options.Threads),
            StopPolicy = stopPolicy,
        };
    }

    /// <summary>
    /// USI文字列をMoveへ変換する。
    /// </summary>
    private Move ParseUsiMove(string usi)
    {
        if (usi.Length < 4)
        {
            return Move.none();
        }

        Color us = position.side_to_move();
        if (usi.Length >= 4 && usi[1] == '*')
        {
            PieceType pt = PieceTypeFromDropChar(usi[0]);
            Square to = ParseSquare(usi.AsSpan(2, 2).ToString());
            return to == Square.SQ_NB ? Move.none() : ShogiTypes.make_move_drop(pt, to, us);
        }

        Square from = ParseSquare(usi.AsSpan(0, 2).ToString());
        Square toSq = ParseSquare(usi.AsSpan(2, 2).ToString());
        if (from == Square.SQ_NB || toSq == Square.SQ_NB)
        {
            return Move.none();
        }

        Piece pc = position.piece_on(from);
        if (pc == Piece.NO_PIECE)
        {
            return Move.none();
        }

        bool promote = usi.Length >= 5 && usi[4] == '+';
        return promote ? ShogiTypes.make_move_promote(from, toSq, pc) : ShogiTypes.make_move(from, toSq, pc);
    }

    /// <summary>
    /// bestmove応答用の文字列へ変換する。
    /// </summary>
    private static string FormatBestMove(Move move)
    {
        return move.to_u32() == Move.none().to_u32() ? "resign" : ToUsi(move);
    }

    /// <summary>
    /// MoveをUSI文字列へ変換する。
    /// </summary>
    private static string ToUsi(Move move)
    {
        if (move.is_drop())
        {
            char pt = DropCharFromPieceType(move.move_dropped_piece());
            return $"{pt}*{SquareToUsi(move.to_sq())}";
        }

        string body = $"{SquareToUsi(move.from_sq())}{SquareToUsi(move.to_sq())}";
        return move.is_promote() ? $"{body}+" : body;
    }

    /// <summary>
    /// USI座標文字列をSquareへ変換する。
    /// </summary>
    private static Square ParseSquare(string sq)
    {
        if (sq.Length != 2)
        {
            return Square.SQ_NB;
        }

        int file = sq[0] - '1';
        int rank = sq[1] - 'a';
        if (file is < 0 or > 8 || rank is < 0 or > 8)
        {
            return Square.SQ_NB;
        }

        return (Square)(file * 9 + rank);
    }

    /// <summary>
    /// SquareをUSI座標文字列へ変換する。
    /// </summary>
    private static string SquareToUsi(Square sq)
    {
        int v = (int)sq;
        int file = v / 9;
        int rank = v % 9;
        return $"{(char)('1' + file)}{(char)('a' + rank)}";
    }

    /// <summary>
    /// 打ち駒文字をPieceTypeへ変換する。
    /// </summary>
    private static PieceType PieceTypeFromDropChar(char c)
    {
        return char.ToUpperInvariant(c) switch
        {
            'P' => PieceType.PAWN,
            'L' => PieceType.LANCE,
            'N' => PieceType.KNIGHT,
            'S' => PieceType.SILVER,
            'G' => PieceType.GOLD,
            'B' => PieceType.BISHOP,
            'R' => PieceType.ROOK,
            _ => PieceType.PAWN,
        };
    }

    /// <summary>
    /// PieceTypeを打ち駒文字へ変換する。
    /// </summary>
    private static char DropCharFromPieceType(PieceType pt)
    {
        return pt switch
        {
            PieceType.PAWN => 'P',
            PieceType.LANCE => 'L',
            PieceType.KNIGHT => 'N',
            PieceType.SILVER => 'S',
            PieceType.GOLD => 'G',
            PieceType.BISHOP => 'B',
            PieceType.ROOK => 'R',
            _ => 'P',
        };
    }

    /// <summary>
    /// 現在の設定から探索器を生成する。
    /// </summary>
    private Searcher CreateSearcher()
    {
        IEvaluator evaluator = new NnueEvaluator(nnueBackend, new MaterialEvaluator());
        return new Searcher(evaluator);
    }

    /// <summary>
    /// 非同期探索を開始する。
    /// </summary>
    private void StartThinking(SearchLimits limits)
    {
        CompleteThinkingIfNeeded();

        lock (thinkingLock)
        {
            thinkingCts = new CancellationTokenSource();
            limits.ShouldStop = () => thinkingCts.IsCancellationRequested;
            limits.StopPolicy?.SetExternalStop(limits.ShouldStop);
            thinkingTask = Task.Run(() => searcher.Search(position, limits, OnSearchProgress));
            AddInfo($"start thinking depth {limits.Depth} time {limits.MaxTimeMs} threads {limits.Threads}");
        }
    }

    /// <summary>
    /// 進行中の探索を停止して結果を取り込む。
    /// </summary>
    private void CompleteThinkingIfNeeded()
    {
        Task<SearchResult>? task;
        CancellationTokenSource? cts;
        lock (thinkingLock)
        {
            task = thinkingTask;
            cts = thinkingCts;
        }

        if (task is null || cts is null)
        {
            return;
        }

        cts.Cancel();
        try
        {
            task.Wait();
            if (task.Status == TaskStatus.RanToCompletion)
            {
                lastBestMove = task.Result.BestMove;
            }
        }
        catch (AggregateException)
        {
            // 停止要求起因の例外は握りつぶす。
        }
        finally
        {
            cts.Dispose();
            lock (thinkingLock)
            {
                thinkingTask = null;
                thinkingCts = null;
            }
        }
    }

    /// <summary>
    /// bool形式オプション文字列を解釈する。
    /// </summary>
    private static bool ParseBooleanOption(string value)
    {
        return value.Equals("true", StringComparison.OrdinalIgnoreCase)
            || value.Equals("1", StringComparison.OrdinalIgnoreCase)
            || value.Equals("on", StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// info stringメッセージを追加する。
    /// </summary>
    private void AddInfo(string message)
    {
        if (!options.DebugLog && !message.StartsWith("NNUE ", StringComparison.Ordinal))
        {
            return;
        }

        infoMessages.Add($"info string {message}");
    }

    /// <summary>
    /// 探索進捗をUSI info形式で出力する。
    /// </summary>
    private void OnSearchProgress(SearchProgress progress)
    {
        int elapsedMs = Math.Max(1, progress.ElapsedMilliseconds);
        long nps = (long)progress.Nodes * 1000L / elapsedMs;
        string scoreToken = FormatScoreToken(progress.Score);
        string currmove = progress.CurrentMove.to_u32() == Move.none().to_u32() ? "none" : ToUsi(progress.CurrentMove);
        string pv = progress.PrincipalVariation.Length == 0
            ? currmove
            : string.Join(' ', progress.PrincipalVariation.Select(ToUsi));

        string line =
            $"info depth {progress.Depth} seldepth {progress.SelDepth} score {scoreToken} nodes {progress.Nodes} nps {nps} time {elapsedMs} hashfull {progress.HashFullPermill} currmove {currmove} pv {pv}";

        EmitInfoLine(line);
    }

    /// <summary>
    /// 評価値をUSI scoreトークンへ変換する。
    /// </summary>
    private static string FormatScoreToken(int score)
    {
        int abs = Math.Abs(score);
        if (abs < MateScoreThreshold)
        {
            return $"cp {score}";
        }

        int matePly = Math.Max(1, MateScore - abs + 1);
        if (score < 0)
        {
            matePly = -matePly;
        }

        return $"mate {matePly}";
    }

    /// <summary>
    /// info行を即時または遅延で出力する。
    /// </summary>
    private void EmitInfoLine(string line)
    {
        OutputSink?.Invoke(line);
    }

    /// <summary>
    /// NNUE差分更新統計をinfo stringへ出力する。
    /// </summary>
    private void AddNnueStatsInfo()
    {
        if (!options.DebugLog)
        {
            return;
        }

        if (nnueBackend is IIncrementalNnueBackend incremental)
        {
            NnueIncrementalStats stats = incremental.GetStats();
            EmitInfoLine($"info string nnue stats rebuild={stats.RebuildCount} delta={stats.DeltaApplyCount} eval={stats.EvaluateCount}");
        }
    }

    /// <summary>
    /// info stringとコマンド応答を連結する。
    /// </summary>
    private string FlushInfo(string response)
    {
        if (infoMessages.Count == 0)
        {
            return response;
        }

        string joined = string.Join('\n', infoMessages);
        infoMessages.Clear();
        return string.IsNullOrEmpty(response) ? joined : $"{joined}\n{response}";
    }
}
