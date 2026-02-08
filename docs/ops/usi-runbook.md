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

## 6.1 時間管理スモーク
- PowerShell で `csharp/tools/selfplay/TimeManagementSmoke.ps1` を実行する。
- スクリプトは以下を自動検証する。
  - 通常ケースで `info string tm status=ok` が複数回出る。
  - 予算不足ケースで `info string tm status=fallback:` が出る。
  - `bestmove` 応答数が想定回数以上である。
  - 出力に `Exception` が含まれない。
- 主な引数:
  - `-Profile ShogiHomeByoyomi3s`（ShogiHome秒読み3秒プリセット）
  - `-Profile ShogidokoroFischer`（将棋所フィッシャー5分+5秒プリセット）
  - `-Profile Custom`（既定、下記個別引数を使用）
  - `-ByoyomiMs 3000`
  - `-BTimeMs 15000 -WTimeMs 15000`
  - `-IncMs 0`

## 6.2 実対局ログ集計
- ShogiHome/将棋所のUSIログを保存し、次を実行する。
  - `powershell -ExecutionPolicy Bypass -File csharp/tools/selfplay/AnalyzeUsiMatchLogs.ps1 -LogDir <ログフォルダ>`
- 主な出力:
  - `tm status=ok/fallback` 件数
  - `bestmove resign` 件数
  - `反則手` 件数
  - `info depth ... time` からの秒読み超過疑い件数
- 失敗条件を有効化する場合:
  - `-FailOnIllegalMove`
  - `-FailOnOverrun`

## 6.3 GUIログ監査（ワンコマンド）
- 上位スクリプト:
  - `powershell -ExecutionPolicy Bypass -File csharp/tools/selfplay/RunGuiLogAudit.ps1 -Profile ShogiHome -LogDir <ログフォルダ>`
  - `powershell -ExecutionPolicy Bypass -File csharp/tools/selfplay/RunGuiLogAudit.ps1 -Profile Shogidokoro -LogDir <ログフォルダ>`
- `-Strict` を付けると、反則手または秒読み超過疑いが1件でもあれば終了コード1を返す。
- 監査結果は `logs/usi-audit/*.json` に保存される。

## 7. NNUE parity strict
- C++版エンジン実行ファイルを `source/YaneuraOu-by-gcc.exe` に配置する。
- `eval/nn.bin` を配置する。
- PowerShell で `csharp/tools/nnue/RunNnueParityStrict.ps1` を実行する。
- 既定で `600` 局面を生成し、`eval/nnue-parity-cases.jsonl` を作成する。
- 生成時に `info string parity-sfen coverage kingmove ... promotion ... drop ...` が出力され、各件数が1以上であることをスクリプトが検証する。
- strictテストは `NNUE_PARITY_STRICT=1` を一時設定して `NnueParityTests` のみ実行する。
- C++評価取得に失敗した場合は `eval/parity-failures/` 配下に `meta.json` / `usi.txt` / `stdout.txt` / `stderr.txt` が出力される。
- 失敗ログ出力先を変更したい場合は `-FailureLogDir <dir>` を指定する。
- 一時ファイルを保持したい場合は `-KeepTempOnError` を指定する。
