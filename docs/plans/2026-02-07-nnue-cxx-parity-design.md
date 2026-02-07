# NNUE C++ Parity Design (YANEURAOU_ENGINE_NNUE)

## 1. 目的
- C#版の評価を、やねうら王C++版 `YANEURAOU_ENGINE_NNUE` ビルド時の挙動へ原則1:1で一致させる。
- 同一局面・同一手順で評価値と更新タイミングを一致させる。

## 2. 現状
- `eval/nn.bin` は配置済み（`eval/nn.bin`）。
- ただし現行 `NnueModelLoader` はテキスト整数を読む仮実装であり、実バイナリNNUEは未対応。
- したがって、まずはローダ/特徴量/accumulatorの本実装が必要。

## 3. 完了条件
- 基準局面セット（200〜500局面）で C++ 基準値との差分が 0（または許容誤差内）
- `do_move/undo_move/null` を含む連続手順で、差分更新評価と全再計算評価の不一致0
- USI運用で `setoption name EvalFile value eval/nn.bin` 後に安定稼働（クラッシュ・反則手なし）

## 4. C++→C# 写像
- `NnueModelLoader`
  - NNUEファイルのヘッダ検証、重み展開、整合性チェック
- `NnueFeatureTransformer`
  - 局面から特徴量インデックス列を生成（手番/反転規則含む）
- `NnueAccumulator`
  - 全再計算/差分更新（do/undo/null）
- `NnueEvaluator`
  - 探索器向け評価インターフェース
- `NnueDebugProbe`（テスト専用）
  - C++基準比較用のダンプ出力

## 5. 実装順（TDD固定）
1. ローダ一致
   - 失敗テスト: ヘッダ/重み件数/先頭末尾値/チェックサム一致
   - 実装: バイナリローダ
2. 特徴量一致（全再計算）
   - 失敗テスト: 局面ごとの特徴量インデックス一致
3. 推論一致（全再計算のみ）
   - 失敗テスト: 局面ごとの評価値一致
4. 差分更新一致
   - 失敗テスト: do/undo/null 各手で差分更新==全再計算
5. USI接続
   - 失敗テスト: `EvalFile=eval/nn.bin` で有効化され評価反映
6. 実戦回帰
   - 自己対局スモークで反則手/クラッシュなし

## 6. テストデータ運用
- 正解データはC++側 `evaluate()` ダンプを採用する。
- 形式: `SFEN, expectedScore, optionalAccumulatorHash` のCSV/JSONL。
- まず20局面のスモールセットを作成し、通過後に500局面へ拡張。

## 7. リスクと対策
- モデル仕様差異
  - 対策: ロード直後のヘッダ/件数/チェックサム照合
- 差分更新破綻
  - 対策: 開発中は毎手 `差分==全再計算` をアサート
- 探索との干渉
  - 対策: 評価移植期間は探索ロジック変更を凍結

## 8. 直近着手タスク
1. `NnueModelLoader` バイナリ対応の失敗テスト追加
2. `eval/nn.bin` を使う読み込み成功テストを追加
3. ローダ本実装
4. build/test 実行でGREEN化

## 9. 検証コマンド
- Build: `dotnet build csharp/YaneuraOu.CSharp.sln -v minimal`
- Test: `dotnet test csharp/YaneuraOu.CSharp.sln -v minimal`
- 対象: `dotnet test csharp/YaneuraOu.CSharp.sln --filter "FullyQualifiedName~NnueModelLoaderTests" -v minimal`

## 10. 補足
- 現在の `eval/nn.bin` は本計画のローダ実装完了後に正式利用する。
- それまでは既存のフォールバック評価（Material）と併用運用。

## 11. C++基準データ生成手順（暫定）
1. C++エンジンをビルドする。
   - 例: `source` ディレクトリで `mingw32-make normal YANEURAOU_EDITION=YANEURAOU_ENGINE_NNUE COMPILER=g++ OS=Windows_NT TARGET_CPU=AVX2`
2. 評価ケースJSONLを生成する。
   - `powershell -ExecutionPolicy Bypass -File csharp/tools/nnue/GenerateNnueParityCases.ps1 -EnginePath source/YaneuraOu-by-gcc.exe -EvalDir ../eval -OutFile eval/nnue-parity-cases.jsonl`
3. C#側の一致テストを実行する。
   - `dotnet test csharp/YaneuraOu.CSharp.sln --filter "FullyQualifiedName~NnueParityTests" -v minimal`

## 12. 照合ログ（2026-02-07）
- C++ `YANEURAOU_ENGINE_NNUE` を `source/YaneuraOu-by-gcc.exe` としてビルド確認。
- `GenerateNnueParityCases.ps1` で 12局面のJSONL生成を確認。
- 厳密照合（`NNUE_PARITY_STRICT=1`）の初回差分:
  - SFEN: `lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1`
  - C++: `58`
  - C#: `-5314`
- 現状の C# NNUE は擬似重み計算であり、C++ 1:1の重み展開・推論ではないため一致しない。

## 13. 照合ログ（2026-02-07 更新）
- C# 側に `nn.bin` 実パラメータ読み込み（FeatureTransformer + 3層ネットワーク）を実装。
- HalfKP(BonaPiece)ベース特徴量へ置換し、`eval/nn.bin` を直接推論可能にした。
- 厳密照合（`NNUE_PARITY_STRICT=1`）実行結果:
  - `dotnet test csharp/YaneuraOu.CSharp.sln --filter "FullyQualifiedName~NnueParityTests" -v minimal`
  - 結果: 1件合格 / 失敗0
- strict実行時の見かけ上GREEN防止として、`NnueParityTests` に最小件数ガードを追加（`MinimumStrictCases = 20`）。
