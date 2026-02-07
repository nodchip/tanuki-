using System.Collections.Generic;
using System.Text;
using YaneuraOu.CSharp.Engine.Core.Board;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Core;

public sealed class Position
{
    public const string StartSfen = "lnsgkgsnl/1r5b1/p1pppp1pp/6p2/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL b - 1";

    private readonly Piece[] board = new Piece[(int)Square.SQ_NB_PLUS1];
    private readonly Bitboard[] byTypeBB = new Bitboard[(int)PieceType.PIECE_BB_NB];
    private readonly Bitboard[] byColorBB = new Bitboard[(int)Color.COLOR_NB];
    private readonly uint[] hand = new uint[(int)Color.COLOR_NB];
    private readonly Square[] kingSquare = new Square[(int)Color.COLOR_NB];

    private StateInfo st;
    private int gamePly = 1;
    private Color sideToMove = Color.BLACK;

    private readonly Stack<PositionSnapshot> moveHistory = new();
    private readonly Stack<PositionSnapshot> nullMoveHistory = new();

    public Position()
    {
        Clear();
        st = new StateInfo();
    }

    public Position set(string sfenStr, StateInfo si)
    {
        if (si is null)
        {
            throw new ArgumentNullException(nameof(si));
        }

        Clear();
        st = si;

        string[] parts = sfenStr.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 4)
        {
            throw new ArgumentException("Invalid SFEN", nameof(sfenStr));
        }

        ParseBoard(parts[0]);
        sideToMove = parts[1] == "w" ? Color.WHITE : Color.BLACK;
        ParseHands(parts[2]);
        gamePly = int.TryParse(parts[3], out int gp) ? gp : 1;

        st.hand = hand[(int)sideToMove];
        st.repetition = 0;
        st.repetition_times = 0;
        st.pliesFromNull = 0;
        st.checkersBB = new Bitboard(0);
        st.continuousCheck[(int)Color.BLACK] = 0;
        st.continuousCheck[(int)Color.WHITE] = 0;

        return this;
    }

    public string sfen() => sfen(gamePly);

    public string sfen(int gamePlyValue)
    {
        return $"{SerializeBoard()} {(sideToMove == Color.WHITE ? "w" : "b")} {SerializeHands()} {gamePlyValue}";
    }

    public string flipped_sfen() => flipped_sfen(gamePly);

    public string flipped_sfen(int gamePlyValue)
    {
        var flippedBoard = new Piece[(int)Square.SQ_NB_PLUS1];
        for (int sq = 0; sq < (int)Square.SQ_NB; sq++)
        {
            int dst = (int)ShogiTypes.Flip((Square)sq);
            Piece pc = board[sq];
            flippedBoard[dst] = pc == Piece.NO_PIECE
                ? Piece.NO_PIECE
                : ShogiTypes.make_piece(ShogiTypes.color_of(pc) == Color.BLACK ? Color.WHITE : Color.BLACK, ShogiTypes.type_of(pc));
        }

        uint blackHand = hand[(int)Color.WHITE];
        uint whiteHand = hand[(int)Color.BLACK];

        string boardText = SerializeBoard(flippedBoard);
        string handText = SerializeHands(blackHand, whiteHand);
        string side = sideToMove == Color.WHITE ? "b" : "w";

        return $"{boardText} {side} {handText} {gamePlyValue}";
    }

    public static string sfen_to_flipped_sfen(string sfenStr)
    {
        var pos = new Position();
        var si = new StateInfo();
        pos.set(sfenStr, si);
        return pos.flipped_sfen(pos.gamePly);
    }

    public Piece piece_on(Square sq) => board[(int)sq];

    public Bitboard pieces() => byTypeBB[(int)PieceType.ALL_PIECES];

    public Bitboard pieces(Color c) => byColorBB[(int)c];

    public StateInfo state() => st;
    public Color side_to_move() => sideToMove;
    public uint hand_of(Color c) => hand[(int)c];
    public Square king_square(Color c) => kingSquare[(int)c];

    public void put_piece(Piece pc, Square sq)
    {
        board[(int)sq] = pc;

        int pt = (int)ShogiTypes.type_of(pc);
        byTypeBB[(int)PieceType.ALL_PIECES] = byTypeBB[(int)PieceType.ALL_PIECES] | sq;
        byTypeBB[pt] = byTypeBB[pt] | sq;

        Color c = ShogiTypes.color_of(pc);
        byColorBB[(int)c] = byColorBB[(int)c] | sq;

        if (ShogiTypes.type_of(pc) == PieceType.KING)
        {
            kingSquare[(int)c] = sq;
        }
    }

    public void remove_piece(Square sq)
    {
        Piece pc = board[(int)sq];
        if (pc == Piece.NO_PIECE)
        {
            return;
        }

        int pt = (int)ShogiTypes.type_of(pc);
        byTypeBB[(int)PieceType.ALL_PIECES] = byTypeBB[(int)PieceType.ALL_PIECES] ^ sq;
        byTypeBB[pt] = byTypeBB[pt] ^ sq;

        Color c = ShogiTypes.color_of(pc);
        byColorBB[(int)c] = byColorBB[(int)c] ^ sq;

        board[(int)sq] = Piece.NO_PIECE;
    }

    public Bitboard attackers_to(Color c, Square sq)
    {
        var result = new Bitboard(0);
        for (int i = 0; i < (int)Square.SQ_NB; i++)
        {
            Square from = (Square)i;
            Piece pc = board[i];
            if (pc == Piece.NO_PIECE || ShogiTypes.color_of(pc) != c)
            {
                continue;
            }

            if (IsAttackingSquare(from, pc, sq))
            {
                result = result | from;
            }
        }

        return result;
    }

    public Bitboard attackers_to(Square sq)
    {
        return attackers_to(Color.BLACK, sq) | attackers_to(Color.WHITE, sq);
    }

    public Bitboard blockers_for_king(Color c) => st.blockersForKing[(int)c];

    public Bitboard pinners(Color c) => st.pinners[(int)c];

    public void update_slider_blockers(Color c)
    {
        st.blockersForKing[(int)c] = new Bitboard(0);
        st.pinners[(int)c] = new Bitboard(0);

        Square ksq = kingSquare[(int)c];
        if (ksq == Square.SQ_NB)
        {
            return;
        }

        (int df, int dr)[] directions =
        {
            (-1, -1), (0, -1), (1, -1),
            (-1, 0),            (1, 0),
            (-1, 1),  (0, 1),   (1, 1),
        };

        foreach ((int df, int dr) in directions)
        {
            Square firstBlocker = Square.SQ_NB;
            bool foundBlocker = false;

            int file = FileOf(ksq) + df;
            int rank = RankOf(ksq) + dr;
            while (IsInside(file, rank))
            {
                Square sq = ToSquare(file, rank);
                Piece pc = piece_on(sq);
                if (pc != Piece.NO_PIECE)
                {
                    if (!foundBlocker)
                    {
                        firstBlocker = sq;
                        foundBlocker = true;
                    }
                    else
                    {
                        if (ShogiTypes.color_of(pc) != c && IsSliderAttackerInDirection(pc, df, dr))
                        {
                            st.blockersForKing[(int)c] = st.blockersForKing[(int)c] | firstBlocker;
                            st.pinners[(int)c] = st.pinners[(int)c] | sq;
                        }

                        break;
                    }
                }

                file += df;
                rank += dr;
            }
        }
    }

    public bool gives_check(Move m)
    {
        if (!m.is_ok())
        {
            return false;
        }

        Color us = sideToMove;
        Color them = Opposite(us);
        Square enemyKing = kingSquare[(int)them];
        if (enemyKing == Square.SQ_NB)
        {
            return false;
        }

        if (m.is_drop())
        {
            Square to = m.to_sq();
            Piece dropped = m.moved_after_piece();
            put_piece(dropped, to);
            bool check = IsAttackingSquare(to, dropped, enemyKing);
            remove_piece(to);
            return check;
        }

        Square fromSq = m.from_sq();
        Square toSq = m.to_sq();
        Piece fromPiece = piece_on(fromSq);
        Piece captured = piece_on(toSq);
        Piece movedAfter = m.moved_after_piece();

        remove_piece(fromSq);
        if (captured != Piece.NO_PIECE)
        {
            remove_piece(toSq);
        }

        put_piece(movedAfter, toSq);
        bool gives = IsAttackingSquare(toSq, movedAfter, enemyKing);

        remove_piece(toSq);
        if (captured != Piece.NO_PIECE)
        {
            put_piece(captured, toSq);
        }

        put_piece(fromPiece, fromSq);
        return gives;
    }
    public bool in_check()
    {
        Color us = sideToMove;
        Square ksq = kingSquare[(int)us];
        if (ksq == Square.SQ_NB)
        {
            return false;
        }

        return attackers_to(Opposite(us), ksq).PopCount() > 0;
    }

    public bool pseudo_legal(Move m) => pseudo_legal(m, false);

    public bool pseudo_legal(Move m, bool generateAllLegalMoves)
    {
        if (!m.is_ok())
        {
            return false;
        }

        Color us = sideToMove;
        Square to = m.to_sq();
        if ((int)to < 0 || (int)to >= (int)Square.SQ_NB)
        {
            return false;
        }

        if (m.is_drop())
        {
            PieceType pt = m.move_dropped_piece();
            if (m.moved_after_piece() != ShogiTypes.make_piece(us, pt))
            {
                return false;
            }

            if (pt < PieceType.PAWN || pt >= PieceType.KING)
            {
                return false;
            }

            if (ShogiTypes.hand_count(hand[(int)us], pt) <= 0)
            {
                return false;
            }

            if (piece_on(to) != Piece.NO_PIECE)
            {
                return false;
            }

            if (pt == PieceType.PAWN && !legal_pawn_drop(us, to))
            {
                return false;
            }

            if (in_check())
            {
                Bitboard checkers = attackers_to(Opposite(us), kingSquare[(int)us]);
                Square checkerSq = checkers.Pop();
                if (checkers.PopCount() > 0)
                {
                    return false;
                }

                if (!IsCaptureOrInterpose(us, checkerSq, to))
                {
                    return false;
                }
            }

            return true;
        }

        Square from = m.from_sq();
        if ((int)from < 0 || (int)from >= (int)Square.SQ_NB || from == to)
        {
            return false;
        }

        Piece moving = piece_on(from);
        if (moving == Piece.NO_PIECE || ShogiTypes.color_of(moving) != us)
        {
            return false;
        }

        Piece captured = piece_on(to);
        if (captured != Piece.NO_PIECE && ShogiTypes.color_of(captured) == us)
        {
            return false;
        }

        if (!IsAttackingSquare(from, moving, to))
        {
            return false;
        }

        PieceType movingType = ShogiTypes.type_of(moving);
        if (!IsPromotionStateValid(us, movingType, from, to, m.is_promote(), generateAllLegalMoves))
        {
            return false;
        }

        if (m.is_promote())
        {
            if (m.moved_after_piece() != ShogiTypes.make_promoted_piece(moving))
            {
                return false;
            }
        }
        else if (m.moved_after_piece() != moving)
        {
            return false;
        }

        if (in_check() && movingType != PieceType.KING)
        {
            Bitboard checkers = attackers_to(Opposite(us), kingSquare[(int)us]);
            Square checkerSq = checkers.Pop();
            if (checkers.PopCount() > 0)
            {
                return false;
            }

            if (!IsCaptureOrInterpose(us, checkerSq, to))
            {
                return false;
            }
        }

        return true;
    }

    public bool legal(Move m)
    {
        if (m.is_drop())
        {
            return true;
        }

        Color us = sideToMove;
        Square from = m.from_sq();
        Square to = m.to_sq();
        Piece moving = piece_on(from);
        PieceType pt = ShogiTypes.type_of(moving);

        if (pt == PieceType.KING)
        {
            return IsLegalKingMove(us, from, to);
        }

        update_slider_blockers(us);
        return !blockers_for_king(us).Test(from) || IsAligned(from, to, kingSquare[(int)us]);
    }

    public bool legal_drop(Square to)
    {
        Color us = sideToMove;
        Color them = Opposite(us);
        Square enemyKing = kingSquare[(int)them];
        if (enemyKing == Square.SQ_NB || !IsPawnAttack(us, to, enemyKing))
        {
            return true;
        }

        // C++ legal_drop() と同様に、打った歩を支える自駒の利きがなければ打ち歩詰めにはならない。
        if (attackers_to(us, to).PopCount() == 0)
        {
            return true;
        }

        Piece droppedPawn = ShogiTypes.make_piece(us, PieceType.PAWN);
        put_piece(droppedPawn, to);

        bool hasEscape = HasKingEscapeAfterPawnDrop(us, them, enemyKing) || CanCaptureDroppedPawn(us, them, enemyKing, to);

        remove_piece(to);

        return hasEscape;
    }

    /// <summary>
    /// 歩打ちが二歩および打ち歩詰めでないかを判定する。
    /// </summary>
    public bool legal_pawn_drop(Color us, Square to)
    {
        if (HasUnpromotedPawnOnFile(us, FileOf(to)))
        {
            return false;
        }

        Color them = Opposite(us);
        Square enemyKing = kingSquare[(int)them];
        if (enemyKing != Square.SQ_NB && IsPawnAttack(us, to, enemyKing))
        {
            return legal_drop(to);
        }

        return true;
    }

    public bool is_mated()
    {
        if (!in_check())
        {
            return false;
        }

        MoveList legalMoves = MoveGenerator.GenerateLegal(this);
        return legalMoves.Count == 0;
    }

    public bool legal_promote(Move m)
    {
        if (!m.is_promote())
        {
            return true;
        }

        Color us = sideToMove;
        return IsPromotionZone(us, m.from_sq()) || IsPromotionZone(us, m.to_sq());
    }

    public Move to_move(Move16 m16)
    {
        if (!m16.is_ok())
        {
            return new Move(m16.to_u16());
        }

        if (m16.is_drop())
        {
            Piece dropped = ShogiTypes.make_piece(sideToMove, m16.move_dropped_piece());
            return new Move((uint)m16.to_u16() + ((uint)dropped << 16));
        }

        Square from = m16.from_sq();
        Piece pc = piece_on(from);
        if (pc == Piece.NO_PIECE || ShogiTypes.color_of(pc) != sideToMove)
        {
            return Move.none();
        }

        if (m16.is_promote() && IsNonPromotablePiece(pc))
        {
            return Move.none();
        }

        Piece movedAfter = m16.is_promote() ? ShogiTypes.make_promoted_piece(pc) : pc;
        return new Move((uint)m16.to_u16() + ((uint)movedAfter << 16));
    }

    public void do_move(Move m, StateInfo newSt, bool givesCheck)
    {
        moveHistory.Push(CreateSnapshot());

        StateInfo prevSt = st;
        Color us = sideToMove;
        Color them = Opposite(us);
        Piece captured = Piece.NO_PIECE;

        if (m.is_drop())
        {
            PieceType pt = m.move_dropped_piece();
            if (ShogiTypes.hand_count(hand[(int)us], pt) > 0)
            {
                ShogiTypes.sub_hand(ref hand[(int)us], pt);
            }

            put_piece(m.moved_after_piece(), m.to_sq());
        }
        else
        {
            Square from = m.from_sq();
            Square to = m.to_sq();

            Piece movingBefore = piece_on(from);
            captured = piece_on(to);
            if (captured != Piece.NO_PIECE)
            {
                remove_piece(to);
                PieceType pr = ShogiTypes.raw_type_of(captured);
                ShogiTypes.add_hand(ref hand[(int)us], pr);
            }

            remove_piece(from);
            Piece movedAfter = m.moved_after_piece();
            if (movedAfter == Piece.NO_PIECE)
            {
                movedAfter = movingBefore;
            }

            put_piece(movedAfter, to);
        }

        newSt.previous = prevSt;
        newSt.capturedPiece = captured;
        newSt.repetition = 0;
        newSt.repetition_times = 0;
        newSt.pliesFromNull = prevSt.pliesFromNull + 1;
        newSt.continuousCheck[(int)us] = givesCheck ? prevSt.continuousCheck[(int)us] + 2 : 0;
        newSt.continuousCheck[(int)them] = prevSt.continuousCheck[(int)them];
        if (givesCheck)
        {
            Square enemyKing = kingSquare[(int)them];
            newSt.checkersBB = enemyKing == Square.SQ_NB ? new Bitboard(0) : attackers_to(us, enemyKing);
        }
        else
        {
            newSt.checkersBB = new Bitboard(0);
        }

        st = newSt;

        sideToMove = them;
        gamePly++;
        st.hand = hand[(int)sideToMove];
    }

    public void do_move(Move m, StateInfo newSt)
    {
        do_move(m, newSt, gives_check(m));
    }

    public void undo_move(Move m)
    {
        if (moveHistory.Count == 0)
        {
            return;
        }

        RestoreSnapshot(moveHistory.Pop());
    }

    public void do_null_move(StateInfo newSt)
    {
        nullMoveHistory.Push(CreateSnapshot());

        StateInfo prevSt = st;
        Color us = sideToMove;

        newSt.previous = prevSt;
        newSt.capturedPiece = Piece.NO_PIECE;
        newSt.repetition = 0;
        newSt.repetition_times = 0;
        newSt.pliesFromNull = 0;
        newSt.checkersBB = new Bitboard(0);
        Array.Copy(prevSt.continuousCheck, newSt.continuousCheck, prevSt.continuousCheck.Length);
        newSt.continuousCheck[(int)us] = 0;
        st = newSt;

        sideToMove = Opposite(sideToMove);
        gamePly++;
        st.hand = hand[(int)sideToMove];
    }

    public void undo_null_move()
    {
        if (nullMoveHistory.Count == 0)
        {
            return;
        }

        RestoreSnapshot(nullMoveHistory.Pop());
    }

    private void Clear()
    {
        for (int i = 0; i < board.Length; i++)
        {
            board[i] = Piece.NO_PIECE;
        }

        for (int i = 0; i < byTypeBB.Length; i++)
        {
            byTypeBB[i] = new Bitboard(0);
        }

        for (int i = 0; i < byColorBB.Length; i++)
        {
            byColorBB[i] = new Bitboard(0);
            hand[i] = 0U;
            kingSquare[i] = Square.SQ_NB;
        }
    }

    private void ParseBoard(string boardText)
    {
        string[] rows = boardText.Split('/');
        if (rows.Length != 9)
        {
            throw new ArgumentException("Invalid board rows", nameof(boardText));
        }

        for (int rank = 0; rank < 9; rank++)
        {
            int fileFromLeft = 0;
            string row = rows[rank];
            for (int i = 0; i < row.Length; i++)
            {
                char token = row[i];
                if (char.IsDigit(token))
                {
                    fileFromLeft += token - '0';
                    continue;
                }

                bool promote = false;
                if (token == '+')
                {
                    promote = true;
                    i++;
                    token = row[i];
                }

                int file = 8 - fileFromLeft;
                Square sq = ToSquare(file, rank);
                Piece pc = CharToPiece(token, promote);
                put_piece(pc, sq);
                fileFromLeft++;
            }
        }
    }

    private void ParseHands(string handText)
    {
        if (handText == "-")
        {
            return;
        }

        int i = 0;
        while (i < handText.Length)
        {
            int count = 0;
            while (i < handText.Length && char.IsDigit(handText[i]))
            {
                count = count * 10 + (handText[i] - '0');
                i++;
            }

            if (count == 0)
            {
                count = 1;
            }

            char token = handText[i++];
            bool white = char.IsLower(token);
            PieceType pt = CharToPieceType(char.ToUpperInvariant(token));
            int side = white ? (int)Color.WHITE : (int)Color.BLACK;

            for (int c = 0; c < count; c++)
            {
                ShogiTypes.add_hand(ref hand[side], pt);
            }
        }
    }

    private string SerializeBoard() => SerializeBoard(board);

    private static string SerializeBoard(Piece[] boardData)
    {
        var sb = new StringBuilder();
        for (int rank = 0; rank < 9; rank++)
        {
            int empty = 0;
            for (int fileFromLeft = 0; fileFromLeft < 9; fileFromLeft++)
            {
                int file = 8 - fileFromLeft;
                Square sq = ToSquare(file, rank);
                Piece pc = boardData[(int)sq];

                if (pc == Piece.NO_PIECE)
                {
                    empty++;
                    continue;
                }

                if (empty > 0)
                {
                    sb.Append(empty);
                    empty = 0;
                }

                AppendPieceString(sb, pc);
            }

            if (empty > 0)
            {
                sb.Append(empty);
            }

            if (rank != 8)
            {
                sb.Append('/');
            }
        }

        return sb.ToString();
    }

    private string SerializeHands() => SerializeHands(hand[(int)Color.BLACK], hand[(int)Color.WHITE]);

    private static string SerializeHands(uint blackHand, uint whiteHand)
    {
        var sb = new StringBuilder();
        AppendHand(sb, blackHand, false);
        AppendHand(sb, whiteHand, true);
        return sb.Length == 0 ? "-" : sb.ToString();
    }

    private static void AppendHand(StringBuilder sb, uint handValue, bool white)
    {
        PieceType[] order =
        {
            PieceType.ROOK,
            PieceType.BISHOP,
            PieceType.GOLD,
            PieceType.SILVER,
            PieceType.KNIGHT,
            PieceType.LANCE,
            PieceType.PAWN,
        };

        foreach (PieceType pt in order)
        {
            int count = ShogiTypes.hand_count(handValue, pt);
            if (count <= 0)
            {
                continue;
            }

            if (count > 1)
            {
                sb.Append(count);
            }

            char c = PieceTypeToChar(pt);
            sb.Append(white ? char.ToLowerInvariant(c) : c);
        }
    }

    private static Piece CharToPiece(char token, bool promote)
    {
        bool white = char.IsLower(token);
        PieceType pt = CharToPieceType(char.ToUpperInvariant(token));
        if (promote)
        {
            pt = (PieceType)((int)pt | (int)PieceType.PIECE_TYPE_PROMOTE);
        }

        return ShogiTypes.make_piece(white ? Color.WHITE : Color.BLACK, pt);
    }

    private static PieceType CharToPieceType(char token)
    {
        return token switch
        {
            'P' => PieceType.PAWN,
            'L' => PieceType.LANCE,
            'N' => PieceType.KNIGHT,
            'S' => PieceType.SILVER,
            'B' => PieceType.BISHOP,
            'R' => PieceType.ROOK,
            'G' => PieceType.GOLD,
            'K' => PieceType.KING,
            _ => throw new ArgumentException($"Invalid piece token: {token}"),
        };
    }

    private static char PieceTypeToChar(PieceType pt)
    {
        if (pt == PieceType.KING)
        {
            return 'K';
        }

        PieceType raw = (PieceType)((int)pt & 7);
        return raw switch
        {
            PieceType.PAWN => 'P',
            PieceType.LANCE => 'L',
            PieceType.KNIGHT => 'N',
            PieceType.SILVER => 'S',
            PieceType.BISHOP => 'B',
            PieceType.ROOK => 'R',
            PieceType.GOLD => 'G',
            _ => throw new ArgumentException($"Invalid piece type: {pt}"),
        };
    }

    private static void AppendPieceString(StringBuilder sb, Piece pc)
    {
        PieceType pt = ShogiTypes.type_of(pc);
        bool promoted = ((int)pt & (int)PieceType.PIECE_TYPE_PROMOTE) != 0 && pt != PieceType.KING;
        if (promoted)
        {
            sb.Append('+');
        }

        char c = PieceTypeToChar(pt);
        if (ShogiTypes.color_of(pc) == Color.WHITE)
        {
            c = char.ToLowerInvariant(c);
        }

        sb.Append(c);
    }

    private bool IsAttackingSquare(Square from, Piece piece, Square target)
    {
        if (from == target)
        {
            return false;
        }

        PieceType pt = ShogiTypes.type_of(piece);
        Color color = ShogiTypes.color_of(piece);

        int ff = FileOf(from);
        int fr = RankOf(from);
        int tf = FileOf(target);
        int tr = RankOf(target);
        int df = tf - ff;
        int dr = tr - fr;
        int adf = Math.Abs(df);
        int adr = Math.Abs(dr);

        bool IsStep(int sFile, int sRank) => df == sFile && dr == sRank;
        int forward = color == Color.BLACK ? -1 : 1;

        switch (pt)
        {
            case PieceType.PAWN:
                return IsStep(0, forward);
            case PieceType.LANCE:
                if (df != 0)
                {
                    return false;
                }

                if ((color == Color.BLACK && dr >= 0) || (color == Color.WHITE && dr <= 0))
                {
                    return false;
                }

                return IsClearRay(from, target, 0, Math.Sign(dr));
            case PieceType.KNIGHT:
                return IsStep(-1, 2 * forward) || IsStep(1, 2 * forward);
            case PieceType.SILVER:
                if (color == Color.BLACK)
                {
                    return IsStep(0, -1) || IsStep(-1, -1) || IsStep(1, -1) || IsStep(-1, 1) || IsStep(1, 1);
                }

                return IsStep(0, 1) || IsStep(-1, 1) || IsStep(1, 1) || IsStep(-1, -1) || IsStep(1, -1);
            case PieceType.GOLD:
            case PieceType.PRO_PAWN:
            case PieceType.PRO_LANCE:
            case PieceType.PRO_KNIGHT:
            case PieceType.PRO_SILVER:
                if (color == Color.BLACK)
                {
                    return IsStep(0, -1) || IsStep(-1, -1) || IsStep(1, -1) || IsStep(-1, 0) || IsStep(1, 0) || IsStep(0, 1);
                }

                return IsStep(0, 1) || IsStep(-1, 1) || IsStep(1, 1) || IsStep(-1, 0) || IsStep(1, 0) || IsStep(0, -1);
            case PieceType.BISHOP:
                return adf == adr && adf > 0 && IsClearRay(from, target, Math.Sign(df), Math.Sign(dr));
            case PieceType.ROOK:
                return ((df == 0 && dr != 0) || (df != 0 && dr == 0))
                       && IsClearRay(from, target, Math.Sign(df), Math.Sign(dr));
            case PieceType.KING:
                return adf <= 1 && adr <= 1;
            case PieceType.HORSE:
                return (adf == adr && adf > 0 && IsClearRay(from, target, Math.Sign(df), Math.Sign(dr)))
                       || (adf <= 1 && adr <= 1 && (adf + adr) == 1);
            case PieceType.DRAGON:
                return ((((df == 0 && dr != 0) || (df != 0 && dr == 0))
                         && IsClearRay(from, target, Math.Sign(df), Math.Sign(dr))))
                       || (adf == 1 && adr == 1);
            default:
                return false;
        }
    }

    private bool IsClearRay(Square from, Square to, int stepFile, int stepRank)
    {
        int file = FileOf(from) + stepFile;
        int rank = RankOf(from) + stepRank;
        int tf = FileOf(to);
        int tr = RankOf(to);
        while (file != tf || rank != tr)
        {
            if (!IsInside(file, rank))
            {
                return false;
            }

            if (piece_on(ToSquare(file, rank)) != Piece.NO_PIECE)
            {
                return false;
            }

            file += stepFile;
            rank += stepRank;
        }

        return true;
    }

    private bool IsSliderAttackerInDirection(Piece piece, int rayDf, int rayDr)
    {
        PieceType pt = ShogiTypes.type_of(piece);
        Color color = ShogiTypes.color_of(piece);

        bool orth = rayDf == 0 || rayDr == 0;
        bool diag = Math.Abs(rayDf) == Math.Abs(rayDr);

        if (pt == PieceType.ROOK || pt == PieceType.DRAGON)
        {
            return orth;
        }

        if (pt == PieceType.BISHOP || pt == PieceType.HORSE)
        {
            return diag;
        }

        if (pt == PieceType.LANCE)
        {
            return rayDf == 0
                   && ((color == Color.BLACK && rayDr < 0) || (color == Color.WHITE && rayDr > 0));
        }

        return false;
    }

    private static Color Opposite(Color c) => c == Color.BLACK ? Color.WHITE : Color.BLACK;

    private static int FileOf(Square sq) => (int)sq / 9;
    private static int RankOf(Square sq) => (int)sq % 9;
    private static bool IsInside(int file, int rank) => file >= 0 && file < 9 && rank >= 0 && rank < 9;
    private bool HasUnpromotedPawnOnFile(Color us, int file)
    {
        for (int rank = 0; rank < 9; rank++)
        {
            Piece pc = piece_on(ToSquare(file, rank));
            if (pc == Piece.NO_PIECE || ShogiTypes.color_of(pc) != us)
            {
                continue;
            }

            if (ShogiTypes.type_of(pc) == PieceType.PAWN)
            {
                return true;
            }
        }

        return false;
    }

    private static bool IsPromotionStateValid(Color us, PieceType movingType, Square from, Square to, bool promote, bool generateAllLegalMoves)
    {
        PieceType raw = ShogiTypes.raw_type_of((Piece)movingType);
        bool promotable = raw is PieceType.PAWN or PieceType.LANCE or PieceType.KNIGHT or PieceType.SILVER or PieceType.BISHOP or PieceType.ROOK;
        bool alreadyPromoted = ((int)movingType & (int)PieceType.PIECE_TYPE_PROMOTE) != 0;

        if (promote)
        {
            if (!promotable || alreadyPromoted)
            {
                return false;
            }

            return true;
        }

        if (IsForcedPromotion(us, raw, to))
        {
            return false;
        }

        if (!generateAllLegalMoves)
        {
            if (raw == PieceType.PAWN && IsPromotionZone(us, to))
            {
                return false;
            }

            if (raw == PieceType.LANCE)
            {
                int rank = RankOf(to);
                if (us == Color.BLACK ? rank <= 1 : rank >= 7)
                {
                    return false;
                }
            }

            if ((raw == PieceType.BISHOP || raw == PieceType.ROOK)
                && (IsPromotionZone(us, from) || IsPromotionZone(us, to)))
            {
                return false;
            }
        }

        return true;
    }

    private static bool IsPromotionZone(Color us, Square sq)
    {
        int rank = RankOf(sq);
        return us == Color.BLACK ? rank <= 2 : rank >= 6;
    }

    private static bool IsForcedPromotion(Color us, PieceType rawType, Square to)
    {
        int rank = RankOf(to);
        if (us == Color.BLACK)
        {
            return (rawType == PieceType.PAWN || rawType == PieceType.LANCE) && rank == 0;
        }

        return (rawType == PieceType.PAWN || rawType == PieceType.LANCE) && rank == 8;
    }

    /// <summary>
    /// 成れない駒（玉・金・成駒）かを判定する。
    /// </summary>
    private static bool IsNonPromotablePiece(Piece piece)
    {
        PieceType type = ShogiTypes.type_of(piece);
        return type == PieceType.GOLD || type == PieceType.KING || type >= PieceType.PRO_PAWN;
    }

    private bool IsCaptureOrInterpose(Color us, Square checkerSq, Square to)
    {
        Square ksq = kingSquare[(int)us];
        if (to == checkerSq)
        {
            return true;
        }

        return IsBetweenOnLine(checkerSq, ksq, to);
    }

    private bool IsLegalKingMove(Color us, Square from, Square to)
    {
        Piece captured = piece_on(to);
        Piece king = piece_on(from);

        remove_piece(from);
        if (captured != Piece.NO_PIECE)
        {
            remove_piece(to);
        }

        put_piece(king, to);
        bool attacked = attackers_to(Opposite(us), to).PopCount() > 0;

        remove_piece(to);
        if (captured != Piece.NO_PIECE)
        {
            put_piece(captured, to);
        }

        put_piece(king, from);
        return !attacked;
    }

    private static bool IsAligned(Square a, Square b, Square c)
    {
        int af = FileOf(a);
        int ar = RankOf(a);
        int bf = FileOf(b);
        int br = RankOf(b);
        int cf = FileOf(c);
        int cr = RankOf(c);

        int abf = bf - af;
        int abr = br - ar;
        int acf = cf - af;
        int acr = cr - ar;

        return abf * acr == abr * acf;
    }

    private static bool IsPawnAttack(Color us, Square from, Square target)
    {
        int df = FileOf(target) - FileOf(from);
        int dr = RankOf(target) - RankOf(from);
        int forward = us == Color.BLACK ? -1 : 1;
        return df == 0 && dr == forward;
    }

    /// <summary>
    /// 打ち歩詰め判定用に、相手玉に逃げ場所があるかを調べる。
    /// </summary>
    private bool HasKingEscapeAfterPawnDrop(Color us, Color them, Square enemyKing)
    {
        for (int df = -1; df <= 1; df++)
        {
            for (int dr = -1; dr <= 1; dr++)
            {
                if (df == 0 && dr == 0)
                {
                    continue;
                }

                int file = FileOf(enemyKing) + df;
                int rank = RankOf(enemyKing) + dr;
                if (!IsInside(file, rank))
                {
                    continue;
                }

                Square to = ToSquare(file, rank);
                Piece dst = piece_on(to);
                if (dst != Piece.NO_PIECE && ShogiTypes.color_of(dst) == them)
                {
                    continue;
                }

                remove_piece(enemyKing);
                if (dst != Piece.NO_PIECE)
                {
                    remove_piece(to);
                }

                put_piece(ShogiTypes.make_piece(them, PieceType.KING), to);
                bool safe = attackers_to(us, to).PopCount() == 0;

                remove_piece(to);
                if (dst != Piece.NO_PIECE)
                {
                    put_piece(dst, to);
                }

                put_piece(ShogiTypes.make_piece(them, PieceType.KING), enemyKing);

                if (safe)
                {
                    return true;
                }
            }
        }

        return false;
    }

    /// <summary>
    /// 打った歩を相手駒で捕獲できるか（捕獲後に玉が安全か）を調べる。
    /// </summary>
    private bool CanCaptureDroppedPawn(Color us, Color them, Square enemyKing, Square droppedSquare)
    {
        update_slider_blockers(them);
        Bitboard pinned = blockers_for_king(them);
        int droppedFile = FileOf(droppedSquare);

        for (int fromValue = 0; fromValue < (int)Square.SQ_NB; fromValue++)
        {
            Square from = (Square)fromValue;
            Piece pc = piece_on(from);
            if (pc == Piece.NO_PIECE || ShogiTypes.color_of(pc) != them)
            {
                continue;
            }

            if (ShogiTypes.type_of(pc) == PieceType.KING)
            {
                continue;
            }

            if (!IsAttackingSquare(from, pc, droppedSquare))
            {
                continue;
            }

            if (pinned.Test(from) && FileOf(from) != droppedFile)
            {
                continue;
            }

            remove_piece(from);
            Piece dropped = piece_on(droppedSquare);
            remove_piece(droppedSquare);
            put_piece(pc, droppedSquare);

            bool kingSafe = attackers_to(us, enemyKing).PopCount() == 0;

            remove_piece(droppedSquare);
            put_piece(dropped, droppedSquare);
            put_piece(pc, from);

            if (kingSafe)
            {
                return true;
            }
        }

        return false;
    }

    private static bool IsBetweenOnLine(Square a, Square b, Square x)
    {
        int af = FileOf(a);
        int ar = RankOf(a);
        int bf = FileOf(b);
        int br = RankOf(b);

        int df = bf - af;
        int dr = br - ar;
        int sf;
        int sr;

        if (df == 0)
        {
            sf = 0;
            sr = Math.Sign(dr);
        }
        else if (dr == 0)
        {
            sf = Math.Sign(df);
            sr = 0;
        }
        else if (Math.Abs(df) == Math.Abs(dr))
        {
            sf = Math.Sign(df);
            sr = Math.Sign(dr);
        }
        else
        {
            return false;
        }

        int file = af + sf;
        int rank = ar + sr;
        while (file != bf || rank != br)
        {
            if (ToSquare(file, rank) == x)
            {
                return true;
            }

            file += sf;
            rank += sr;
        }

        return false;
    }

    private static bool IsLegalDropSquare(Color us, PieceType pt, Square to)
    {
        int rank = RankOf(to);
        if (us == Color.BLACK)
        {
            if ((pt == PieceType.PAWN || pt == PieceType.LANCE) && rank == 0)
            {
                return false;
            }

            if (pt == PieceType.KNIGHT && rank <= 1)
            {
                return false;
            }
        }
        else
        {
            if ((pt == PieceType.PAWN || pt == PieceType.LANCE) && rank == 8)
            {
                return false;
            }

            if (pt == PieceType.KNIGHT && rank >= 7)
            {
                return false;
            }
        }

        return true;
    }
    private static Square ToSquare(int file, int rank) => (Square)(file * 9 + rank);

    private PositionSnapshot CreateSnapshot()
    {
        return new PositionSnapshot(
            (Piece[])board.Clone(),
            (Bitboard[])byTypeBB.Clone(),
            (Bitboard[])byColorBB.Clone(),
            (uint[])hand.Clone(),
            (Square[])kingSquare.Clone(),
            st,
            gamePly,
            sideToMove);
    }

    private void RestoreSnapshot(PositionSnapshot snapshot)
    {
        Array.Copy(snapshot.Board, board, board.Length);
        Array.Copy(snapshot.ByTypeBB, byTypeBB, byTypeBB.Length);
        Array.Copy(snapshot.ByColorBB, byColorBB, byColorBB.Length);
        Array.Copy(snapshot.Hand, hand, hand.Length);
        Array.Copy(snapshot.KingSquare, kingSquare, kingSquare.Length);
        st = snapshot.State;
        gamePly = snapshot.GamePly;
        sideToMove = snapshot.SideToMove;
    }

    private sealed class PositionSnapshot
    {
        public PositionSnapshot(
            Piece[] board,
            Bitboard[] byTypeBB,
            Bitboard[] byColorBB,
            uint[] hand,
            Square[] kingSquare,
            StateInfo state,
            int gamePly,
            Color sideToMove)
        {
            Board = board;
            ByTypeBB = byTypeBB;
            ByColorBB = byColorBB;
            Hand = hand;
            KingSquare = kingSquare;
            State = state;
            GamePly = gamePly;
            SideToMove = sideToMove;
        }

        public Piece[] Board { get; }
        public Bitboard[] ByTypeBB { get; }
        public Bitboard[] ByColorBB { get; }
        public uint[] Hand { get; }
        public Square[] KingSquare { get; }
        public StateInfo State { get; }
        public int GamePly { get; }
        public Color SideToMove { get; }
    }
}









