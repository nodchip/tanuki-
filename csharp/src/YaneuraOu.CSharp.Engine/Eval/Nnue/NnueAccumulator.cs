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
    private readonly Stack<AccumulatorState> statePool = new();

    /// <summary>
    /// フル再構築が実行された回数を返す。
    /// </summary>
    public int RebuildCount { get; private set; }

    /// <summary>
    /// 差分適用が実行された回数を返す。
    /// </summary>
    public int DeltaApplyCount { get; private set; }

    /// <summary>
    /// 評価関数推論が実行された回数を返す。
    /// </summary>
    public int EvaluateCount { get; private set; }

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
        AccumulatorState top = states.Peek();
        if (top.ScoreDirty)
        {
            EvaluateState(position, top);
        }

        return top.Score;
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
        AccumulatorState next = RentState();
        CopyState(previous, next);

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
            DeltaApplyCount++;
            MarkStateDirty(positionAfterMove, next);
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
            AccumulatorState released = states.Pop();
            ReleaseState(released);
        }
    }

    /// <summary>
    /// 状態を初期化する。
    /// </summary>
    public void Reset()
    {
        while (states.Count > 0)
        {
            ReleaseState(states.Pop());
        }

        states.Clear();
        RebuildCount = 0;
        DeltaApplyCount = 0;
        EvaluateCount = 0;
    }

    /// <summary>
    /// 指定局面で状態を初期化する。
    /// </summary>
    public void Reset(Position position)
    {
        states.Clear();
        var state = RentState();
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
        RebuildCount++;
        transformer.BuildAccumulation(position, model, Color.BLACK, state.BlackAccumulation);
        transformer.BuildAccumulation(position, model, Color.WHITE, state.WhiteAccumulation);
        MarkStateDirty(position, state);
    }

    /// <summary>
    /// 状態のキーと手番を更新し、評価を未確定化する。
    /// </summary>
    private void MarkStateDirty(Position position, AccumulatorState state)
    {
        state.PositionKey = position.state().key().ToUInt64();
        state.SideToMove = position.side_to_move();
        state.ScoreDirty = true;
    }

    /// <summary>
    /// 状態の評価値を更新する。
    /// </summary>
    private void EvaluateState(Position position, AccumulatorState state)
    {
        transformer.ConvertAccumulatorsToFeatures(position.side_to_move(), state.BlackAccumulation, state.WhiteAccumulation, state.Features);
        state.Score = model.Evaluate(state.Features);
        EvaluateCount++;
        state.ScoreDirty = false;
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
        public Color SideToMove { get; set; }
        public int Score { get; set; }
        public bool ScoreDirty { get; set; }

    }

    /// <summary>
    /// 使い回し用の状態オブジェクトを取得する。
    /// </summary>
    private AccumulatorState RentState()
    {
        if (statePool.Count > 0)
        {
            return statePool.Pop();
        }

        return new AccumulatorState();
    }

    /// <summary>
    /// 状態オブジェクトを再利用プールへ返却する。
    /// </summary>
    private void ReleaseState(AccumulatorState state)
    {
        state.PositionKey = 0;
        state.SideToMove = Color.BLACK;
        state.Score = 0;
        state.ScoreDirty = true;
        statePool.Push(state);
    }

    /// <summary>
    /// 状態の内容をコピーする。
    /// </summary>
    private static void CopyState(AccumulatorState source, AccumulatorState destination)
    {
        destination.PositionKey = source.PositionKey;
        destination.SideToMove = source.SideToMove;
        destination.Score = source.Score;
        destination.ScoreDirty = source.ScoreDirty;
        Array.Copy(source.BlackAccumulation, destination.BlackAccumulation, source.BlackAccumulation.Length);
        Array.Copy(source.WhiteAccumulation, destination.WhiteAccumulation, source.WhiteAccumulation.Length);
        Array.Copy(source.Features, destination.Features, source.Features.Length);
    }
}
