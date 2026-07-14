# 棋譜 corpus を利用した定跡 DB 延長

## 構成

- やねうら王形式の定跡 DB は探索中の data plane とし、メモリ上で更新して約1時間ごとに原子的保存する。バックアップは3世代で、`peta_shock` は保存後に手動実行する。
- SQLite は corpus、優先度、探索 task、checkpoint、N、revision、計測値を持つ control plane とする。SQLite 操作とメモリ上の定跡 DB 操作は同じ `book_lock` 内、USI 探索はロック外で行う。
- 通常 MultiPV で `min(MultiPV, 合法手数)` を distinct かつ合法な手で満たした後、MCTS が実際に訪問した経路上だけで corpus 手を `searchmoves` 探索する。仮評価値は登録しない。
- 通常探索と corpus 探索は `position startpos moves ...` または任意 root の `position sfen ... moves ...` を使う。引き分け評価0と既存 UCB は変更しない。

## 1コマンドでの corpus 作成

定跡延長プログラムを手動停止し、対象の state directory に入力定跡を input-book.db として置いてから、次のどちらか1コマンドを実行する。

24時間pilot用:

    python script/prepare_book_corpus.py --profile pilot --state-dir C:/book-extension-state/pilot

本番運用用フルセット:

    python script/prepare_book_corpus.py --profile production --state-dir C:/book-extension-state/production

作成中の進捗は標準エラーへ即時出力され、Jenkinsではコンソールログから確認できる。storage、download、ingest、metadata、coverage、optimize、validation、publishの開始・完了と、長時間処理中の30秒ごとのheartbeatを表示する。downloadはファイル数・バイト数、ingestは処理棋譜数・accepted・excludedも表示する。正常終了時の最終JSONは従来どおり標準出力へ1行だけ出力される。

pilot は取得日時を固定した floodgate 2026、WCSC36、電竜戦6本戦の3アーカイブである。production は公式配布を確認できた floodgate 年度別15本（2011、2012、2014～2026）、WCSC35大会（第1～29回、第31～36回。第30回は中止）、平手開始の電竜戦本戦5大会（第2～6回）、第7回電竜戦TSEC第2部・先手持ち時間0秒戦347棋譜を含む。TSEC7の公式ZIPは指定局面戦と平手戦が混在するため、profileの`member_pattern`で第2部のKIFだけを取り込む。第2部347棋譜のうち341局は合法に取り込め、指し手がない6棋譜は`ingest_error`へ記録してcoverage分母から除外する。manifest上の合計ダウンロード量は約3.55 GiBで、ダウンロード済みファイルはサイズとSHA-256が一致すれば再利用する。古いWCSCのLZH展開には7-Zipが必要である。

実行中の定跡延長が book-extension.lock を保持している場合、および別のcorpus作成が corpus-build.lock を保持している場合は、既存成果物を変更せず終了する。作成は一時SQLite上で行い、検証成功後に corpus.sqlite、snapshot.json、coverage、summaryを配置する。旧 corpus.sqlite は corpus.sqlite.previous に1世代だけ保持する。失敗時はphase別終了コードと corpus-build-summary.json を残す。

入力定跡が別の場所にある場合だけ --input-book D:/path/book.db を追加する。peta_shock と定跡延長ランタイムの起動は自動実行しない。

## SQLite schema v5 と容量管理

schema v5では、局面までの累積`history_json`を各局面・候補・出典へ保存しない。`logical_game`が初期SFENと全指し手列を1回だけ保持し、`game_position`は`game_id`、`ply`、`position_id`、実戦手だけを持つ。候補の代表履歴は`representative_game_id + representative_ply`から復元するため、千日手判定に必要な順序付き履歴を失わない。canonical SFENは手数を除いた`position.position_key`へ正規化し、候補と棋譜局面は整数IDで参照する。

候補優先度は11要素を88 byteの固定長big-endian BLOBへ符号化する。SQLiteのBLOB辞書順がpriority tuple順と一致し、`candidate_position_priority_idx(position_id, active, priority_key DESC, id)`が局面別上位N検索に使われる。`candidate_source`表は廃止し、出典集合は`game_position -> source_game -> raw_source`から導出する。DDLの正本は`script/corpus_schema.sql`で、Python作成系とRustランタイムが同じファイルを使用する。

schema v4以前のDBは新ランタイムが自動変更せず、明確な再構築エラーにする。固定manifestから上記1コマンドでschema v5を再作成する。巨大な旧DBのin-place migrationは正式運用にしない。旧DBを比較する場合はread-onlyで次を実行し、logical game、raw source、source-game対応、全棋譜局面、候補集合、優先度、ingest errorの件数とストリーミングSHA-256を比較する。

```powershell
python script/compare_corpus_schemas.py `
  --old C:\path\schema-v4\corpus.sqlite `
  --new C:\path\schema-v5\corpus.sqlite `
  --output C:\path\corpus-schema-comparison.json

python script/analyze_corpus_storage.py `
  --db C:\path\schema-v4\corpus.sqlite `
  --db C:\path\schema-v5\corpus.sqlite `
  --output C:\path\corpus-storage-comparison.json
```

作成前にはmanifestの圧縮アーカイブ合計から新DBを保守的に見積もり、未取得ダウンロード量と512 MiBの安全余裕を加えて空き容量を検査する。不足時はdownload/ingest開始前に`storage` phaseで終了する。summaryの`storage`に見積値と開始時空き容量を残す。

更新時は完成した一時DBを検査してから正本へ置き換える。同一ボリュームがハードリンクを提供する場合、旧正本を`corpus.sqlite.previous`へ全量コピーせずリンクで世代化する。ハードリンク非対応環境だけcopy fallbackを使う。通常状態はactive + previousの2世代、作成中ピークはactive + previous + 新規DB + 未取得アーカイブで見積もる。WALはpublish前にcheckpoint/truncateし、公開対象へWAL/SHMを含めない。

coverageは全棋譜局面をPythonへ`fetchall()`せず、disk-backed TEMP tableとSQL集計を使う。優先度再計算も候補全件を辞書へ保持せず、棋譜局面をストリーミングして1万件単位で条件付き更新する。

### schema v5 pilot全件実測（2026-07-13）

固定pilot manifestと`user_book1.2026-07-05`を使い、上記pilotコマンドを新規state directoryへ実行した。18分41.7秒で完了し、内訳はingest 898.24秒、ranking等metadata 96.67秒、coverage 93.85秒、DB検証28.57秒だった。Pythonプロセスの観測最大Working Setは1.363 GiBで、入力定跡の読込みを含む。完成DBは3,701,354,496 bytes（3.447 GiB）で、旧27,011,112,960 bytes（25.156 GiB）から86.30%削減した。初回作成中に同時保持した最大値はDB 3.447 GiB、WAL約0.06 GiB、取得済みアーカイブ0.161 GiBで約3.67 GiBだった。実測サイズの2世代は6.89 GiB、active + previous + build + archiveの更新時推定は約10.50 GiBである。失敗runの`build/<run-id>`とphase別summaryは診断用に自動削除せず保持し、正本停止確認後に運用者が削除する。publish前にはWALをtruncateし、WAL/SHMはbundleへ含めない。

`dbstat`の主要内訳はcandidate 0.775 GiB、局面別candidate索引0.664 GiB、global candidate索引0.651 GiB、position表とunique索引の合計0.987 GiB、game_position 0.154 GiBだった。同じ100論理局面・各10回のlookupは旧schemaがp50 27.9 us / p95 70.2 us、新schemaがp50 27.8 us / p95 73.0 usで、p95は旧比1.04倍である。`EXPLAIN QUERY PLAN`はpositionのunique索引と`candidate_position_priority_idx`を使い、global scanはない。

read-onlyの全件比較では、68,142 logical game、69,389 source-game対応、70,974 raw source、全7,801,932棋譜局面（site/event/yearを含む）、5,963,731 canonical candidateと優先度、7,864,207 candidate-source組、1,585 ingest errorのストリーミングSHA-256がschema v4とv5で一致した。occurrence coverageと全内訳も同じである。unique coverageは、旧DBがSFEN手数を別局面として数えていたため、旧5,915,153局面・79,050 coveredからcanonicalな5,846,363局面・58,703 coveredへ訂正されたもので、棋譜局面の欠落ではない。`integrity_check=ok`、`foreign_key_check`は0件だった。

production manifestの圧縮アーカイブは3.547 GiBである。pilotの実測比21.35倍を単純適用した完成DBの中心推定は約75.7 GiB、コードが空き容量検査に使う保守的40倍推定は141.89 GiBである。新規作成ではアーカイブと512 MiB余裕を含め約145.93 GiBの空きを要求する。中心推定では通常のactive + previousが約151.4 GiB、更新中のactive + previous + buildが約227.1 GiBとなる。大会ごとの平均手数・重複率で変動するため、実運用では保守的推定を空き容量判定の正本とする。
## 固定 snapshot の収集条件

`script/collect_book_corpus.py` は明示した JSON manifest だけを取得する。manifest の各 source には `site`、`event`、`year`、`retrieved_at`、`url`、`relative_path`、`size`、`sha256` を記録する。巨大データを暗黙に全取得しない。

- floodgate: 完了年度は年度別 `.7z` を全件、進行年度は取得日時を固定した snapshot。rating は棋譜を削る条件にせず、同年度・同 anchor era・同 connected component の信頼できる相対値だけを優先度に使う。
- WCSC: 公式公開の大会・全 stage。到達 stage、指し手側順位、対戦相手順位を ranking JSON から入れる。
- 電竜戦: 大会 ZIP または取得日時固定の個別 CSA/KIF。平手初期局面から始まる棋譜だけを採用し、駒落ちと指定局面開始は除外する。1つの公式アーカイブに対象・対象外の棋譜が混在する場合は、ingestの`member_pattern`をアーカイブ内相対パスへ適用し、対象棋譜だけを解析する。

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

## Rust定跡延長ランタイムとpilot

長時間稼働する定跡延長、MCTS、USIプロセス管理、corpus lane、保存・停止・再開はRust版`book-extender.exe`が担当する。棋譜収集、取り込み、ranking/rating、coverage、bundle管理はPythonツールを継続利用する。

```powershell
cargo build --manifest-path script\book-extension-rs\Cargo.toml --release --bin book-extender
$runtime = Resolve-Path script\book-extension-rs\target\release\book-extender.exe
& $runtime `
  --config config\book-extension-pilot.toml `
  --input C:\book-extension-state\pilot\input-book.db `
  --output C:\book-extension-state\pilot\output-book.db `
  --engine C:\engine\YaneuraOu.exe `
  --nodes 3000000 --multipv 4 `
  --black-target C:\book-extension-state\pilot\peta-shock.db `
  --white-target C:\book-extension-state\pilot\peta-shock.db `
  --corpus-db C:\book-extension-state\pilot\corpus.sqlite `
  --max-runtime-sec 600
```

24時間pilotは同じコマンドの`--max-runtime-sec`を`86400`へ変更する。元DBを直接変更せず、入力コピーと専用state dirを使う。Rust runtimeは設定したworker数、仮想敵先手・後手・general、corpus同時数を使用する。`position startpos moves ...`または任意rootの`position sfen ... moves ...`で履歴を渡し、停止時の探索結果は破棄する。 各USI探索には既定3,600秒のwatchdogがあり、`--usi-search-timeout-sec`で変更できる。timeout時は`stop`、設定済みUSI stop timeout後の強制終了、engine再起動、最大3回再試行を行う。

保存は設定した間隔（本番・pilotは3600秒）で行う。ロック内では定跡snapshotとSQLite task watermarkだけを取得し、全件検証とファイル書き込みはロック外で行う。保存成功後、watermarkまでをbook hash付きcheckpointにする。終了時も最終世代を保存し、3世代backupを維持する。

coverageは管理用Pythonで生成する。

```powershell
$python = 'C:\Users\nodchip\AppData\Local\Programs\Python\Python312\python.exe'
& $python script\manage_book_corpus.py coverage --db C:\book-extension-state\pilot\corpus.sqlite --book C:\book-extension-state\pilot\output-book.db --snapshot-id pilot-24h --output C:\book-extension-state\pilot\coverage.json
```

観測値は正常/除外局数、SQLiteサイズ、unique/occurrence coverage、site別`metric_counter` node数、N、revision、通常探索速度、メモリ、保存時間、停止時間、再開後hashとする。

## Jenkins Freestyle

Build stepはRust Release binaryを直接ラッパーへ渡す。Pythonや`ExtensionScript`は定跡延長ランタイムの起動に使用しない。

```powershell
powershell -ExecutionPolicy Bypass -File script\run_extend_book_mcts_jenkins.ps1 `
  -RuntimeExe C:\book-extension\book-extender.exe `
  -StateDir C:\book-extension-state\pilot `
  --config C:\book-extension\book-extension-pilot.toml `
  --input C:\book-extension-state\pilot\input-book.db `
  --output C:\book-extension-state\pilot\output-book.db `
  --engine C:\engine\YaneuraOu.exe `
  --nodes 3000000 --multipv 4 `
  --black-target C:\book-extension-state\pilot\peta-shock.db `
  --white-target C:\book-extension-state\pilot\peta-shock.db `
  --corpus-db C:\book-extension-state\pilot\corpus.sqlite
```

ラッパーは子だけに`BUILD_ID=dontKillMe`を継承させる。Jenkins Abortでラッパーが終了するとheartbeatが止まり、Rust子プロセスはtimeout後に全USIへ`stop`を送り、中断結果を破棄して最終保存する。通常の手動停止は`script/request_extend_book_stop.ps1 -StateDir C:\book-extension-state\pilot`を使う。

実Jenkins停止ボタン試験は別マシンで行う。Abort後に`[stop] reason=jenkins-heartbeat-expired`が記録され、出力DBをRust validatorで読めること、再開時にtaskを重複適用しないことを確認する。

## 搬送

停止後に bundle を作る。共有 registry は全延長マシンから同じパスに見える必要がある。

```powershell
& $python script\manage_book_corpus.py bundle-create --book C:\book-extension-state\pilot\output-book.db --db C:\book-extension-state\pilot\corpus.sqlite --config config\book-extension-pilot.toml --vulnerability-book C:\book-extension-state\pilot\peta-shock.db --runtime-exe script\book-extension-rs\target\release\book-extender.exe --output C:\transfer\pilot.zip
& $python script\manage_book_corpus.py bundle-verify --bundle C:\transfer\pilot.zip --destination C:\book-extension-work\pilot
& $python script\manage_book_corpus.py bundle-claim --bundle-id '<manifest bundle_id>' --registry \\server\book-extension-claims --machine-id $env:COMPUTERNAME
```

同じ bundle ID の2台目は共有 registry の排他的 claim で拒否される。最新棋譜の追加時は延長を停止し、transactional ingest、coverage snapshot、bundle再作成、再開の順に行う。N は維持し、飽和 counter だけをリセットする。

`peta_shock` はこの一連の処理から自動実行しない。保存済み DB に対し、利用者が任意の時点で手動実行する。

## Rustランタイムの同期規約と検証記録

`script/book-extension-rs/Cargo.lock`をRust依存関係の正本とする。直接依存は`shogi_core 0.1.5`、`shogi_legality_lite 0.1.3`、`shogi_usi_parser 0.1.0`（MIT）、`rusqlite 0.32.1`（MIT）、および`clap 4.6.1`、`serde 1.0.228`、`serde_json 1.0.149`、`sha2 0.10.9`、`tempfile 3.27.0`、`thiserror 2.0.18`、`toml 0.8.23`、`windows-sys 0.61.2`（MIT OR Apache-2.0）である。Windows x64で`cargo build --offline --release --bins`に成功している。

同期順序は次の通りとする。

1. workerは共有state mutexを取得してleafまたはcorpus taskを予約し、直ちに解放する。
2. USI探索中は共有state mutexもSQLite transactionも保持しない。
3. 結果反映時だけ共有state mutexを取得し、その内側で当該SQLite接続を操作する。逆順の取得は禁止する。
4. saverは共有state mutex内でbook snapshotとtask watermarkを取得し、mutex解放後に全件検証とatomic saveを行う。保存後のcheckpointだけ再度mutex内で記録する。
5. stop monitorはstop admissionとresult discardをmutex内で設定してから、mutex外でUSI `stop`と必要時の強制終了を行う。

旧Pythonランタイム`script/extend_book_mcts.py`は差分試験用の仕様オラクルとしてdeprecated扱いで保持する。通常入口`script/run_book_extension.py`は`--runtime-exe`で指定されたRust実行ファイルを子プロセスとして起動するだけで、Python MCTSやworkerを実行しない。通常入口はheartbeatを明示指定しない限りJenkins heartbeat監視を有効にせず、`state_dir/stop.request`による手動停止だけを常時監視する。

固定fixtureのPython/Rust差分試験は次で実行する。parse/rewrite、ignore-ply、distinct合法手数、UCB、leaf path、minimax、履歴付きposition、固定seed、SQLite予約順・状態・attempt・Nを比較する。

```powershell
cargo build --manifest-path script\book-extension-rs\Cargo.toml --release --bin compat-probe
& $python script\compare_book_extension_runtimes.py --rust-probe script\book-extension-rs\target\release\compat-probe.exe
```

2026-07-12に`user_book1.2026-07-05`（848,543局面、2,161,793登録行、strict issue 0）を同一機で測定した。Rust Release validatorは3回で2.334、2.309、2.309秒、Pythonの読み込みと全件検証は30.932秒（読み込み4.984秒）で、Rustは約13.4倍高速だった。既存4.53 GB pilot SQLiteは複合index追加前のため末尾側局面queryが1.845～5.236秒だった。SQLite backup APIで作った一時コピー（元DBは未変更）をRustで開くと、schema v4を維持したまま1,183,262候補へ`candidate_position_priority_idx(position_key, active, priority_key DESC, id)`を約7.1秒で作成できた。`EXPLAIN QUERY PLAN`は同indexを`position_key=? AND active=?`で使用し、同じ末尾側局面の1,000回測定はmedian 27.3 μs、p95 28.6 μsだった。

実エンジンsmokeは`user_book1.2026-07-05`、`source/tanuki-.exe`、1 engine、1 thread、10,000 nodes、MultiPV 4、固定seed 42、1探索で行った。終了コード0、wall 9.472秒、Rust CPU 7.703秒（入力検証・最終保存検証を含む）、観測できたengine CPU 1.297秒、探索1回、追加局面1だった。探索11 ms、worker join 117 ms、atomic save・検証4,533 ms、最終save/checkpoint待ち5,049 ms、ランタイム内合計6,536 msとログへ出力した。出力は848,544局面、2,161,796行、issue 0で再読込でき、同出力を入力にした再開smokeも終了コード0、探索1回、strict issue 0だった。fake engine試験ではmanual stop、heartbeat timeout、runtime limitの各経路が探索結果を破棄し、unresponsive engineをUSI stop timeout後に強制終了する。 さらに2 workerへ各500 msの独立leaf探索を与えた統合試験はwall 0.56秒で完了し、約1秒の直列実行にならないことから、USI探索中にglobal state lockを保持していないことを確認した。

ローカル停止と再開は次の順に行う。

```powershell
powershell -ExecutionPolicy Bypass -File script\request_extend_book_stop.ps1 -StateDir C:\book-extension-state\pilot
# [stop]、最終保存、Rust validator成功を確認してから、同じ起動コマンドの--inputを保存済みoutput DBへ向ける。
```

Jenkins実機Abort試験では、Abort時刻、heartbeat最終更新、`jenkins-heartbeat-expired`検出、USI stop完了、worker join、最終保存、checkpoint、プロセス終了までを記録する。実機試験後はRust validator、SQLite task重複、再開後hashを確認する。この試験だけはJenkinsがある別マシンで行う。
