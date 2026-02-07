# USI Info Output Design (Depth Completion)

## 目的
- ShogiHome/将棋所で `go` 実行中に、反復深化の各depth完了時点でUSI `info` を逐次出力する。
- `bestmove` 応答経路は既存互換を維持し、探索中ログは即時出力経路へ分離する。

## 方針
1. `Searcher` に進捗通知を追加する。
- `Search(Position, SearchLimits, Action<SearchProgress>?)` を導入する。
- 反復深化ループの各depth完了時に `SearchProgress` を通知する。

2. `UsiEngine` に即時出力シンクを追加する。
- `OutputSink: Action<string>?` を追加する。
- 進捗通知をUSI `info` 文字列へ整形し、`OutputSink` に送る。
- `OutputSink` 未設定時は `info` を抑制し、`HandleCommand()` の戻り値互換を維持する。

3. `Program` で標準出力へ接続する。
- `engine.OutputSink = Console.WriteLine;` を設定し、GUI実運用で即時表示する。

## 出力フォーマット
- `info depth <d> seldepth <sd> score <cp|mate> <v> nodes <n> nps <nps> time <ms> hashfull <hf> currmove <m> pv <pv...>`
- `score mate` 判定: `abs(score) >= 100000 - 512`。
- `mate` 値: `matePly = max(1, 100000 - abs(score) + 1)`、負スコアは符号反転。
- `hashfull` は現行TT構造の都合で `0` 固定。

## テスト
- `go depth 2` でdepthごとに `info depth` が2件以上出力されること。
- `info` に `pv` と `currmove` が含まれること。
- 玉欠落などmate評価局面で `score mate` が出ること。
- 既存 `bestmove` 応答テストが維持されること。
