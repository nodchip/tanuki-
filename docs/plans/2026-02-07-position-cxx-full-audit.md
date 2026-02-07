# Position C++ Full Audit Design

Date: 2026-02-07
Target: `source/position.cpp`, `source/position.h`, `csharp/src/YaneuraOu.CSharp.Engine/Core/Position.cs`

## Goal
C++ 版 `Position` と C# 版 `Position` の挙動差分を完全監査し、探索破綻につながる差分を優先順位付きで修正可能なバックログへ落とし込む。

## Scope
- C++ 側 `Position` 関連メソッド全件（`position.cpp/.h`）
- C# 側対応実装とテスト全件
- `YANEURAOU_ENGINE_NNUE` 前提の通常ビルド相当

## Out of Scope
- Stockfish 専用分岐
- 深層評価実装の個別最適化
- WASM/特殊ビルド差分

## Recommended Approach
監査方式は **仕様トレーサビリティ監査** を採用する。

### 代替案比較
1. 仕様トレーサビリティ監査（採用）
- 長所: 抜け漏れ最小、根拠付きで重要度分類しやすい
- 短所: 初期整理に時間がかかる

2. 実行経路中心監査
- 長所: 探索バグ直結箇所に早く到達
- 短所: 周辺 API の欠落を見落としやすい

3. テスト差分逆引き監査
- 長所: 修正優先度を付けやすい
- 短所: C++ の暗黙仕様を拾いきれない

## Audit Framework
監査対象を以下のカテゴリへ分割し、C++ と C# を 1 対 1 で対応づける。
- 初期化・局面生成
- 利き・王手判定
- 合法性判定
- 局面更新（do/undo/null）
- 反復・千日手・宣言勝ち
- 補助 API

各項目に以下を記録する。
- C++ 関数
- C# 対応関数
- 対応状態（完全/部分/未対応）
- 差分要約
- 影響領域
- 最小再現条件
- 追加テスト案
- 推奨修正

## Severity Policy
差分は `Critical / Major / Minor` に分類する。

- Critical
  - 合法手生成の誤り
  - 王手回避判定ミス
  - do/undo 後の非同値復元
  - 反復判定の誤り
- Major
  - 再現性・性能に影響するが直ちに破綻しない差分
  - 条件分岐順や副作用順の差で将来不具合化しやすい差分
- Minor
  - 可読性・保守性・将来互換性の改善項目

## Execution Plan
1. C++ `Position` API 台帳を宣言順で作成する。
2. C# 対応実装を同粒度でマッピングする。
3. 分岐順・事前条件・副作用順・例外系の差分を抽出する。
4. 差分ごとに再現シナリオと最小テストを付与する。
5. `Critical -> Major -> Minor` の順に修正バックログを確定する。

## Error Handling Policy
判定不能項目は放置せず `Major(要確認)` として記録する。
以下を `Blocker` として明示する。
- C++ ソース参照不足
- C# 側対応関数不在
- 依存未実装でテスト不能

## Verification Strategy
2 層検証を行う。

1. 静的比較
- 関数、分岐、状態更新順の差分確認

2. 動的比較
- 最小局面による再現テスト
- `Critical` 候補は必ず動的比較まで実施

## Deliverables
1. 監査設計書（本ドキュメント）
2. 差分トラッキング表（`ID / 区分 / 関数対応 / 差分 / 影響 / 再現 / 修正 / テスト`）
3. 優先度付き修正バックログ

## Done Criteria
- `Position` 関連 API 全件が台帳化されている。
- 全差分に重要度と根拠が付与されている。
- 全 `Critical` に修正案とテスト案がある。
- `Major` まで着手順が確定している。

## Next Step
本設計に基づき、`source/position.cpp/.h` と C# 実装の完全監査を実行し、差分トラッキング表を作成する。
