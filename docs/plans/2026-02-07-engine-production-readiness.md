# YaneuraOu C# Production Readiness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** C#移植版をUSI実戦投入可能な品質まで引き上げる（合法性・探索・時間管理・運用検証を満たす）。

**Architecture:** 既存の`Core/Position`互換実装を土台に、`Critical -> Major -> Minor`の順で差分を解消し、その後に探索・USI運用・性能・品質保証を段階導入する。各段階でTDDと検証ゲート（build/test/対局回帰）を必須化する。

**Tech Stack:** C#, .NET 8, MSTest, Visual Studio, USI, CLI対局ツール

---

## 0. 現在地（完了済み）
- `types/bitboard/movegen/position` の基礎移植は成立。
- `to_move/pseudo_legal/legal/legal_promote` の互換テストは整備済み。
- 監査成果: `docs/plans/2026-02-07-position-cxx-full-audit-tracking.md`
- 既知の優先差分: `D-CRIT-01`, `D-CRIT-02` ほか。

## 1. 残作業一覧（実戦投入まで）
1. Critical差分の解消（`to_move` 成り不正手, `StateInfo`/`do-undo-null`整合）。
2. 反復/引き分け/宣言勝ちの実装（`is_repetition/is_draw/DeclarationWin`）。
3. 探索器の実装（反復深化, alpha-beta, quiescence, move ordering）。
4. 置換表・履歴ヒューリスティクス・killer/historyの導入。
5. 時間管理（`go`各種）とUSI主要コマンド完全対応。
6. 詰み探索・終盤安全策（最低限mate検出/詰み回避）。
7. 評価関数方針確定（暫定評価→NNUE接続）と精度検証。
8. 並列化（最低1手読みの安定化後にスレッド導入）。
9. 対局回帰・性能回帰・再現性検証基盤。
10. 実戦運用（設定プリセット、ログ、クラッシュ時診断、配布）。

## 2. フェーズ計画（ゲート付き）

### Phase A: 監査Critical解消（必須）
**Files:**
- Modify: `csharp/src/YaneuraOu.CSharp.Engine/Core/Position.cs`
- Modify: `csharp/src/YaneuraOu.CSharp.Engine/Core/StateInfo.cs`
- Modify: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Core/Position/PositionToMoveTests.cs`
- Modify: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Core/Position/PositionMoveCycleTests.cs`

1. `D-CRIT-01` の失敗テストを追加。
2. `to_move` で非成駒成りを `Move.none()` 化。
3. `D-CRIT-02` の失敗テストを追加（`do/undo/null` 状態整合）。
4. `StateInfo` 拡張と更新順実装。
5. `dotnet build` / `dotnet test` 実行。
6. コミット。

**Gate A:**
- 既存テスト + 追加テストが全て通過。
- `Critical` が0件になる。

### Phase B: ルール完全性（反復・終局）
**Files:**
- Modify: `csharp/src/YaneuraOu.CSharp.Engine/Core/Position.cs`
- Create: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Core/Position/PositionRepetitionTests.cs`
- Create: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Core/Position/PositionDeclarationWinTests.cs`

1. `is_repetition/is_draw/has_repeated` テストを先に追加。
2. 最小実装でGREEN化。
3. `DeclarationWin/update_entering_point` テスト追加。
4. 最小実装でGREEN化。
5. 全体テスト実行。
6. コミット。

**Gate B:**
- 千日手/連続王手千日手の判定が再現ケースで一致。
- 宣言勝ち判定が基本ケースで一致。

### Phase C: 探索最小成立（1スレッド）
**Files:**
- Create: `csharp/src/YaneuraOu.CSharp.Engine/Search/Searcher.cs`
- Create: `csharp/src/YaneuraOu.CSharp.Engine/Search/SearchLimits.cs`
- Create: `csharp/src/YaneuraOu.CSharp.Engine/Search/MoveOrdering.cs`
- Create: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Search/SearcherBasicTests.cs`

1. `bestmove` が返る失敗テストを追加。
2. 深さ固定の反復深化 + alpha-beta最小実装。
3. `quiescence` 追加。
4. move ordering（TT move, captures, killers, history）最小導入。
5. build/test。
6. コミット。

**Gate C:**
- `go depth N` で常に `bestmove` を返す。
- 自殺手/不正手を返さない。

### Phase D: USI運用機能の実装
**Files:**
- Modify: `csharp/src/YaneuraOu.CSharp.Engine/Usi/UsiEngine.cs`
- Modify: `csharp/src/YaneuraOu.CSharp.Engine/Program.cs`
- Create: `csharp/src/YaneuraOu.CSharp.Engine/Usi/UsiOptions.cs`
- Create: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Usi/UsiProtocolTests.cs`

1. `isready`, `setoption`, `position`, `go`, `stop`, `quit`, `ucinewgame` テスト追加。
2. コマンド処理実装。
3. 時間管理（`byoyomi`, `btime/wtime`, `movetime`, `infinite`）実装。
4. build/test。
5. コミット。

**Gate D:**
- USI GUI接続で基本運用が成立。
- タイムオーバーや応答不能が発生しない。

### Phase E: 評価関数の実戦化
**Files:**
- Create: `csharp/src/YaneuraOu.CSharp.Engine/Eval/Evaluator.cs`
- Create: `csharp/src/YaneuraOu.CSharp.Engine/Eval/Nnue/*`
- Create: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Eval/*`

1. 暫定評価（駒価値+玉安全）を固定。
2. NNUE読み込みインターフェースを設計。
3. NNUE接続（最初は推論のみ）を実装。
4. build/test。
5. コミット。

**Gate E:**
- ランダム対局で極端な自滅率が減少。
- 評価が安定（同一局面で再現性あり）。

### Phase F: 性能・品質保証
**Files:**
- Create: `csharp/tools/bench/BenchRunner.cs`
- Create: `csharp/tests/YaneuraOu.CSharp.Engine.Tests/Regression/*`
- Create: `.github/workflows/csharp-ci.yml`

1. ベンチコマンド追加（nodes/s, depth固定）。
2. 回帰局面セット（合法手/詰み/千日手/宣言勝ち）追加。
3. CIで build/test/bench smoke を実行。
4. コミット。

**Gate F:**
- CI安定。
- 主要回帰テストが固定化。

### Phase G: 実戦投入準備
**Files:**
- Create: `docs/ops/usi-runbook.md`
- Create: `docs/ops/tuning-guide.md`
- Modify: `README.md`

1. 推奨`setoption`プリセットを定義。
2. 実戦ログ採取手順とクラッシュ時診断手順を整備。
3. リリース手順（VSビルド設定、配布物）を明文化。
4. コミット。

**Gate G (投入判定):**
- 長時間自己対局で致命クラッシュなし。
- USI GUI実運用で安定。
- 既知Critical/Majorが解消済み、残件がMinorのみ。

## 3. 実行順（推奨）
1. Phase A
2. Phase B
3. Phase C
4. Phase D
5. Phase E
6. Phase F
7. Phase G

## 4. 検証コマンド標準
- Build: `dotnet build csharp/YaneuraOu.CSharp.sln -v minimal`
- Test: `dotnet test csharp/YaneuraOu.CSharp.sln -v minimal`
- 対象テスト: `dotnet test csharp/YaneuraOu.CSharp.sln --filter "FullyQualifiedName~<TestClass>" -v minimal`

## 5. マイルストーン完了条件
- M1: `Critical` 完了（Phase A）
- M2: ルール完全性完了（Phase B）
- M3: 1スレッド探索で実戦対局可能（Phase C-D）
- M4: 評価と性能安定（Phase E-F）
- M5: 運用・配布準備完了（Phase G）

## 6. 直近着手タスク（次の1週間）
1. Phase A を3分割で着手（`to_move` -> `StateInfo` -> `null move`）。
2. Phase B の `PositionRepetitionTests` 作成まで完了。
3. 毎日終わりに `dotnet test` 全体通過を維持。

## 7. 実施ログ（2026-02-07）
- Phase A: 完了（`to_move` 成り不正手拒否、`StateInfo`更新順/`do-null`副作用調整）。
- Phase B: 完了（`is_repetition/is_draw/has_repeated`、宣言勝ち最小実装）。
- Phase C: 完了（1スレッド探索、反復深化+alpha-beta+quiescence、基本順序付け）。
- Phase D: 進行中（`usi/isready/setoption/position/go/stop/quit/ucinewgame`実装、USIプロトコルテスト追加）。
- Phase E: 進行中（`IEvaluator/MaterialEvaluator/NnueEvaluator/INnueBackend`導入）。
- Phase F: 着手（`BenchRunner`、回帰テスト、`csharp-ci.yml`追加）。
- Phase G: 着手（`docs/ops/usi-runbook.md`、`docs/ops/tuning-guide.md`、`README.md`導線追加）。

- Phase D: 進行中（時間制御に対する探索深さ選択のテストを追加）。
- Phase E: 進行中（`NnueModelLoader` とファイルバックエンドを追加）。

- Phase C: 完了（最小置換表、TT手優先、killer/history、終局安全判定を追加）。
- Phase D: 進行中（時間制御テスト拡充、`go`の境界挙動を強化）。
- Phase E: 進行中（NNUE読込は固定値バックエンドまで実装）。
- Phase D: 進行中（`usinewgame`受理、`usi`応答でDepth/MoveTime option広告を追加）。
- Phase E: 進行中（`setoption name EvalFile`でNNUE有効化を反映、USIテスト追加）。
- Phase F: 進行中（反復・宣言勝ちを回帰テストへ追加）。
- Phase D: 進行中（`go`の時間上限算出を実装し、探索側で時間制約を参照）。
- Phase D: 進行中（`Threads`オプション受理、`go`時の探索設定へ反映）。
- Phase H(並列化準備): 着手（探索制約にThreadsを追加、1スレッド実装のまま将来拡張点を確保）。
