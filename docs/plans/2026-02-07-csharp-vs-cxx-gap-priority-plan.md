# C#移植差分 優先度付き実行計画（対C++ `YANEURAOU_ENGINE_NNUE`）

## 1. 目的
- C#版を、やねうら王C++版 `YANEURAOU_ENGINE_NNUE` に対して実戦品質で追従させる。
- 「評価一致」だけでなく「対局品質」「時間管理」「運用安定性」を揃える。

## 2. 優先度定義
- P0（致命）: 実戦投入を止める差分。最優先で解消。
- P1（重要）: 勝率・安定性に大きく影響。P0直後に解消。
- P2（改善）: 性能・運用性向上。P1完了後に解消。

## 3. 差分と実行順

### P0-1: 実戦での悪手/反則級挙動の根治
- 差分: 探索と局面更新連携のどこかに破綻が残っている可能性。
- 実施:
  1. 再現ログ固定（SFEN + go条件 + 返し手）を回帰テスト化。
  2. `do_move/undo_move` と評価更新（NNUE増分/再計算）の一致監査を探索経路で実施。
  3. 反則手を返す経路にガードを追加（`bestmove` 選定直前の合法性最終確認）。
- 完了条件:
  - 再現ケースで悪手/反則手が消える。
  - 追加回帰テストがGREEN。

### P0-2: `go mate` の未完全実装解消
- 差分: 現状は通常探索フォールバックが残る。
- 実施:
  1. `go mate` 用の制約と停止条件を分離。
  2. 最低限の詰み探索（mate solver）を実装。
  3. USI `score mate` と整合する応答に統一。
- 完了条件:
  - `go mate` 指定時にフォールバックログが出ない。
  - 詰み/逃れ局面テストで期待通り動作。

### P0-3: USI長時間運用安定化
- 差分: GUI実運用での長時間対局耐性が未保証。
- 実施:
  1. ShogiHome/将棋所向けの連続対局スモークを自動化。
  2. `go/stop/ponderhit/quit` の競合タイミングを回帰化。
  3. ログ粒度を整理（深さ完了ごと `info`、異常時 `info string`）。
- 完了条件:
  - 長時間スモークでクラッシュ/ハング/プロトコル異常0件。

### P1-1: 探索強化（C++との差が大きい層）
- 差分: 高度な探索制御が不足。
- 実施:
  1. 導入順を固定: Null Move → LMR → Aspiration Window。
  2. 各段階で Elo 低下がないことを自己対局で確認。
  3. 不安定化時は feature flag で即時無効化可能にする。
- 完了条件:
  - 同時間条件で現行比の勝率改善。
  - 既存回帰テストに退行なし。

### P1-2: 時間管理のC++寄せ
- 差分: 配分式と停止判定の成熟度差。
- 実施:
  1. `movetime/byoyomi/btime/wtime/inc` の配分式をC++に寄せる。
  2. 深さ完了時停止と即時停止の境界条件を一致化。
  3. 切れ負け/秒読みでのオーバーを監視テスト化。
- 完了条件:
  - 時間超過0件。
  - 既知ケースでC++と同傾向の思考時間。

### P1-3: NNUE増分更新の本運用化
- 差分: 局面評価一致は進捗済みだが、探索経路での増分運用をさらに堅牢化する余地。
- 実施:
  1. `Incremental == Recompute` アサートをデバッグ運用で維持。
  2. king移動・成り・打ち・取り・undoの全ケース監査拡張。
  3. NPS劣化が許容内か測定。
- 完了条件:
  - 増分/再計算不一致0。
  - 性能回帰が許容範囲内。

### P2-1: 並列探索（Threads>1）
- 差分: option受理はあるが実探索の並列化は未達。
- 実施:
  1. Split point戦略を設計。
  2. 共有TTと履歴の同期方針を確定。
  3. 2スレッドから段階導入。
- 完了条件:
  - `Threads=2` 以上で安定動作。
  - 1スレッド比で速度向上。

### P2-2: オプション/運用整備
- 差分: C++運用オプションの完全互換に未達。
- 実施:
  1. 主要USIオプションの差分表を作成。
  2. 優先度順に追加。
  3. README/runbook/tuningを同期更新。
- 完了条件:
  - GUI利用者目線で設定互換が向上。

## 4. 直近スプリント（次の実装単位）
1. P0-1 再現ケース固定と回帰化。
2. P0-2 `go mate` 実装。
3. P0-3 長時間USIスモーク自動化。

## 5. 標準検証
- Build: `dotnet build csharp/YaneuraOu.CSharp.sln -v minimal`
- Test: `dotnet test csharp/YaneuraOu.CSharp.sln -v minimal`
- 長時間USIスモーク: `csharp/tools/selfplay/SelfPlaySmoke.ps1` を秒読み条件で実行


## 6. 進捗更新 (2026-02-07)
### 完了
- P0-1: 違法手無視・bestmove合法化・USIスモーク整備を実装済み。
- P0-2: `go mate` 探索経路を実装済み。
- P0-3: USI運用安定化（`go/stop/ponderhit/quit` 回帰、停止理由ログ）を実装済み。
- P1-1: NullMove / LMR / Aspiration を導入済み。さらに `UseNullMovePruning` / `UseLmr` / `UseAspirationWindow` で feature flag 切替可能。
- P1-2: `MoveOverhead` / `MinimumThinkingTime` / `SlowMover` / `RoundUpToFullSecond` / `NetworkDelay` / `NetworkDelay2` を実装済み。`ponderhit` 基準の停止判定も対応済み。
- P1-3: `Incremental == Recompute` の検証を拡張（白側成り・白側打ち・玉取りfallback）。NNUE parity strict 実行フロー、ケース網羅チェック、失敗時診断ログを整備済み。

### 進行中
- P1-3: parityケース品質の継続改善（対局ログ由来ケースの追加、分布最適化）。

### 未完了
- P2-1: `Threads > 1` の並列探索は未実装。
- P2-2: C++同等のチューニング/運用オプション体系への収束は未完了。

### 次アクション
1. `Threads > 1` に向けて split point 設計と最小実装計画を作成する。
2. 並列化前に単一スレッドのベンチ基準値（NPS/Elo proxy）を固定する。
3. strict parity を定期実行ジョブ化し、回帰を自動検出する。
