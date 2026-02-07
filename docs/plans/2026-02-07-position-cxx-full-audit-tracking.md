# Position C++ Full Audit Tracking (Batch 1)

Date: 2026-02-07
Plan: `docs/plans/2026-02-07-position-cxx-full-audit.md`
Scope: Execution Plan 1-3

## 1. C++ Position API Ledger (definition order)
| No | C++ 関数 | 区分 |
|---:|---|---|
| 1 | `Position::init` | 初期化 |
| 2 | `Position::set_castling_right` | 初期化補助(Stockfish) |
| 3 | `Position::set_check_info` | 王手情報更新 |
| 4 | `Position::set_state` | 局面状態初期化 |
| 5 | `Position::set` | SFEN設定 |
| 6 | `Position::fen` / `Position::sfen` | 文字列化 |
| 7 | `Position::update_slider_blockers` | pin情報更新 |
| 8 | `Position::attackers_to` / `attackers_to_exist` | 利き判定 |
| 9 | `Position::flipped_sfen` / `sfen_to_flipped_sfen` | 反転SFEN |
| 10 | `Position::update_bitboards` | ビットボード更新 |
| 11 | `Position::update_kingSquare` | 玉位置更新 |
| 12 | `Position::moves_from_start` | デバッグ履歴 |
| 13 | `Position::attackers_to_pawn` | 打ち歩詰め補助 |
| 14 | `Position::gives_check` | 王手判定 |
| 15 | `Position::is_mated` | 詰み判定 |
| 16 | `Position::legal_drop` | 打ち歩詰め判定補助 |
| 17 | `Position::legal_pawn_drop` | 二歩/打ち歩詰め判定 |
| 18 | `Position::pseudo_legal` / `pseudo_legal_s` | 擬似合法判定 |
| 19 | `Position::legal` | 合法判定 |
| 20 | `Position::legal_promote` | 成り条件判定 |
| 21 | `Position::to_move` | Move16展開 |
| 22 | `Position::do_move_impl` / `do_move` | 局面更新 |
| 23 | `Position::key_after` | 事後ハッシュ |
| 24 | `Position::undo_move_impl` / `undo_move` | 局面巻き戻し |
| 25 | `Position::do_null_move` / `undo_null_move` | null move |
| 26 | `Position::see_ge` | SEE |
| 27 | `Position::is_draw` | 引き分け判定 |
| 28 | `Position::is_repetition` (2 overloads) | 千日手判定 |
| 29 | `Position::has_repeated` | 反復履歴判定 |
| 30 | `Position::update_entering_point` | 入玉点更新 |
| 31 | `Position::DeclarationWin` | 宣言勝ち |
| 32 | `Position::flip` | 局面反転 |
| 33 | `Position::pos_is_ok` | 整合性検査 |
| 34 | `Position::UnitTest` | C++内蔵テスト |

## 2. C# Mapping (1:1)
| C++ 関数 | C# 対応 | 対応状態 | 備考 |
|---|---|---|---|
| `set` | `Position.set` | 完全 | SFEN入力は実装済み |
| `sfen` | `Position.sfen` | 完全 | `sfen(int)` あり |
| `flipped_sfen` / `sfen_to_flipped_sfen` | `Position.flipped_sfen` / `Position.sfen_to_flipped_sfen` | 完全 | |
| `piece_on` / `pieces` | `Position.piece_on` / `Position.pieces` | 部分 | C++の多引数テンプレート版は未対応 |
| `attackers_to` | `Position.attackers_to` | 部分 | `occ` 指定版が未対応 |
| `update_slider_blockers` | `Position.update_slider_blockers` | 完全 | |
| `gives_check` | `Position.gives_check` | 完全 | |
| `in_check` | `Position.in_check` | 完全 | C++は `checkers()` ベース |
| `pseudo_legal` | `Position.pseudo_legal` | 部分 | C++ `pseudo_legal_s<All>` に対する分岐は概ね再現 |
| `legal` | `Position.legal` | 部分 | 事前条件の扱いが異なる（下記差分） |
| `legal_drop` / `legal_pawn_drop` | `Position.legal_drop` / `Position.legal_pawn_drop` | 部分 | 参照データ更新の差あり |
| `legal_promote` | `Position.legal_promote` | 完全 | |
| `to_move` | `Position.to_move` | 部分 | 非成駒成りの扱い差分あり |
| `do_move` | `Position.do_move` | 部分 | C++ incremental 更新を未再現 |
| `undo_move` | `Position.undo_move` | 部分 | スナップショット復元で代替 |
| `do_null_move` / `undo_null_move` | `Position.do_null_move` / `Position.undo_null_move` | 部分 | repetition/check連鎖更新未対応 |
| `is_mated` | `Position.is_mated` | 完全 | MoveGen依存 |
| `key_after` | なし | 未対応 | C++のみ |
| `is_draw` / `is_repetition` / `has_repeated` | なし | 未対応 | C++のみ |
| `update_entering_point` / `DeclarationWin` | なし | 未対応 | C++のみ |
| `flip` | なし | 未対応 | C++のみ |
| `pos_is_ok` | なし | 未対応 | C++のみ |
| `see_ge` | なし | 未対応 | C++のみ |

## 3. Extracted Differences (branch/precondition/side-effect/exception)

### 3.1 Critical
| ID | 差分 | 種別 | 影響 |
|---|---|---|---|
| D-CRIT-01 | `to_move` で C++ は「成れない駒の成り」を `Move::none()` 化するが、C# は `make_promoted_piece()` 経由で通る可能性がある | 事前条件 | TT由来不正手混入時に誤受理の恐れ |
| D-CRIT-02 | C++ `do_move/undo_move` は `checkers/repetition/continuousCheck/hash` を逐次更新するが、C# はスナップショット復元中心で同等情報を保持していない | 副作用順 | 探索接続時に反復・王手連続判定が不整合 |

### 3.2 Major
| ID | 差分 | 種別 | 影響 |
|---|---|---|---|
| D-MAJ-01 | C++ `attackers_to(sq, occ)` / `attackers_to_exist` があり占有差し替え判定を提供。C# は `occ` 指定版なし | API欠落 | 王手・合法性判定の最適化/一致性に影響 |
| D-MAJ-02 | C++ `legal` は `effected_to(~us, to, from)`（移動元玉除去を明示）で判定。C# は盤面を一時更新して `attackers_to` で代替 | 分岐実装差 | 意図は近いが将来差分混入リスク |
| D-MAJ-03 | C++ `do_null_move` は `continuousCheck`/`repetition` 系を初期化。C# は `sideToMove` と `st.hand` のみ更新 | 副作用欠落 | null move探索導入時に誤判定リスク |
| D-MAJ-04 | C++ `key_after` が存在し、探索で投機 prefetch に利用可能。C# は未実装 | API欠落 | 探索性能・将来互換性 |
| D-MAJ-05 | C++ `is_draw/is_repetition/has_repeated` 実装あり。C# は未実装 | 機能欠落 | 探索終局判定を移植不能 |
| D-MAJ-06 | C++ `DeclarationWin/update_entering_point` 実装あり。C# は未実装 | 機能欠落 | ルール互換未達 |

### 3.3 Minor
| ID | 差分 | 種別 | 影響 |
|---|---|---|---|
| D-MIN-01 | C++ は `fen/square<Pt>/capture/capture_stage/...` 等の補助APIを持つ。C# は最小セットのみ | API差 | テスト移植時の補助不足 |
| D-MIN-02 | C++ `pos_is_ok` による整合性チェックあり。C# は同等関数なし | 例外/検証 | デバッグ性低下 |
| D-MIN-03 | C++ `flip()` 実装あり。C# は `flipped_sfen` のみ | API差 | 局面反転デバッグの利便性差 |

## 4. Batch 1 Outcome (Plan step 1-3)
- [x] C++ `Position` API 台帳作成
- [x] C# 対応マッピング作成
- [x] 分岐順・事前条件・副作用・例外系差分の一次抽出

## 5. Next in Plan
Execution Plan 4-5:
1. 差分ごとの最小再現シナリオとテスト案を付与
2. `Critical -> Major -> Minor` 順の修正バックログを確定
