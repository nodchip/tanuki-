# 2026-02-08 C++時間管理一括置換設計

## 目的
- C#版の時間管理を、やねうら王C++版(`YANEURAOU_ENGINE_NNUE`)に寄せる。
- ShogiHome/将棋所を同時対象にし、まずは秒読み超過ゼロを優先する。
- 切替オプションは設けず、既存ロジックを即時置換する。

## 方針
- `Search/Time/TimeManagement.cs` を C++ `timeman.cpp` 相当の式へ置換する。
- `UsiEngine.ParseGoLimits()` は `LimitsType` を組み立てた後、必ず新 `TimeManagement` で `optimum/maximum/minimum` を算出する。
- 失敗時は起動継続し、保守的フォールバック時間 `min(byoyomi, 1000ms)` (byoyomi未指定時は1000ms) を使う。

## データフロー
1. `go` 受信で `LimitsType` を生成。
2. `GamePly` / `PonderEnabledOption` を含む入力を `TimeManagement.Init()` に渡す。
3. `SearchStopPolicy` は `OptimumTimeMs` を depth完了停止、`MaximumTimeMs` を強制停止上限として利用。
4. `stop` / `ponderhit` の既存連携は維持する。

## ログ要件
- `DebugLog=true` 時に毎手 `info string tm ...` を出力する。
- ログに以下を含める。
  - 入力: side, btime/wtime, binc/winc, byoyomi, movetime, movestogo, ply
  - 出力: min/opt/max, remain
  - 状態: `status=ok` または `status=fallback:*`

## 実装タスク
1. `LimitsType` に `GamePly`, `MaxMovesToDraw`, `PonderEnabledOption` を追加。
2. `Position` に `game_ply()` アクセサを追加。
3. `TimeManagement` を C++式へ置換し、フォールバックと要約ログを実装。
4. `UsiEngine` から新入力を渡し、`tm` ログを出力。
5. 時間管理テストを新仕様に合わせて更新。

## 完了条件
- `dotnet build csharp/YaneuraOu.CSharp.sln -c Debug` 成功。
- `dotnet test csharp/tests/YaneuraOu.CSharp.Engine.Tests/YaneuraOu.CSharp.Engine.Tests.csproj -c Debug` 成功。
- 時間系テストで `fallback` と `tm` ログを検証。
- ShogiHome/将棋所の実運用ログ比較で秒読み超過が再現しない。
