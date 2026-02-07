using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;
using YaneuraOu.CSharp.Engine.Eval;
using YaneuraOu.CSharp.Engine.Search;

namespace YaneuraOu.CSharp.Engine.Usi;

/// <summary>
/// USIコマンドを処理するエンジンクラス。
/// </summary>
public sealed class UsiEngine
{
    private readonly string name;
    private readonly string author;
    private readonly UsiOptions options = new();
    private readonly NnueModelLoader nnueLoader = new();
    private readonly Position position = new();

    private Searcher searcher;
    private INnueBackend nnueBackend = new NullNnueBackend();
    private Move lastBestMove = Move.none();

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
    /// NNUE評価が有効かどうかを返す。
    /// </summary>
    public bool IsNnueEnabled => nnueBackend.IsEnabled;

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

        if (string.Equals(trimmed, "usi", StringComparison.OrdinalIgnoreCase))
        {
            return
                $"id name {name}\n" +
                $"id author {author}\n" +
                "option name Depth type spin default 1 min 1 max 64\n" +
                "option name MoveTime type spin default 1000 min 1 max 600000\n" +
                "option name EvalFile type string default \n" +
                "usiok";
        }

        if (string.Equals(trimmed, "isready", StringComparison.OrdinalIgnoreCase))
        {
            return "readyok";
        }

        if (string.Equals(trimmed, "ucinewgame", StringComparison.OrdinalIgnoreCase)
            || string.Equals(trimmed, "usinewgame", StringComparison.OrdinalIgnoreCase))
        {
            ResetToStartPosition();
            return string.Empty;
        }

        if (string.Equals(trimmed, "stop", StringComparison.OrdinalIgnoreCase))
        {
            return $"bestmove {FormatBestMove(lastBestMove)}";
        }

        if (string.Equals(trimmed, "quit", StringComparison.OrdinalIgnoreCase))
        {
            ShouldQuit = true;
            return string.Empty;
        }

        if (trimmed.StartsWith("setoption", StringComparison.OrdinalIgnoreCase))
        {
            ApplySetOption(trimmed);
            return string.Empty;
        }

        if (trimmed.StartsWith("position", StringComparison.OrdinalIgnoreCase))
        {
            ApplyPosition(trimmed);
            return string.Empty;
        }

        if (trimmed.StartsWith("go", StringComparison.OrdinalIgnoreCase))
        {
            SearchLimits limits = ParseGoLimits(trimmed);
            LastSearchDepth = limits.Depth;
            SearchResult result = searcher.Search(position, limits);
            lastBestMove = result.BestMove;
            return $"bestmove {FormatBestMove(result.BestMove)}";
        }

        return string.Empty;
    }

    /// <summary>
    /// 対局状態を開始局面に戻す。
    /// </summary>
    private void ResetToStartPosition()
    {
        position.set(Position.StartSfen, new StateInfo());
        lastBestMove = Move.none();
    }

    /// <summary>
    /// setoptionコマンドを適用する。
    /// </summary>
    private void ApplySetOption(string command)
    {
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
        }

        if (optionName.Equals("MoveTime", StringComparison.OrdinalIgnoreCase)
            && int.TryParse(optionValue, out int moveTime)
            && moveTime > 0)
        {
            options.DefaultMoveTimeMs = moveTime;
        }

        if (optionName.Equals("EvalFile", StringComparison.OrdinalIgnoreCase))
        {
            options.EvalFilePath = optionValue;
            nnueBackend = nnueLoader.Load(optionValue);
            searcher = CreateSearcher();
        }
    }

    /// <summary>
    /// positionコマンドを適用する。
    /// </summary>
    private void ApplyPosition(string command)
    {
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

        int depth = 0;
        int movetimeMs = 0;
        int byoyomiMs = 0;
        int btimeMs = 0;
        int wtimeMs = 0;
        int bincMs = 0;
        int wincMs = 0;
        bool infinite = false;

        for (int i = 1; i < parts.Length; i++)
        {
            string token = parts[i].ToLowerInvariant();
            if (token == "infinite")
            {
                infinite = true;
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
                    depth = value;
                    i++;
                    break;
                case "movetime":
                    movetimeMs = value;
                    i++;
                    break;
                case "byoyomi":
                    byoyomiMs = value;
                    i++;
                    break;
                case "btime":
                    btimeMs = value;
                    i++;
                    break;
                case "wtime":
                    wtimeMs = value;
                    i++;
                    break;
                case "binc":
                    bincMs = value;
                    i++;
                    break;
                case "winc":
                    wincMs = value;
                    i++;
                    break;
            }
        }

        if (depth <= 0)
        {
            depth = SelectDepthFromTimeControl(movetimeMs, byoyomiMs, btimeMs, wtimeMs, bincMs, wincMs, infinite);
        }

        return new SearchLimits { Depth = Math.Max(1, depth) };
    }

    /// <summary>
    /// 時間情報から暫定的な探索深さを決定する。
    /// </summary>
    private int SelectDepthFromTimeControl(int movetimeMs, int byoyomiMs, int btimeMs, int wtimeMs, int bincMs, int wincMs, bool infinite)
    {
        if (infinite)
        {
            return options.DefaultDepth + 2;
        }

        int sideTime = position.side_to_move() == Color.BLACK ? btimeMs + bincMs : wtimeMs + wincMs;
        int candidate = movetimeMs;
        if (candidate <= 0)
        {
            if (byoyomiMs > 0)
            {
                candidate = byoyomiMs;
            }
            else if (sideTime > 0)
            {
                candidate = Math.Max(100, sideTime / 30);
            }
            else
            {
                candidate = options.DefaultMoveTimeMs;
            }
        }

        if (candidate >= 5000)
        {
            return options.DefaultDepth + 2;
        }

        if (candidate >= 2000)
        {
            return options.DefaultDepth + 1;
        }

        return options.DefaultDepth;
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
}
