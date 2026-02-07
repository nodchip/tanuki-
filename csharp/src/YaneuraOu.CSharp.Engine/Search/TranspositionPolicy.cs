using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Search;

/// <summary>
/// 置換表の利用可否を判定するポリシークラス。
/// </summary>
public static class TranspositionPolicy
{
    /// <summary>
    /// 指定エントリを探索窓に対して利用可能かどうかを判定する。
    /// </summary>
    public static bool TryResolve(
        int storedDepth,
        int requiredDepth,
        int storedScore,
        TranspositionBound bound,
        int alpha,
        int beta,
        Move storedMove,
        out int score,
        out Move bestMove)
    {
        bestMove = storedMove;
        score = 0;
        if (storedDepth < requiredDepth)
        {
            return false;
        }

        if (bound == TranspositionBound.Exact)
        {
            score = storedScore;
            return true;
        }

        if (bound == TranspositionBound.Lower && storedScore >= beta)
        {
            score = storedScore;
            return true;
        }

        if (bound == TranspositionBound.Upper && storedScore <= alpha)
        {
            score = storedScore;
            return true;
        }

        return false;
    }
}
