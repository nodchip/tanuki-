using System.Linq;
using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.MoveGen;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Tools;

/// <summary>
/// 生成した局面に含まれる手タイプの注釈。
/// </summary>
[Flags]
public enum SfenSampleFeatures
{
    /// <summary>
    /// 注釈なし。
    /// </summary>
    None = 0,

    /// <summary>
    /// 玉移動を含む。
    /// </summary>
    KingMove = 1 << 0,

    /// <summary>
    /// 成り手を含む。
    /// </summary>
    Promotion = 1 << 1,

    /// <summary>
    /// 打ち手を含む。
    /// </summary>
    Drop = 1 << 2,
}

/// <summary>
/// 生成したSFENと注釈を保持する。
/// </summary>
public readonly record struct SfenSample(string Sfen, SfenSampleFeatures Features);

/// <summary>
/// 合法手ランダムプレイアウトでSFENを生成するクラス。
/// </summary>
public sealed class SfenSampler
{
    /// <summary>
    /// 指定条件でSFENを複数生成する。
    /// </summary>
    public IReadOnlyList<string> Generate(int count, int seed, int minPlies, int maxPlies)
    {
        return GenerateAnnotated(count, seed, minPlies, maxPlies).Select(sample => sample.Sfen).ToList();
    }

    /// <summary>
    /// 指定条件で注釈付きSFENを複数生成する。
    /// </summary>
    public IReadOnlyList<SfenSample> GenerateAnnotated(int count, int seed, int minPlies, int maxPlies)
    {
        int safeCount = Math.Max(1, count);
        int safeMinPlies = Math.Max(1, minPlies);
        int safeMaxPlies = Math.Max(safeMinPlies, maxPlies);

        var random = new Random(seed);
        var results = new List<SfenSample>(safeCount);
        var unique = new HashSet<string>(StringComparer.Ordinal);
        int attempts = 0;
        int maxAttempts = safeCount * 40;
        SfenSampleFeatures covered = SfenSampleFeatures.None;

        while (results.Count < safeCount && attempts < maxAttempts)
        {
            attempts++;
            SfenSampleFeatures target = SelectTargetFeature(results.Count, covered);
            SfenSample sample = GenerateOne(random, safeMinPlies, safeMaxPlies, target);
            if (!unique.Add(sample.Sfen))
            {
                continue;
            }

            covered |= sample.Features;
            results.Add(sample);
        }

        EnsureCoverage(SfenSampleFeatures.KingMove, results, unique, safeCount, ref covered);
        EnsureCoverage(SfenSampleFeatures.Promotion, results, unique, safeCount, ref covered);
        EnsureCoverage(SfenSampleFeatures.Drop, results, unique, safeCount, ref covered);

        return results;
    }

    /// <summary>
    /// 1局面分のSFENを生成する。
    /// </summary>
    private static SfenSample GenerateOne(Random random, int minPlies, int maxPlies, SfenSampleFeatures targetFeature)
    {
        var position = new Position();
        position.set(Position.StartSfen, new StateInfo());
        int targetPlies = random.Next(minPlies, maxPlies + 1);
        SfenSampleFeatures features = SfenSampleFeatures.None;

        for (int ply = 0; ply < targetPlies; ply++)
        {
            MoveList legal = MoveGenerator.GenerateLegal(position);
            if (legal.Count == 0)
            {
                break;
            }

            Move selected = SelectMove(random, position, legal, targetFeature, features);
            features |= ClassifyMove(position, selected);

            var st = new StateInfo();
            position.do_move(selected, st, position.gives_check(selected));
        }

        return new SfenSample(position.sfen(), features);
    }

    /// <summary>
    /// ターゲット手タイプを優先して1手選ぶ。
    /// </summary>
    private static Move SelectMove(
        Random random,
        Position position,
        MoveList legal,
        SfenSampleFeatures targetFeature,
        SfenSampleFeatures currentFeatures)
    {
        if ((currentFeatures & targetFeature) != 0)
        {
            return legal[random.Next(legal.Count)];
        }

        var preferredIndices = new List<int>();
        for (int i = 0; i < legal.Count; i++)
        {
            if ((ClassifyMove(position, legal[i]) & targetFeature) != 0)
            {
                preferredIndices.Add(i);
            }
        }

        if (preferredIndices.Count > 0)
        {
            int selected = preferredIndices[random.Next(preferredIndices.Count)];
            return legal[selected];
        }

        return legal[random.Next(legal.Count)];
    }

    /// <summary>
    /// 指し手の手タイプを判定する。
    /// </summary>
    private static SfenSampleFeatures ClassifyMove(Position position, Move move)
    {
        SfenSampleFeatures result = SfenSampleFeatures.None;
        if (move.is_drop())
        {
            result |= SfenSampleFeatures.Drop;
        }

        if (move.is_promote())
        {
            result |= SfenSampleFeatures.Promotion;
        }

        if (!move.is_drop())
        {
            Piece piece = position.piece_on(move.from_sq());
            if (ShogiTypes.raw_type_of(piece) == PieceType.KING)
            {
                result |= SfenSampleFeatures.KingMove;
            }
        }

        return result;
    }

    /// <summary>
    /// まだ未カバーの手タイプを優先してターゲットを選ぶ。
    /// </summary>
    private static SfenSampleFeatures SelectTargetFeature(int index, SfenSampleFeatures covered)
    {
        if ((covered & SfenSampleFeatures.KingMove) == 0)
        {
            return SfenSampleFeatures.KingMove;
        }

        if ((covered & SfenSampleFeatures.Promotion) == 0)
        {
            return SfenSampleFeatures.Promotion;
        }

        if ((covered & SfenSampleFeatures.Drop) == 0)
        {
            return SfenSampleFeatures.Drop;
        }

        return (index % 3) switch
        {
            0 => SfenSampleFeatures.KingMove,
            1 => SfenSampleFeatures.Promotion,
            _ => SfenSampleFeatures.Drop,
        };
    }

    /// <summary>
    /// 未カバー機能の強制サンプルを追加する。
    /// </summary>
    private static void EnsureCoverage(
        SfenSampleFeatures feature,
        List<SfenSample> results,
        HashSet<string> unique,
        int limit,
        ref SfenSampleFeatures covered)
    {
        if ((covered & feature) != 0)
        {
            return;
        }

        SfenSample forced = GenerateForcedSample(feature);
        if (results.Count >= limit && results.Count > 0)
        {
            SfenSample removed = results[^1];
            unique.Remove(removed.Sfen);
            results.RemoveAt(results.Count - 1);
        }

        if (!unique.Contains(forced.Sfen))
        {
            unique.Add(forced.Sfen);
        }

        covered |= forced.Features;
        results.Add(forced);
    }

    /// <summary>
    /// 指定機能を必ず含むサンプルを生成する。
    /// </summary>
    private static SfenSample GenerateForcedSample(SfenSampleFeatures feature)
    {
        var position = new Position();
        switch (feature)
        {
            case SfenSampleFeatures.KingMove:
                position.set("4k4/9/9/9/9/9/9/4K4/9 b - 1", new StateInfo());
                break;
            case SfenSampleFeatures.Promotion:
                position.set("4k4/6P2/9/9/9/9/9/9/4K4 b - 1", new StateInfo());
                break;
            default:
                position.set("4k4/9/9/9/9/9/9/9/4K4 b P 1", new StateInfo());
                break;
        }

        MoveList legal = MoveGenerator.GenerateLegal(position);
        Move move = Move.none();
        for (int i = 0; i < legal.Count; i++)
        {
            if ((ClassifyMove(position, legal[i]) & feature) != 0)
            {
                move = legal[i];
                break;
            }
        }

        if (move.to_u32() == Move.none().to_u32())
        {
            return new SfenSample(position.sfen(), feature);
        }

        SfenSampleFeatures features = ClassifyMove(position, move);
        var st = new StateInfo();
        position.do_move(move, st, position.gives_check(move));
        return new SfenSample(position.sfen(), features);
    }
}
