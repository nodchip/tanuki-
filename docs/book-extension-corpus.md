# 棋譜 corpus を利用した定跡 DB 延長

## 構成

- やねうら王形式の定跡 DB は探索中の data plane とし、メモリ上で更新して約1時間ごとに原子的保存する。バックアップは3世代で、`peta_shock` は保存後に手動実行する。
- SQLite は corpus、優先度、探索 task、checkpoint、N、revision、計測値を持つ control plane とする。SQLite 操作とメモリ上の定跡 DB 操作は同じ `book_lock` 内、USI 探索はロック外で行う。
- 通常 MultiPV で `min(MultiPV, 合法手数)` を distinct かつ合法な手で満たした後、MCTS が実際に訪問した経路上だけで corpus 手を `searchmoves` 探索する。仮評価値は登録しない。
- 通常探索と corpus 探索は `position startpos moves ...` または任意 root の `position sfen ... moves ...` を使う。引き分け評価0と既存 UCB は変更しない。

## 固定 snapshot の収集条件

`script/collect_book_corpus.py` は明示した JSON manifest だけを取得する。manifest の各 source には `site`、`event`、`year`、`retrieved_at`、`url`、`relative_path`、`size`、`sha256` を記録する。巨大データを暗黙に全取得しない。

- floodgate: 完了年度は年度別 `.7z` を全件、進行年度は取得日時を固定した snapshot。rating は棋譜を削る条件にせず、同年度・同 anchor era・同 connected component の信頼できる相対値だけを優先度に使う。
- WCSC: 公式公開の大会・全 stage。到達 stage、指し手側順位、対戦相手順位を ranking JSON から入れる。
- 電竜戦: 大会 ZIP または取得日時固定の個別 CSA。平手初期局面から始まる棋譜だけを採用し、駒落ちは除外する。

収集例:

```powershell
$python = 'C:\Users\nodchip\AppData\Local\Programs\Python\Python312\python.exe'
& $python script\collect_book_corpus.py --manifest C:\corpus-manifests\pilot.json --destination C:\corpus-data\pilot
```

取り込みは延長を手動停止してから site/event/year ごとに実行する。`retrieved-at` は Unix 秒で固定する。壊れた棋譜は `ingest_error` に残り、coverage 分母から除外される。

```powershell
& $python script\manage_book_corpus.py ingest --db C:\book-extension-state\pilot\corpus.sqlite --site floodgate --event floodgate-2026-07 --year 2026 --retrieved-at 1783785600 --input C:\corpus-data\pilot\floodgate-2026-07
& $python script\manage_book_corpus.py ingest --db C:\book-extension-state\pilot\corpus.sqlite --site wcsc --event wcsc36 --year 2026 --retrieved-at 1783785600 --input C:\corpus-data\pilot\wcsc36.zip
& $python script\manage_book_corpus.py ingest --db C:\book-extension-state\pilot\corpus.sqlite --site denryu --event denryu6 --year 2025 --retrieved-at 1783785600 --input C:\corpus-data\pilot\denryu6.zip
```

同一棋譜の別 mirror は raw source を両方保持し、完全に同じ対局者・指し手列だけを同一 logical game とする。AI名は Unicode 正規化後の完全一致または明示 alias のみを使う。

## 優先度データ

WCSC/電竜戦 ranking JSON は大会 metadata と `official_name, stage, stage_tier, rank, participants` の配列を持つ。floodgate rating JSON は snapshot 品質情報と `player_name, rating, effective_games, connected_to_anchor, anchor_margin, snapshot_percentile` を持つ。

```powershell
& $python script\manage_book_corpus.py ranking-import --db C:\book-extension-state\pilot\corpus.sqlite --json C:\corpus-data\pilot\wcsc36-ranking.json
& $python script\manage_book_corpus.py rating-import --db C:\book-extension-state\pilot\corpus.sqlite --json C:\corpus-data\pilot\floodgate-rating.json --config config\book-extension-pilot.toml
```

3年 bucket を最上位キーとし、bucket 内で大会 stage・指し手側順位・対戦相手順位を比較する。floodgate は anchor 接続と effective games による信頼度を先に比較し、参加者数で rating を直接乗除算しない。site node 予算は WCSC 40%、電竜戦40%、floodgate20%で、空 queue の枠は実消費 node に基づき再配分される。

## pilot

入力は floodgate 直近年度の1か月、WCSC直近1大会の全 stage、電竜戦直近1大会の平手棋譜、現在の定跡 DB のコピーとする。元 DB を直接変更しない。

設定は `config/book-extension-pilot.toml`（8 engine、各1 thread、2+2+4、corpus同時1、general poolの25%）を使う。まず10分 smoke、その後同じコマンドで24時間に伸ばす。

```powershell
$python = 'C:\Users\nodchip\AppData\Local\Programs\Python\Python312\python.exe'
$common = @(
  '--config', 'config\book-extension-pilot.toml',
  '--input', 'C:\book-extension-state\pilot\input-book.db',
  '--output', 'C:\book-extension-state\pilot\output-book.db',
  '--engine', 'C:\engine\YaneuraOu.exe',
  '--nodes', '3000000', '--multipv', '4',
  '--black-target', 'C:\book-extension-state\pilot\peta-shock.db',
  '--white-target', 'C:\book-extension-state\pilot\peta-shock.db',
  '--corpus-db', 'C:\book-extension-state\pilot\corpus.sqlite'
)
& $python script\run_book_extension.py @common --max-runtime-sec 600
& $python script\run_book_extension.py @common --max-runtime-sec 86400
```

coverage:

```powershell
& $python script\manage_book_corpus.py coverage --db C:\book-extension-state\pilot\corpus.sqlite --book C:\book-extension-state\pilot\output-book.db --snapshot-id pilot-24h --output C:\book-extension-state\pilot\coverage.json
```

観測値は正常/除外局数、SQLiteサイズ、unique/occurrence coverage、site別 `metric_counter` node数、N、revision、通常探索速度、メモリ、保存時間、停止時間、再開後 hash とする。

## Jenkins Freestyle

Build step は `script/run_extend_book_mcts_jenkins.ps1` を実行し、`ExtensionScript` に `script/run_book_extension.py`、残りを上記引数として渡す。ラッパーは子だけに `BUILD_ID=dontKillMe` を継承させる。Jenkins Abort でラッパーが終了すると heartbeat が止まり、子は10秒後に USI `stop`、中断結果破棄、最終保存を行う。通常の手動停止は `script/request_extend_book_stop.ps1 -StateDir C:\book-extension-state\pilot` を使う。

実 Jenkins の停止ボタン試験は別マシンで行う。Abort 後に `[stop] reason=heartbeat-expired` が記録され、出力 DB が読めること、再開時に task が重複しないことを確認する。終了時の全定跡検証は `GracefulStopTimeoutSec` を超える可能性があるため、停止所要時間も測定する。

## 搬送

停止後に bundle を作る。共有 registry は全延長マシンから同じパスに見える必要がある。

```powershell
& $python script\manage_book_corpus.py bundle-create --book C:\book-extension-state\pilot\output-book.db --db C:\book-extension-state\pilot\corpus.sqlite --config config\book-extension-pilot.toml --vulnerability-book C:\book-extension-state\pilot\peta-shock.db --output C:\transfer\pilot.zip
& $python script\manage_book_corpus.py bundle-verify --bundle C:\transfer\pilot.zip --destination C:\book-extension-work\pilot
& $python script\manage_book_corpus.py bundle-claim --bundle-id '<manifest bundle_id>' --registry \\server\book-extension-claims --machine-id $env:COMPUTERNAME
```

同じ bundle ID の2台目は共有 registry の排他的 claim で拒否される。最新棋譜の追加時は延長を停止し、transactional ingest、coverage snapshot、bundle再作成、再開の順に行う。N は維持し、飽和 counter だけをリセットする。

`peta_shock` はこの一連の処理から自動実行しない。保存済み DB に対し、利用者が任意の時点で手動実行する。