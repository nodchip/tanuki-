# USI Runbook (C#移植版)

## 1. ビルド
- Visual Studio 2022 で `csharp/YaneuraOu.CSharp.sln` を開く。
- 構成を `Debug` または `Release`、プラットフォームを `Any CPU` に設定する。
- `YaneuraOu.CSharp.Engine` をスタートアップにしてビルドする。

## 2. 最低限のUSI疎通確認
- 標準入出力で以下を送る。
  - `usi`
  - `isready`
  - `usinewgame`
  - `position startpos`
  - `go depth 1`
- `bestmove ...` が返ることを確認する。

## 3. 推奨オプション（暫定）
- `setoption name Depth value 3`
- `setoption name MoveTime value 1000`

## 4. 障害時の初動
- `dotnet test csharp/YaneuraOu.CSharp.sln -v minimal` を実行し、回帰有無を確認する。
- `dotnet run --project csharp/src/YaneuraOu.CSharp.Engine/YaneuraOu.CSharp.Engine.csproj -- bench 1 2` で最低限の探索動作を確認する。
- 再現局面がある場合は `position ...` と `go ...` のログを保存する。

## 5. 収集するログ
- 入力したUSIコマンド列。
- エンジン出力（`bestmove`/`info string`）。
- 実行バイナリ（Debug/Release）とコミットID。

## 6. 自己対局スモーク
- PowerShell で `csharp/tools/selfplay/SelfPlaySmoke.ps1` を実行する。
- 既定は40手分の疎通確認。手数を変更する場合は `-Moves 200` のように指定する。
- 深さ固定で実行する場合は `-Depth 1`（既定値）を使用する。
- 秒読み系で実行する場合は `-ByoyomiMs 1000` のように指定する。
- スクリプトは `bestmove` 応答数と例外文字列を自動検証し、異常時は終了コード1を返す。

## 7. NNUE parity strict
- C++版エンジン実行ファイルを `source/YaneuraOu-by-gcc.exe` に配置する。
- `eval/nn.bin` を配置する。
- PowerShell で `csharp/tools/nnue/RunNnueParityStrict.ps1` を実行する。
- 既定で `600` 局面を生成し、`eval/nnue-parity-cases.jsonl` を作成する。
- 生成時に `info string parity-sfen coverage kingmove ... promotion ... drop ...` が出力され、各件数が1以上であることをスクリプトが検証する。
- strictテストは `NNUE_PARITY_STRICT=1` を一時設定して `NnueParityTests` のみ実行する。
