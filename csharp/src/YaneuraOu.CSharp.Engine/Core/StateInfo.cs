using YaneuraOu.CSharp.Engine.Core.Board;
using YaneuraOu.CSharp.Engine.Core.Hash;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Core;

public sealed class StateInfo
{
    public Key board_key = new(0UL);
    public Key hand_key = new(0UL);

    public Bitboard checkersBB = new(0);
    public StateInfo? previous;
    public Bitboard[] blockersForKing = new Bitboard[(int)Color.COLOR_NB];
    public Bitboard[] pinners = new Bitboard[(int)Color.COLOR_NB];
    public Bitboard[] checkSquares = new Bitboard[(int)PieceType.PIECE_TYPE_NB];
    public Piece capturedPiece = Piece.NO_PIECE;
    public int repetition;
    public int repetition_times;
    public int repetition_type;
    public int pliesFromNull;
    public int[] continuousCheck = new int[(int)Color.COLOR_NB];
    public uint hand;

    public Key key()
    {
        return board_key ^ hand_key;
    }
}
