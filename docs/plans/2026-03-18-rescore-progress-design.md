# Rescore Progress Display Design

## Summary

`Tanuki::Rescore()` の進捗表示を、長時間バッチ処理でも状況が把握しやすく、かつ Jenkins のログを膨らませすぎない実用寄りの表示へ改善する。

## Goals

- 開始直後に処理が進んでいることを目視で確認しやすい。
- 処理が進んだ後は表示頻度を落としてログ量を抑える。
- Jenkins で見ても追いやすい `info string` ベースの 1 行ログにする。
- 全体件数、進捗率、速度、経過時間、完了予想時刻を確認できる。

## Non-Goals

- コンソールの同一行を上書きする UI にしない。
- `Generate()` など既存の進捗表示仕様は変更しない。
- 進捗表示を汎用クラスへ大きく抽象化しない。

## Current Problems

- 現状は 1000 万件ごとに処理件数だけを表示しており、全体率や残り時間が分からない。
- 小さな入力では途中経過がほとんど出ず、大きな入力では速度感が見えない。
- `while (!std::feof(input_file))` ベースで、EOF まわりの意図が読み取りづらい。

## Proposed Design

### Progress Basis

- 入力ファイルを開いた直後に `_fseeki64` と `_ftelli64` で総バイト数を取得する。
- `sizeof(PackedSfenValue)` で割って総レコード数 `total_records` を求める。
- 進捗率は `num_processed / total_records` で計算する。

### Progress Output Format

- 進捗行は以下のような 1 行形式にする。

`info string Rescore 123456789/500000000 (24.7%) 1.8M rec/s elapsed 00:03:12 ETA 2026-03-18 23:41:05 next report in 00:00:32`

- 完了時は別途 `info string Rescore finished ...` を 1 行出す。
- 失敗時は `info string Rescore failed ...` を出して処理を中断する。

### Adaptive Reporting

- 更新タイミングは件数基準ではなく時間基準にする。
- 初回更新は 1 秒後、その後は 2 倍ずつ広げる。
- 更新間隔の上限は 1 時間とする。
- これにより開始直後は細かく進捗が見え、長時間ジョブではログ増加を抑える。

### Throughput Estimation

- 速度は区間ごとの生値ではなく、指数移動平均で滑らかにする。
- ETA は平滑化済み速度から算出する。
- 初回数回の更新では速度が荒れやすいので、速度がまだ安定していない場合もゼロ割を避けて安全に表示する。

## Edge Cases

- 入力ファイルを開けない場合は即座に失敗メッセージを出す。
- 出力ファイルを開けない場合は入力ファイルを閉じて失敗メッセージを出す。
- 入力サイズが `sizeof(PackedSfenValue)` の整数倍でない場合は破損入力として扱い、失敗メッセージを出す。
- 0 件ファイルは成功扱いとし、専用の完了メッセージを出す。
- ループ終了時には最終進捗を必ず 1 回表示して、最後の途中経過が欠落しないようにする。

## Implementation Scope

- 主な変更対象は `source/tanuki_training_data.cpp`。
- `Rescore()` 専用の小さな進捗ヘルパーを同ファイルの無名名前空間へ追加する。
- 既存 `ProgressReport` は変更しない。

## Verification

- 少量データで開始直後に進捗が複数回出ることを確認する。
- 長時間相当の処理で更新間隔が段階的に広がることを確認する。
- 0 件入力と不正サイズ入力で表示が破綻しないことを確認する。
