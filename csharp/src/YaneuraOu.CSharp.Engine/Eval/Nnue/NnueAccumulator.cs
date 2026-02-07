using YaneuraOu.CSharp.Engine.Core;
using YaneuraOu.CSharp.Engine.Core.Types;

namespace YaneuraOu.CSharp.Engine.Eval;

/// <summary>
/// NNUE特徴の差分更新状態を管理するアキュムレータクラス。
/// </summary>
public sealed class NnueAccumulator
{
    private readonly NnueFeatureTransformer transformer;
    private readonly NnueModel model;
    private readonly Stack<AccumulatorState> states = new();

    /// <summary>
    /// NnueAccumulatorのインスタンスを初期化する。
    /// </summary>
    public NnueAccumulator(NnueFeatureTransformer transformer, NnueModel model)
    {
        this.transformer = transformer;
        this.model = model;
    }

    /// <summary>
    /// 差分更新状態で評価値を返す。
    /// </summary>
    public int EvaluateIncremental(Position position)
    {
        EnsureTopState(position);
        return states.Peek().Score;
    }

    /// <summary>
    /// 常にフル再計算した評価値を返す。
    /// </summary>
    public int EvaluateByRecompute(Position position)
    {
        byte[] transformed = transformer.Transform(position, model);
        return model.Evaluate(transformed);
    }

    /// <summary>
    /// 手適用後の局面に対して差分状態をpushする。
    /// </summary>
    public void PushMove(Position positionAfterMove, Move move, Piece capturedPiece, Color movingSide)
    {
        EnsureTopState(positionAfterMove, allowKeyMismatch: true);
        AccumulatorState previous = states.Peek();
        AccumulatorState next = previous.Clone();

        transformer.PrepareForDelta(positionAfterMove);
        bool applied = transformer.TryApplyMoveDelta(
            positionAfterMove,
            model,
            move,
            capturedPiece,
            movingSide,
            next.BlackAccumulation,
            next.WhiteAccumulation);
        if (!applied)
        {
            RebuildState(positionAfterMove, next);
        }
        else
        {
            UpdateStateScore(positionAfterMove, next);
        }

        states.Push(next);
    }

    /// <summary>
    /// 直前のpush状態をpopする。
    /// </summary>
    public void Pop()
    {
        if (states.Count > 1)
        {
            states.Pop();
        }
    }

    /// <summary>
    /// 状態を初期化する。
    /// </summary>
    public void Reset()
    {
        states.Clear();
    }

    /// <summary>
    /// 指定局面で状態を初期化する。
    /// </summary>
    public void Reset(Position position)
    {
        states.Clear();
        var state = new AccumulatorState();
        RebuildState(position, state);
        states.Push(state);
    }

    /// <summary>
    /// 先頭状態を必要に応じて再構築する。
    /// </summary>
    private void EnsureTopState(Position position, bool allowKeyMismatch = false)
    {
        if (states.Count == 0)
        {
            Reset(position);
            return;
        }

        if (allowKeyMismatch)
        {
            return;
        }

        AccumulatorState top = states.Peek();
        ulong key = position.state().key().ToUInt64();
        if (top.PositionKey != key)
        {
            Reset(position);
        }
    }

    /// <summary>
    /// 状態をフル再構築する。
    /// </summary>
    private void RebuildState(Position position, AccumulatorState state)
    {
        transformer.BuildAccumulation(position, model, Color.BLACK, state.BlackAccumulation);
        transformer.BuildAccumulation(position, model, Color.WHITE, state.WhiteAccumulation);
        UpdateStateScore(position, state);
    }

    /// <summary>
    /// 状態の評価値とキーを更新する。
    /// </summary>
    private void UpdateStateScore(Position position, AccumulatorState state)
    {
        state.PositionKey = position.state().key().ToUInt64();
        transformer.ConvertAccumulatorsToFeatures(position.side_to_move(), state.BlackAccumulation, state.WhiteAccumulation, state.Features);
        state.Score = model.Evaluate(state.Features);
    }

    /// <summary>
    /// 累積状態を保持する内部クラス。
    /// </summary>
    private sealed class AccumulatorState
    {
        public int[] BlackAccumulation { get; } = new int[NnueModel.HalfDimensions];
        public int[] WhiteAccumulation { get; } = new int[NnueModel.HalfDimensions];
        public byte[] Features { get; } = new byte[NnueModel.HalfDimensions * 2];
        public ulong PositionKey { get; set; }
        public int Score { get; set; }

        public AccumulatorState Clone()
        {
            var clone = new AccumulatorState
            {
                PositionKey = PositionKey,
                Score = Score,
            };
            Array.Copy(BlackAccumulation, clone.BlackAccumulation, BlackAccumulation.Length);
            Array.Copy(WhiteAccumulation, clone.WhiteAccumulation, WhiteAccumulation.Length);
            Array.Copy(Features, clone.Features, Features.Length);
            return clone;
        }
    }
}
