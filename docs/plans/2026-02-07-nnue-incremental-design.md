# NNUE Incremental Update Design (C++ Parity)

## 目的
- NNUE評価の再計算コストを削減し、`byoyomi 3000` でも探索深さが伸びる状態にする。
- YaneuraOu C++ の考え方に合わせ、`do_move/undo_move` に連動した差分更新を実装する。
- 実運用で悪手を誘発しやすい「浅い探索 + 評価揺れ」を低減する。

## 方針
- `NnueAccumulator` を「再計算器」から「状態スタック付き差分更新器」へ拡張する。
- 通常手/成り/打ち/捕獲/undo を一括対応する。
- king移動で特徴量基準が変わる場合は、安全側としてそのplyでフル再構築する。
- 完了条件は「一致性」と「速度」を同時達成する。

## 対象範囲

### 含む
- `NnueAccumulator` の差分更新ロジック
- `NnueFeatureTransformer` の差分適用API追加
- `Searcher` での `do_move/undo_move` に同期した評価更新
- 一致性テストとベンチ比較

### 含まない
- SIMD最適化の全面導入（必要なら次フェーズ）
- NNUEネットワーク構造自体の変更

## アーキテクチャ

### 1. Accumulator状態スタック
- `NnueAccumulator` に ply単位の状態を保持する。
- 各状態は最低限以下を持つ:
  - `friendAccum[256]`
  - `enemyAccum[256]`
  - `score`
  - `sideToMove`
- `Push(move, before, after)` で次状態を生成、`Pop()` で巻き戻す。

### 2. 差分イベントモデル
- 1手に対して「remove feature」「add feature」を列挙し適用する。
- イベント分類:
  - 通常移動: `from`除去 + `to`追加
  - 捕獲: 被捕獲駒の盤上特徴を除去
  - 成り: 非成駒除去 + 成駒追加
  - 打ち: 持駒特徴を減算 + 盤上特徴を加算
  - undo: 逆差分（実装上はスタック復元優先）

### 3. king移動時の扱い
- HalfKPはking位置に依存するため、king移動時は差分ではなく再構築を許容する。
- これにより実装リスクを抑えつつ、非king移動で大幅高速化を狙う。

### 4. Search統合
- `Searcher` は `position.do_move` 前後で `NnueAccumulator` を同期する。
- 末端評価は accumulator 経由に統一し、ノードごとの全面再計算を避ける。
- `undo_move` 時は accumulator も必ず巻き戻す。

## データフロー
1. root局面で accumulator を初期化（1回フル構築）
2. 各 `do_move`:
   - 差分イベント生成
   - 非king移動なら差分適用
   - king移動なら再構築
3. 評価要求で現在stateのscoreを返す
4. `undo_move` で state pop

## テスト計画

### 一致性テスト（必須）
- `Incremental == Recompute` を局面列で毎ply比較
- ケース:
  - 通常手
  - 捕獲
  - 成り
  - 打ち
  - king移動
  - undoを挟む往復

### 回帰テスト（実運用寄り）
- 再現ログ局面を固定し、3秒思考時のdepth/score推移をスナップショット化
- 重大な劣化（深さ低下・異常揺れ）を検出

### 性能テスト（必須）
- benchでNNUE有効時のNPSを比較
- 目標: 現状比で有意改善（最低 1.5x を目安）

## 実装順
1. `NnueAccumulator` に state stack を導入
2. `NnueFeatureTransformer` に差分イベントAPIを追加
3. 差分適用（通常/成り/打ち/捕獲）実装
4. king移動時フル再構築フォールバック実装
5. `Searcher` と統合
6. 一致性テスト追加
7. bench計測と閾値確認

## 受け入れ条件
- `dotnet build` 成功
- `dotnet test` 成功（一致性テストを含む）
- NNUE有効benchでNPS改善を確認
- 実運用局面で `byoyomi 3000` の深さ到達が改善

## リスクと対策
- リスク: 差分イベントの符号ミスで評価破損
  - 対策: 手種別ごとの最小テスト + 毎ply一致比較
- リスク: undo非対称
  - 対策: push/popの可逆性テストを独立追加
- リスク: king移動時の不整合
  - 対策: 該当時は強制再構築で安全側に倒す
