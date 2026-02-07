using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tools;

/// <summary>
/// 開始局面から合法手ランダムプレイアウトでSFENを生成するクラス。
/// </summary>
public sealed class SfenSampler
{
    /// <summary>
    /// 指定条件でSFENを複数生成する。
    /// </summary>
    public IReadOnlyList<string> Generate(int count, int seed, int minPlies, int maxPlies)
    {
        int safeCount = Math.Max(1, count);
        int safeMinPlies = Math.Max(1, minPlies);
        int safeMaxPlies = Math.Max(safeMinPlies, maxPlies);

        var random = new Random(seed);
        var results = new List<string>(safeCount);
        var unique = new HashSet<string>(StringComparer.Ordinal);
        int attempts = 0;
        int maxAttempts = safeCount * 20;

        while (results.Count < safeCount && attempts < maxAttempts)
        {
            attempts++;
            string sfen = GenerateOne(random, safeMinPlies, safeMaxPlies);
            if (!unique.Add(sfen))
            {
                continue;
            }

            results.Add(sfen);
        }

        return results;
    }

    /// <summary>
    /// 1局面分のSFENを生成する。
    /// </summary>
    private static string GenerateOne(Random random, int minPlies, int maxPlies)
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        int targetPlies = random.Next(minPlies, maxPlies + 1);

        for (int ply = 0; ply < targetPlies; ply++)
        {
            MoveList legal = MoveGenerator.GenerateLegal(position);
            if (legal.Count == 0)
            {
                break;
            }

            Move selected = legal[random.Next(legal.Count)];
            var st = new StateInfo();
            position.do_move(selected, st, position.gives_check(selected));
        }

        return position.sfen();
    }
}
