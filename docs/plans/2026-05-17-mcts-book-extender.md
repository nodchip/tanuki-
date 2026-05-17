# MCTS Book Extender Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** やねうら王形式の定跡 DB を、USI エンジンの MultiPV 探索と MCTS 風 traversal で継続拡張する Python スクリプトを追加する。

**Architecture:** `script/extend_book_mcts.py` に、DB パーサ、UCB traversal、USI エンジンラッパ、book 更新、Min-Max 伝搬、定期保存を分けて実装する。テストは `script/test_extend_book_mcts.py` に置き、外部エンジンを起動しない純粋ロジックを中心に検証する。

**Tech Stack:** Python 3, unittest, cshogi, USI process communication.

---

### Task 1: Book DB Parser and Writer

**Files:**
- Create: `script/test_extend_book_mcts.py`
- Create: `script/extend_book_mcts.py`

**Step 1: Write failing tests**

パーサが省略された `depth` / `visits` を `0` にし、出力時に評価値降順の安定ソートを行うことを検証する。

**Step 2: Run test to verify it fails**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Implement parser and writer**

`BookEntry`, `BookPosition`, `OpeningBook`, `load_book`, `write_book_atomic` を実装する。

**Step 4: Run test to verify it passes**

Run: `python -m unittest script.test_extend_book_mcts -v`

### Task 2: UCB and Book Update Rules

**Files:**
- Modify: `script/test_extend_book_mcts.py`
- Modify: `script/extend_book_mcts.py`

**Step 1: Write failing tests**

UCB が初期訪問回数 `0` を有限値として扱うこと、既存 `move` の直接探索結果は `eval/depth` を上書きせず `response == none` のみ補完し、`visits` を更新できることを検証する。

**Step 2: Run test to verify it fails**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Implement UCB and merge logic**

`score_to_winrate`, `calculate_ucb`, `merge_search_results` を実装する。

**Step 4: Run test to verify it passes**

Run: `python -m unittest script.test_extend_book_mcts -v`

### Task 3: Traversal and Min-Max Propagation

**Files:**
- Modify: `script/test_extend_book_mcts.py`
- Modify: `script/extend_book_mcts.py`

**Step 1: Write failing tests**

DB 登録手だけで root から leaf を選ぶこと、leaf から root まで選択経路上の `eval` を Min-Max で更新することを検証する。

**Step 2: Run test to verify it fails**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Implement traversal and propagation**

`PositionNavigator`, `select_leaf_path`, `propagate_minimax` を実装する。cshogi を使える環境では SFEN 遷移に cshogi を使う。

### Task 3.5: PetaNext-Style Leaf Candidate Pruning

**Files:**
- Modify: `script/test_extend_book_mcts.py`
- Modify: `script/extend_book_mcts.py`

**Step 1: Write failing tests**

`--eval-diff` 指定時に、定跡側手番では bestmove のみ、非定跡側手番では `root_best_eval - eval_diff` 以上の登録手だけが UCB 候補になることを検証する。`--max-ply` で traversal が停止することも検証する。

**Step 2: Run test to verify it fails**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Implement pruning**

`filter_entries_for_peta_rule`, `sfen_turn`, `root_best_eval` を追加し、`select_leaf_path` と CLI に `--eval-diff`, `--book-side`, `--max-ply` を接続する。

**Step 4: Run test to verify it passes**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 4: Run test to verify it passes**

Run: `python -m unittest script.test_extend_book_mcts -v`

### Task 4: USI Engine and Runner CLI

**Files:**
- Modify: `script/test_extend_book_mcts.py`
- Modify: `script/extend_book_mcts.py`

**Step 1: Write failing tests**

USI の `info multipv` 解析、起動時 1 回だけの `setoption` / `isready` / `usinewgame` 初期化、CLI 引数の解釈を検証する。

**Step 2: Run test to verify it fails**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Implement engine wrapper and CLI**

`UsiEngine`, `parse_info_line`, `run_extend_loop`, `main` を実装する。

**Step 4: Run test to verify it passes**

Run: `python -m unittest script.test_extend_book_mcts -v`

### Task 4.5: Automatic Stop Conditions

**Files:**
- Modify: `script/test_extend_book_mcts.py`
- Modify: `script/extend_book_mcts.py`

**Step 1: Write failing tests**

追加局面数、総思考回数、総ノード数、実行時間の停止判定を検証する。並列実行時に思考回数・総ノード数を超えて探索を開始しない予約判定も検証する。

**Step 2: Run test to verify it fails**

Run: `python -m unittest script.test_extend_book_mcts -v`

### Task 4.6: Progress Reporting

**Files:**
- Modify: `script/test_extend_book_mcts.py`
- Modify: `script/extend_book_mcts.py`

**Step 1: Write failing tests**

一定間隔の `[progress]`、保存時の `[save]`、停止時の `[stop]` が、経過時間・総思考回数・追加局面数・総ノード数を含んで出力されることを検証する。

**Step 2: Run test to verify it fails**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Implement progress reporter**

`ProgressReporter` と `--progress-interval-sec` を追加し、探索完了、定期保存、終了処理へ接続する。

**Step 4: Run test to verify it passes**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Implement stop limits**

`RunStats` と `StopLimits` を追加し、`--max-added-positions`, `--max-searches`, `--max-total-nodes`, `--max-runtime-sec` を CLI に接続する。

**Step 4: Run test to verify it passes**

Run: `python -m unittest script.test_extend_book_mcts -v`

### Task 5: Verification

**Files:**
- Check: `script/extend_book_mcts.py`
- Check: `script/test_extend_book_mcts.py`

**Step 1: Syntax check**

Run: `python -m py_compile script/extend_book_mcts.py script/test_extend_book_mcts.py`

**Step 2: Unit tests**

Run: `python -m unittest script.test_extend_book_mcts -v`

**Step 3: Review diff**

Run: `git diff -- script/extend_book_mcts.py script/test_extend_book_mcts.py docs/plans/2026-05-17-mcts-book-extender.md`
