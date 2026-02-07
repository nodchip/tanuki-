using System.Collections.Generic;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 探索用の指し手並び替えを行うクラス。
/// </summary>
public static class MoveOrdering
{
    /// <summary>
    /// 簡易ヒューリスティクスで指し手を並び替える。
    /// </summary>
    public static List<Move> Order(Position position, MoveList moves)
    {
        var scored = new List<(Move Move, int Score)>();
        Color us = position.side_to_move();
        foreach (Move move in moves)
        {
            int score = 0;
            if (!move.is_drop())
            {
                Piece target = position.piece_on(move.to_sq());
                if (target != Piece.NO_PIECE && ShogiTypes.color_of(target) != us)
                {
                    score += 10_000;
                }
            }

            if (move.is_promote())
            {
                score += 1_000;
            }

            scored.Add((move, score));
        }

        scored.Sort((a, b) => b.Score.CompareTo(a.Score));
        var ordered = new List<Move>(scored.Count);
        foreach ((Move move, _) in scored)
        {
            ordered.Add(move);
        }

        return ordered;
    }
}
