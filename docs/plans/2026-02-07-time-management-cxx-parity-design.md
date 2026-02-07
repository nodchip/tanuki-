# Time Management C++ Parity Design

## 目的
- YaneuraOu C# の `go` 時間制御を、YaneuraOu C++ (`YANEURAOU_ENGINE_NNUE`) の責務分割に合わせて再設計する。
- 現行の「時間から深さを推定する」方式を廃止し、実時間ベース停止へ移行する。
- `movetime/byoyomi/btime/wtime/inc/ponder/ponderhit/nodes/mate/infinite` を統一的に扱う。

## ゴール
- `USI parse -> LimitsType -> TimeManagement -> Search stop` の流れを実装する。
- 反復深化は各 depth 完了時点で `optimum` を見て継続判定し、探索中は `maximum` を超えない。
- 停止契機（時間・ノード・外部stop・ponder遷移）が C++ の意図と整合する。

## 非ゴール
- 指し手完全一致の保証。
- 初回で詰将棋専用探索の完全実装（`go mate` は安全に入口整備し段階実装）。

## 現状課題
- `UsiEngine` の `SelectTimeLimitFromTimeControl()` が単純化されており、C++の時間配分思想と乖離。
- `Searcher` は `MaxTimeMs` の単一上限のみを参照しており、`optimum/maximum` の二段停止がない。
- `ponderhit` は受理ログのみで、時間再計算に接続されていない。
- `go nodes`/`go mate` の運用境界が未整備。

## 設計方針
1. C++責務準拠
- `UsiEngine` は入力解釈と `LimitsType` 構築まで。
- `TimeManagement` は時間配分の計算と停止判定用パラメータ生成。
- `Searcher` は停止ポリシーを参照して探索制御のみ担当。

2. 停止条件を明示的に合成
- 停止条件は次の OR:
  - 外部 stop/quit キャンセル
  - `maximumTime` 超過
  - `nodes` 上限到達
  - `mate` 条件成立（段階実装）

3. 安全側フォールバック
- 未対応詳細はクラッシュさせず `info string` で通知し、通常探索へ安全に退避。

## コンポーネント設計

### 1. LimitsType (新設)
- 候補: `Search/Time/LimitsType.cs`
- 主なフィールド:
  - `int[] TimeMs` (BLACK/WHITE)
  - `int[] IncMs` (BLACK/WHITE)
  - `int ByoyomiMs`
  - `int MoveTimeMs`
  - `int Depth`
  - `long Nodes`
  - `int Mate`
  - `bool Infinite`
  - `bool Ponder`
  - `DateTimeOffset StartTime`

### 2. TimeManagement (新設)
- 候補: `Search/Time/TimeManagement.cs`
- 入力:
  - `LimitsType limits`
  - `Color sideToMove`
  - `int ply`
  - `UsiOptions` または時間管理専用設定
- 出力:
  - `MinimumTimeMs`
  - `OptimumTimeMs`
  - `MaximumTimeMs`
  - `PonderHitTime`（必要時）
- 仕様:
  - `movetime` 指定時は `min=opt=max=movetime`
  - `infinite` は時間停止無効
  - `byoyomi/inc` と残り時間を組み合わせて配分
  - `ponderhit` 後は境界を再評価可能にする

### 3. StopPolicy (新設)
- 候補: `Search/Time/SearchStopPolicy.cs`
- 役割:
  - 現在時刻・ノード数・外部キャンセルを受け取り停止判定を返す
  - depth境界判定 (`ShouldStopAfterCompletedDepth`) と探索中判定 (`ShouldStopNow`) を分離

### 4. UsiEngine 改修
- `ParseGoLimits()` を `LimitsType` 生成に置換
- `go` 受理時:
  - `limits.StartTime` 記録
  - `TimeManagement.Init()` 実行
  - `Searcher` へ `StopPolicy` を渡す
- `ponder`:
  - すぐ `bestmove` を返さない
  - `ponderhit` で通常探索へ遷移

### 5. Searcher 改修
- `SearchLimits.MaxTimeMs` 直接参照を縮退
- 停止判定を `StopPolicy` 経由へ統一
- 反復深化の各 depth 完了時に `optimum` 判定
- ノード増加時に `nodes` 上限判定

## go パラメータ処理方針
- 解釈不能トークンは無視、解釈可能値のみ反映。
- 優先順位:
1. `infinite`（時間停止無効）
2. `movetime`（固定時間）
3. 残り時間 + `inc/byoyomi` 配分
4. 既定値
- `depth` 未指定時も、時間は実時間停止で管理する（深さ推定に依存しない）。

## エラー処理・ログ
- `DebugLog=true` のときのみ詳細ログ:
  - 入力 limits
  - 計算結果 `min/opt/max`
  - 停止理由（time/nodes/stop/ponder）
- `go mate` の未実装詳細は `info string` で明示し、安全にフォールバック。

## テスト設計

### 単体テスト
- `TimeManagementTests`
  - `movetime` 固定値
  - `byoyomi` 単独
  - `btime/wtime + inc`
  - 低残時間での不変条件 `min <= opt <= max`
- `UsiEngineGoLimitsTests`
  - `go infinite`
  - `go ponder` + `ponderhit`
  - `go nodes`
  - `go mate`
- `SearchStopPolicyTests`
  - 時間超過
  - ノード上限
  - 外部stop優先
  - depth完了境界判定

### 統合テスト
- `UsiEngineTests`
  - `usi/isready/position/go/stop` 往復
  - `bestmove` 応答保証
  - `go infinite` から `stop` で返却

## 実装順
1. `LimitsType` / `TimeManagement` / `StopPolicy` 追加
2. `UsiEngine.ParseGoLimits` 差し替え
3. `Searcher` 停止判定差し替え
4. `ponder/ponderhit` 遷移強化
5. `DebugLog` 整備
6. build/test/対局スモークで検証

## 完了条件
- `go movetime/byoyomi/btime/wtime/inc/infinite/ponder/nodes/mate` を受理できる。
- 深さ推定に依存せず、実時間・実ノードで停止する。
- ShogiHome/将棋所で `stop` 応答不能や時間切れ事故が再現しない。
