using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;
using YaneuraOu.CSharp.Engine.Search;
using YaneuraOu.CSharp.Engine.Search.Time;
using System.Diagnostics;

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
    private readonly List<Searcher> lazySmpSearchers = new();
    private readonly TimeManagement timeManagement = new();
    private bool isPondering;
    private LimitsType? lastLimits;
    private SearchStopPolicy? activeStopPolicy;

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
                "option name MoveOverhead type spin default 0 min 0 max 10000\n" +
                "option name MinimumThinkingTime type spin default 2000 min 1 max 100000\n" +
                "option name SlowMover type spin default 100 min 1 max 1000\n" +
                "option name RoundUpToFullSecond type check default false\n" +
                "option name NetworkDelay type spin default 0 min 0 max 10000\n" +
                "option name NetworkDelay2 type spin default 0 min 0 max 10000\n" +
                "option name UseNullMovePruning type check default true\n" +
                "option name UseLmr type check default true\n" +
                "option name UseAspirationWindow type check default true\n" +
                "option name Threads type spin default 1 min 1 max 256\n" +
                "option name Hash type spin default 64 min 1 max 8192\n" +
                "option name Ponder type check default false\n" +
                "option name MultiPV type spin default 1 min 1 max 16\n" +
                "option name USI_AnalyseMode type check default false\n" +
                "option name DebugLog type check default false\n" +
                "option name NnueIncrementalStrict type check default false\n" +
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

            SearchResult result = SearchWithLazySmpIfNeeded(limits);
            lastBestMove = EnsureSafeBestMove(result.BestMove);
            AddInfo($"search depth {limits.Depth} time {limits.MaxTimeMs} nodes {result.Nodes} score cp {result.Score}");
            AddStopInfo(limits.StopPolicy, result.Nodes);
            AddNnueStatsInfo();
            response = $"bestmove {FormatBestMove(lastBestMove)}";
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

        if (optionName.Equals("MoveOverhead", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int moveOverhead)
            && moveOverhead >= 0)
        {
            options.MoveOverheadMs = moveOverhead;
            AddInfo($"MoveOverhead={moveOverhead}");
        }

        if (optionName.Equals("MinimumThinkingTime", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int minimumThinkingTime)
            && minimumThinkingTime > 0)
        {
            options.MinimumThinkingTimeMs = minimumThinkingTime;
            AddInfo($"MinimumThinkingTime={minimumThinkingTime}");
        }

        if (optionName.Equals("SlowMover", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int slowMover)
            && slowMover > 0)
        {
            options.SlowMover = slowMover;
            AddInfo($"SlowMover={slowMover}");
        }

        if (optionName.Equals("RoundUpToFullSecond", StringComparison.OrdinalIgnoreCase))
        {
            options.RoundUpToFullSecond = ParseBooleanOption(optionValue);
            AddInfo($"RoundUpToFullSecond={options.RoundUpToFullSecond.ToString().ToLowerInvariant()}");
        }

        if (optionName.Equals("NetworkDelay", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int networkDelay)
            && networkDelay >= 0)
        {
            options.NetworkDelayMs = networkDelay;
            AddInfo($"NetworkDelay={networkDelay}");
        }

        if (optionName.Equals("NetworkDelay2", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int networkDelay2)
            && networkDelay2 >= 0)
        {
            options.NetworkDelay2Ms = networkDelay2;
            AddInfo($"NetworkDelay2={networkDelay2}");
        }

        if (optionName.Equals("UseNullMovePruning", StringComparison.OrdinalIgnoreCase))
        {
            options.UseNullMovePruning = ParseBooleanOption(optionValue);
            searcher = CreateSearcher();
            InvalidateLazySmpSearchers();
            AddInfo($"UseNullMovePruning={options.UseNullMovePruning.ToString().ToLowerInvariant()}");
        }

        if (optionName.Equals("UseLmr", StringComparison.OrdinalIgnoreCase))
        {
            options.UseLmr = ParseBooleanOption(optionValue);
            searcher = CreateSearcher();
            InvalidateLazySmpSearchers();
            AddInfo($"UseLmr={options.UseLmr.ToString().ToLowerInvariant()}");
        }

        if (optionName.Equals("UseAspirationWindow", StringComparison.OrdinalIgnoreCase))
        {
            options.UseAspirationWindow = ParseBooleanOption(optionValue);
            searcher = CreateSearcher();
            InvalidateLazySmpSearchers();
            AddInfo($"UseAspirationWindow={options.UseAspirationWindow.ToString().ToLowerInvariant()}");
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

        if (optionName.Equals("NnueIncrementalStrict", StringComparison.OrdinalIgnoreCase))
        {
            options.NnueIncrementalStrict = ParseBooleanOption(optionValue);
            AddInfo($"NnueIncrementalStrict={options.NnueIncrementalStrict.ToString().ToLowerInvariant()}");
            if (!string.IsNullOrWhiteSpace(options.EvalFilePath))
            {
                nnueBackend = nnueLoader.Load(options.EvalFilePath, options.NnueIncrementalStrict);
                searcher = CreateSearcher();
                InvalidateLazySmpSearchers();
                AddInfo(nnueBackend.IsEnabled
                    ? $"NNUE reloaded strict={options.NnueIncrementalStrict.ToString().ToLowerInvariant()}: {options.EvalFilePath}"
                    : $"NNUE disabled: failed to load {options.EvalFilePath}");
            }
        }

        if (optionName.Equals("EvalFile", StringComparison.OrdinalIgnoreCase))
        {
            options.EvalFilePath = optionValue;
            nnueBackend = nnueLoader.Load(optionValue, options.NnueIncrementalStrict);
            searcher = CreateSearcher();
            InvalidateLazySmpSearchers();
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
            string usiMoveText = parts[i];
            Move move = ParseUsiMove(usiMoveText);
            if (!move.is_ok())
            {
                move = ResolveLegalMoveFromUsi(usiMoveText);
                if (!move.is_ok())
                {
                    AddInfo($"ignore illegal move: {usiMoveText}");
                    break;
                }
            }

            position.do_move(move, new StateInfo(), position.gives_check(move));
        }

        lastBestMove = Move.none();
    }

    /// <summary>
    /// 現局面の合法手からUSI文字列に一致する手を解決する。
    /// </summary>
    private Move ResolveLegalMoveFromUsi(string usiMoveText)
    {
        MoveList legalMoves = MoveGenerator.GenerateLegal(position);
        for (int i = 0; i < legalMoves.Count; i++)
        {
            Move legal = legalMoves[i];
            if (string.Equals(ToUsi(legal), usiMoveText, StringComparison.OrdinalIgnoreCase))
            {
                return legal;
            }
        }

        return Move.none();
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
            GamePly = position.game_ply(),
            MoveOverheadMs = options.MoveOverheadMs,
            MinimumThinkingTimeMs = options.MinimumThinkingTimeMs,
            SlowMover = options.SlowMover,
            RoundUpToFullSecond = options.RoundUpToFullSecond,
            NetworkDelayMs = options.NetworkDelayMs,
            NetworkDelay2Ms = options.NetworkDelay2Ms,
            PonderEnabledOption = options.PonderEnabled,
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
                case "movestogo":
                    limits.MovesToGo = Math.Max(0, value);
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
        AddInfo(timeManagement.LastSummary);
        SearchStopPolicy stopPolicy = new(timeManagement, limits.Infinite, limits.Nodes, null, limits.Ponder);
        lastLimits = limits;
        return new SearchLimits
        {
            Depth = Math.Max(1, limits.Depth),
            MaxTimeMs = Math.Max(0, timeManagement.MaximumTimeMs),
            NodesLimit = limits.Nodes,
            Threads = Math.Max(1, options.Threads),
            StopPolicy = stopPolicy,
            MateMoves = Math.Max(0, limits.Mate),
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
        if (pc == Piece.NO_PIECE || ShogiTypes.color_of(pc) != us)
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
        var features = new SearchFeatures
        {
            EnableNullMovePruning = options.UseNullMovePruning,
            EnableLmr = options.UseLmr,
            EnableAspirationWindow = options.UseAspirationWindow,
        };
        return new Searcher(evaluator, features);
    }

    /// <summary>
    /// LazySMPを必要に応じて適用して探索を実行する。
    /// </summary>
    private SearchResult SearchWithLazySmpIfNeeded(SearchLimits limits)
    {
        int workers = Math.Max(1, limits.Threads);
        if (workers <= 1 || limits.MateMoves > 0)
        {
            return searcher.Search(position, limits, OnSearchProgress);
        }

        AddInfo($"lazysmp workers={workers}");
        string rootSfen = position.sfen();
        EnsureLazySmpSearchers(workers);
        var sharedTimer = Stopwatch.StartNew();
        Func<bool> sharedStop = () =>
        {
            if (limits.ShouldStop is not null && limits.ShouldStop())
            {
                return true;
            }

            return limits.MaxTimeMs > 0 && sharedTimer.ElapsedMilliseconds >= limits.MaxTimeMs;
        };

        var tasks = new Task<(int WorkerId, SearchResult Result, int PvLength, int CompletedDepth)>[workers];
        for (int workerId = 0; workerId < workers; workerId++)
        {
            int capturedWorkerId = workerId;
            tasks[workerId] = Task.Factory.StartNew(() =>
            {
                var workerPosition = new Position();
                workerPosition.set(rootSfen, new StateInfo());
                Searcher workerSearcher = lazySmpSearchers[capturedWorkerId];
                SearchLimits workerLimits = CloneWorkerLimits(limits, sharedStop);
                int latestPvLength = 0;
                int latestCompletedDepth = 0;
                SearchResult workerResult = workerSearcher.Search(
                    workerPosition,
                    workerLimits,
                    progress =>
                    {
                        latestPvLength = progress.PrincipalVariation.Length;
                        latestCompletedDepth = progress.Depth;
                        if (capturedWorkerId == 0)
                        {
                            OnSearchProgress(progress);
                        }
                    });

                if (latestPvLength <= 0 && workerResult.BestMove.to_u32() != Move.none().to_u32())
                {
                    latestPvLength = 1;
                }

                if (latestCompletedDepth <= 0)
                {
                    latestCompletedDepth = workerResult.Depth;
                }

                return (capturedWorkerId, workerResult, latestPvLength, latestCompletedDepth);
            },
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default);
        }

        Task.WaitAll(tasks);

        SearchResult[] workerResults = tasks.Select(task => task.Result.Result).ToArray();
        int[] workerPvLengths = tasks.Select(task => task.Result.PvLength).ToArray();
        int[] workerCompletedDepths = tasks.Select(task => task.Result.CompletedDepth).ToArray();
        int winnerIndex = SelectLazySmpWinnerIndex(workerResults, workerPvLengths, workerCompletedDepths);
        if (winnerIndex < 0 || winnerIndex >= tasks.Length)
        {
            winnerIndex = 0;
        }

        (int WorkerId, SearchResult Result, int PvLength, int CompletedDepth) selected = tasks[winnerIndex].Result;
        SearchResult resolved = ResolveLazySmpResult(workerResults, selected.Result);
        if (resolved.BestMove.to_u32() != selected.Result.BestMove.to_u32())
        {
            AddInfo("lazysmp fallback to non-resign worker");
        }

        AddInfo($"lazysmp selected_worker={selected.WorkerId} score={selected.Result.Score} depth={selected.Result.Depth}");
        return resolved;
    }

    /// <summary>
    /// 並列探索ワーカー向けの探索器を生成する。
    /// </summary>
    private Searcher CreateParallelWorkerSearcher()
    {
        INnueBackend backend = string.IsNullOrWhiteSpace(options.EvalFilePath)
            ? new NullNnueBackend()
            : nnueLoader.Load(options.EvalFilePath, options.NnueIncrementalStrict);
        IEvaluator evaluator = new NnueEvaluator(backend, new MaterialEvaluator());
        var features = new SearchFeatures
        {
            EnableNullMovePruning = options.UseNullMovePruning,
            EnableLmr = options.UseLmr,
            EnableAspirationWindow = options.UseAspirationWindow,
        };

        return new Searcher(evaluator, features);
    }

    /// <summary>
    /// LazySMP用の探索器を必要数まで確保する。
    /// </summary>
    private void EnsureLazySmpSearchers(int workers)
    {
        while (lazySmpSearchers.Count < workers)
        {
            lazySmpSearchers.Add(CreateParallelWorkerSearcher());
        }
    }

    /// <summary>
    /// LazySMP用探索器キャッシュを無効化する。
    /// </summary>
    private void InvalidateLazySmpSearchers()
    {
        lazySmpSearchers.Clear();
    }

    /// <summary>
    /// 並列探索ワーカー向けに探索条件を複製する。
    /// </summary>
    private static SearchLimits CloneWorkerLimits(SearchLimits source, Func<bool> sharedStop)
    {
        return new SearchLimits
        {
            Depth = source.Depth,
            MaxTimeMs = source.MaxTimeMs,
            NodesLimit = source.NodesLimit,
            Threads = 1,
            ShouldStop = sharedStop,
            MateMoves = source.MateMoves,
        };
    }

    /// <summary>
    /// LazySMPのwinner indexを選出する。
    /// </summary>
    private static int SelectLazySmpWinnerIndex(SearchResult[] workers, int[] pvLengths, int[] completedDepths)
    {
        if (workers.Length == 0 || workers.Length != pvLengths.Length || workers.Length != completedDepths.Length)
        {
            return -1;
        }

        int minScore = workers.Min(result => result.Score);
        var voteByMove = new Dictionary<uint, long>(workers.Length * 2);
        long[] threadVotingValue = new long[workers.Length];

        for (int i = 0; i < workers.Length; i++)
        {
            SearchResult worker = workers[i];
            int completedDepth = Math.Max(1, completedDepths[i]);
            long value = ((long)worker.Score - minScore + 14L) * completedDepth;
            threadVotingValue[i] = value;
            uint move = worker.BestMove.to_u32();
            if (!voteByMove.TryAdd(move, value))
            {
                voteByMove[move] += value;
            }
        }

        int best = 0;
        for (int i = 1; i < workers.Length; i++)
        {
            SearchResult bestResult = workers[best];
            SearchResult candidate = workers[i];
            int bestScore = bestResult.Score;
            int candidateScore = candidate.Score;

            long bestMoveVote = voteByMove[bestResult.BestMove.to_u32()];
            long candidateMoveVote = voteByMove[candidate.BestMove.to_u32()];

            bool bestInProvenWin = IsProvenWin(bestScore);
            bool candidateInProvenWin = IsProvenWin(candidateScore);
            bool bestInProvenLoss = IsProvenLoss(bestScore);
            bool candidateInProvenLoss = IsProvenLoss(candidateScore);

            bool betterVotingValue =
                threadVotingValue[i] * (pvLengths[i] > 2 ? 1 : 0)
                > threadVotingValue[best] * (pvLengths[best] > 2 ? 1 : 0);

            if (bestInProvenWin)
            {
                if (candidateScore > bestScore)
                {
                    best = i;
                }
            }
            else if (bestInProvenLoss)
            {
                if (candidateInProvenLoss && candidateScore < bestScore)
                {
                    best = i;
                }
            }
            else if (candidateInProvenWin
                || candidateInProvenLoss
                || (!IsProvenLoss(candidateScore)
                    && (candidateMoveVote > bestMoveVote
                        || (candidateMoveVote == bestMoveVote && betterVotingValue))))
            {
                best = i;
            }
        }

        return best;
    }

    /// <summary>
    /// LazySMPの選出結果を安全に確定する。
    /// </summary>
    private static SearchResult ResolveLazySmpResult(SearchResult[] workers, SearchResult selected)
    {
        if (selected.BestMove.to_u32() != Move.none().to_u32())
        {
            return selected;
        }

        for (int i = 0; i < workers.Length; i++)
        {
            if (workers[i].BestMove.to_u32() != Move.none().to_u32())
            {
                return workers[i];
            }
        }

        return selected;
    }

    /// <summary>
    /// 詰み確定勝ち相当の評価値かどうかを判定する。
    /// </summary>
    private static bool IsProvenWin(int score)
    {
        return score >= MateScoreThreshold;
    }

    /// <summary>
    /// 詰み確定負け相当の評価値かどうかを判定する。
    /// </summary>
    private static bool IsProvenLoss(int score)
    {
        if (score == int.MinValue)
        {
            return false;
        }

        return score <= -MateScoreThreshold;
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
            activeStopPolicy = limits.StopPolicy;
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
                lastBestMove = EnsureSafeBestMove(task.Result.BestMove);
                AddStopInfo(activeStopPolicy, task.Result.Nodes);
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
                activeStopPolicy = null;
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
    /// bestmove候補が合法か検証し、非合法なら合法手へフォールバックする。
    /// </summary>
    private Move EnsureSafeBestMove(Move move)
    {
        Move legal = EnsureLegalBestMove(move);
        if (legal.to_u32() == Move.none().to_u32())
        {
            return Move.none();
        }

        if (IsUsiRoundTripLegal(legal))
        {
            return legal;
        }

        AddInfo($"illegal usi bestmove filtered: {ToUsi(legal)}");
        return SelectFallbackMove();
    }

    /// <summary>
    /// 探索結果の指し手を合法手へ補正する。
    /// </summary>
    private Move EnsureLegalBestMove(Move move)
    {
        if (move.to_u32() == Move.none().to_u32())
        {
            Move fallback = SelectFallbackMove();
            return fallback.to_u32() == Move.none().to_u32() ? Move.none() : fallback;
        }

        if (IsLegalMove(move))
        {
            return move;
        }

        AddInfo($"illegal bestmove filtered: {ToUsi(move)}");
        return SelectFallbackMove();
    }

    /// <summary>
    /// 指し手が現局面で合法かどうかを判定する。
    /// </summary>
    private bool IsLegalMove(Move move)
    {
        if (!move.is_ok())
        {
            return false;
        }

        MoveList legalMoves = MoveGenerator.GenerateLegal(position);
        uint target = move.to_u32();
        for (int i = 0; i < legalMoves.Count; i++)
        {
            if (legalMoves[i].to_u32() == target)
            {
                return true;
            }
        }

        return false;
    }

    /// <summary>
    /// USI文字列への往復変換後も合法手であることを確認する。
    /// </summary>
    private bool IsUsiRoundTripLegal(Move move)
    {
        string usi = ToUsi(move);
        Move reparsed = ParseUsiMove(usi);
        return IsLegalMove(reparsed);
    }

    /// <summary>
    /// 合法手一覧からフォールバック用の手を選択する。
    /// </summary>
    private Move SelectFallbackMove()
    {
        MoveList legalMoves = MoveGenerator.GenerateLegal(position);
        if (legalMoves.Count == 0)
        {
            return Move.none();
        }

        return legalMoves[0];
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
            EmitInfoLine(
                $"info string nnue stats rebuild={stats.RebuildCount} delta={stats.DeltaApplyCount} eval={stats.EvaluateCount} verify={stats.VerificationCount} mismatch={stats.MismatchCount}");
        }
    }

    /// <summary>
    /// 探索停止理由をinfo stringへ出力する。
    /// </summary>
    private void AddStopInfo(SearchStopPolicy? stopPolicy, long nodes)
    {
        if (!options.DebugLog)
        {
            return;
        }

        string reason = stopPolicy?.LastReason ?? "none";
        int elapsed = stopPolicy?.ElapsedMilliseconds ?? 0;
        AddInfo($"stop reason={reason} elapsed={elapsed} nodes={nodes}");
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


