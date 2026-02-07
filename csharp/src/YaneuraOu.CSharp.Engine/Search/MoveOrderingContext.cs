using System.Collections.Generic;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 指し手順序付けで使う履歴・killer情報を保持するクラス。
/// </summary>
public sealed class MoveOrderingContext
{
    private readonly Dictionary<uint, int> history = new();
    private readonly Move[] killer1 = new Move[128];
    private readonly Move[] killer2 = new Move[128];

    /// <summary>
    /// history値を加算する。
    /// </summary>
    public void AddHistory(Move move, int depth)
    {
        uint key = move.to_u32();
        int bonus = depth * depth;
        history.TryGetValue(key, out int current);
        history[key] = current + bonus;
    }

    /// <summary>
    /// killer手を登録する。
    /// </summary>
    public void RegisterKiller(int ply, Move move)
    {
        if (ply < 0 || ply >= killer1.Length || move.to_u32() == Move.none().to_u32())
        {
            return;
        }

        if (killer1[ply].to_u32() == move.to_u32())
        {
            return;
        }

        killer2[ply] = killer1[ply];
        killer1[ply] = move;
    }

    /// <summary>
    /// 指し手に対する追加ボーナスを返す。
    /// </summary>
    public int GetBonus(Move move, int ply)
    {
        int score = 0;
        uint key = move.to_u32();

        if (history.TryGetValue(key, out int h))
        {
            score += h;
        }

        if (ply >= 0 && ply < killer1.Length)
        {
            if (killer1[ply].to_u32() == key)
            {
                score += 50_000;
            }
            else if (killer2[ply].to_u32() == key)
            {
                score += 25_000;
            }
        }

        return score;
    }
}
