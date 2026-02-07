# Position C++ Compatibility Design (to_move / pseudo_legal / legal / legal_promote)

Date: 2026-02-07
Target: `csharp/src/YaneuraOu.CSharp.Engine/Core/Position.cs`

## Goal
`source/position.cpp` の判定順序と分岐意図を C# 側に再現し、TT/兄弟局面由来の混入手に対する耐性を上げる。

## Scope
- `to_move(Move16)`
- `pseudo_legal(Move, bool all)`
- `legal(Move)`
- `legal_promote(Move)`
- 関連補助 (`legal_pawn_drop`, `legal_drop`)

## Architecture
1. `to_move(Move16)`
- special move (`none/null/win`) をそのまま返す。
- drop は `make_piece(side_to_move, dropped_piece)` を上位 16bit に付与。
- non-drop は `from` の駒を参照し、手番側でない場合 `Move.none()`。
- promote 指し手は `make_promoted_piece(piece_on(from))` を上位 16bit に付与。

2. `pseudo_legal(Move, all)`
- 先頭で `is_ok` と `to` の範囲確認。
- drop 分岐:
  - `moved_after_piece` 整合
  - 手駒存在 / 空き升
  - 打てない段制約
  - 歩の二歩 + 打ち歩詰め
  - 王手中は合駒制約（単王手のみ、両王手不可）
- non-drop 分岐:
  - `from` の自駒 / 駒の利き / 自駒占有チェック
  - promote / non-promote の `moved_after_piece` 整合
  - `all` に応じた不成制約差分
  - 王手中は capture/interpose 制約

3. `legal(Move)`
- `pseudo_legal(m, true)` を前提。
- drop は true（打ち歩詰めは `pseudo_legal` 側で除外）。
- king move は移動先の被攻撃判定。
- non-king move は pin 判定（`blockers_for_king` + `aligned`）。

4. `legal_promote(Move)`
- promote 指し手のみ判定。
- from または to が敵陣なら true。

## Testing Strategy
- `to_move`: special / drop / non-drop / side mismatch / promote。
- `pseudo_legal`: moved_after 不整合（drop/promote/non-promote）
- 王手中の drop/non-drop 合駒制約。
- `all` フラグ差分（不成許容差）。
- `legal`: king move / pin move の軽量判定。

## Execution Policy
- TDD で小さい条件単位に RED->GREEN。
- 各変更後に対象テスト、最後に `dotnet test` と `dotnet build` を実行。
