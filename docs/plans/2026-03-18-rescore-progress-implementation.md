# Rescore Progress Display Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `Tanuki::Rescore()` の進捗表示を、開始直後は細かく、その後は徐々に更新頻度を落とす `info string` ベースの実用表示へ改善する。

**Architecture:** `source/tanuki_training_data.cpp` の無名名前空間に `Rescore()` 専用の進捗表示ヘルパーを追加し、入力ファイルサイズから総レコード数を計算して進捗率と ETA を算出する。処理ループは `fread()` 戻り値ベースへ整理し、開始直後の細かい更新と長時間処理でのログ抑制を両立する時間バックオフ方式を採用する。

**Tech Stack:** C++17, stdio file I/O, `time_t`, existing USI-style console logging

---

### Task 1: 現状の `Rescore()` を安全に観察できる土台を整える

**Files:**
- Modify: `source/tanuki_training_data.cpp`

**Step 1: 進捗表示要件をコメントで整理する**

`Rescore()` 近傍に、開始直後は更新頻度を高くし、後半は更新頻度を下げる意図が分かる短いコメントを追加する方針を決める。

**Step 2: 入力ファイルサイズ取得とエラー経路のテスト観点を明文化する**

確認ポイント:
- `fopen()` 失敗時に処理を継続しないこと
- ファイルサイズがレコード境界と一致しない場合に失敗させること
- 0 件ファイルを成功扱いにすること

**Step 3: 最小限のコード変更位置を決める**

変更範囲:
- 無名名前空間にローカルヘルパー追加
- `Rescore()` のファイルオープン部
- `Rescore()` の処理ループ
- `Rescore()` の完了ログ

**Step 4: 変更前の挙動を手元で確認する**

Run: `rg -n "void Tanuki::Rescore|Finished\\.|progress_duration|next_progress" source/tanuki_training_data.cpp`
Expected: 既存の件数ベース進捗表示箇所が確認できる

**Step 5: Commit**

```bash
git add source/tanuki_training_data.cpp
git commit -m "Prep rescore progress reporting refactor"
```

### Task 2: 失敗ケースとファイル総量計算を実装する

**Files:**
- Modify: `source/tanuki_training_data.cpp`

**Step 1: 失敗系を先に扱うための最小実装を書く**

追加内容:
- 入出力 `fopen()` 失敗時の `info string Rescore failed ...`
- 入力ファイルサイズ取得
- レコードサイズ不整合の検出

**Step 2: 失敗時のメッセージを確認しやすい形に整える**

メッセージ例:

```cpp
sync_cout << "info string Rescore failed to open input file: " << input_file_path << sync_endl;
```

**Step 3: 0 件入力の完了経路を実装する**

要件:
- 進捗表示本体へ入る前に `0 records` の完了メッセージを出す
- 入出力ファイルを確実に close する

**Step 4: 変更箇所を確認する**

Run: `rg -n "failed to open|invalid input file size|0 records" source/tanuki_training_data.cpp`
Expected: 追加した失敗・境界条件の分岐が確認できる

**Step 5: Commit**

```bash
git add source/tanuki_training_data.cpp
git commit -m "Handle rescore file size and open failures"
```

### Task 3: 可変更新の進捗ヘルパーを追加する

**Files:**
- Modify: `source/tanuki_training_data.cpp`

**Step 1: まずヘルパーの呼び出し側を決める**

要件:
- `num_processed` 更新後に `MaybeReport(num_processed)`
- ループ終了後に `ReportFinal(num_processed)`

**Step 2: ヘルパーの最小実装を書く**

ヘルパー責務:
- 総レコード数保持
- 開始時刻、前回更新時刻、前回件数保持
- EMA 速度計算
- 更新間隔の倍々バックオフ

**Step 3: 出力フォーマットを固定する**

表示例:

```cpp
info string Rescore 123/1000 (12.3%) 1.5k rec/s elapsed 00:00:10 ETA 2026-03-18 21:12:13 next report in 00:00:08
```

**Step 4: 文字列整形の補助関数を追加する**

候補:
- `FormatDuration(time_t seconds)`
- `FormatTimestamp(time_t t)`
- `FormatRecordsPerSecond(double value)`

**Step 5: Commit**

```bash
git add source/tanuki_training_data.cpp
git commit -m "Add adaptive rescore progress reporter"
```

### Task 4: `Rescore()` の読み書きループを整理する

**Files:**
- Modify: `source/tanuki_training_data.cpp`

**Step 1: `while (!std::feof(...))` をやめて `fread()` 主導へ切り替える**

期待する形:

```cpp
for (;;) {
    size_t num_samples = std::fread(...);
    if (num_samples == 0) {
        break;
    }
    ...
}
```

**Step 2: `grp.nn_forward()` に渡す件数を見直す**

確認事項:
- 末尾チャンクでも既存バッファを使って安全に処理できるか
- `num_samples` と `batch_size` の扱いが壊れないか

**Step 3: 進捗更新を新ヘルパーへ接続する**

要件:
- 各チャンク後に `MaybeReport`
- 終了時に `ReportFinal`
- 既存の `progress_duration` と `next_progress` を削除

**Step 4: ループまわりの差分を確認する**

Run: `git diff -- source/tanuki_training_data.cpp`
Expected: 件数ベース更新が消え、可変間隔更新へ置き換わっている

**Step 5: Commit**

```bash
git add source/tanuki_training_data.cpp
git commit -m "Refactor rescore loop around adaptive reporting"
```

### Task 5: 検証と仕上げを行う

**Files:**
- Modify: `source/tanuki_training_data.cpp`
- Test: repository-specific manual build and smoke verification commands

**Step 1: ビルドコマンドを実行する**

Run: project-appropriate compile command for this repository
Expected: `tanuki_training_data.cpp` を含むビルドが成功する

**Step 2: 進捗表示の静的確認をする**

Run: `rg -n "info string Rescore|finished|next report in" source/tanuki_training_data.cpp`
Expected: 開始中と完了時のメッセージが意図どおり存在する

**Step 3: 既存コードとの整合性を見直す**

確認事項:
- `sync_cout` と `std::cout` の使い分け
- 例外ではなく既存流儀のエラー終了に揃っているか
- 使っていない変数や include が残っていないか

**Step 4: 最終確認を行う**

Run: `git diff --stat`
Expected: 変更が進捗表示改善の範囲に収まっている

**Step 5: Commit**

```bash
git add docs/plans/2026-03-18-rescore-progress-design.md docs/plans/2026-03-18-rescore-progress-implementation.md source/tanuki_training_data.cpp
git commit -m "Improve rescore progress reporting"
```
