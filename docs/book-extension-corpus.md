# 棋譜 corpus を利用した定跡 DB 延長

## 構成

- やねうら王形式の定跡 DB は探索中の data plane とし、メモリ上で更新して約1時間ごとに原子的保存する。バックアップは3世代で、`peta_shock` は保存後に手動実行する。
- SQLite は corpus、優先度、探索 task、checkpoint、N、revision、計測値を持つ control plane とする。SQLite 操作とメモリ上の定跡 DB 操作は同じ `book_lock` 内、USI 探索はロック外で行う。
- 通常 MultiPV で `min(MultiPV, 合法手数)` を distinct かつ合法な手で満たした後、MCTS が実際に訪問した経路上だけで corpus 手を `searchmoves` 探索する。仮評価値は登録しない。
- 通常探索と corpus 探索は `position startpos moves ...` または任意 root の `position sfen ... moves ...` を使う。引き分け評価0と既存 UCB は変更しない。

## 1コマンドでの corpus 作成

定跡延長プログラムを手動停止し、対象の state directory に入力定跡を input-book.db として置いてから、次のどちらか1コマンドを実行する。

最初にRust Release binaryを作成する（依存crate取得済みなら`--offline`でもよい）。

    cargo build --manifest-path script/book-extension-rs/Cargo.toml --release --bin corpus-builder

24時間pilot用:

    & .\script\book-extension-rs\target\release\corpus-builder.exe build --profile pilot --state-dir C:\book-extension-state\pilot --input-book C:\book-extension-state\pilot\input-book.db

本番運用用フルセット:

    & .\script\book-extension-rs\target\release\corpus-builder.exe build --profile production --state-dir C:\book-extension-state\production --input-book C:\book-extension-state\production\input-book.db

古いWCSCのLZHを含むprofileでは、起動前に7-Zipを検証する。標準配置または`PATH`にない場合だけ`--seven-zip C:\path\7z.exe`を追加する。実体パス、version、SHA-256はsummaryへ記録される。固定manifestの通常運用経路はこのRust binaryだけで完結し、Python/cshogiを起動・importしない。`script/prepare_book_corpus.py`は旧結果との比較用reference-onlyであり、本番入口として使用しない。

作成中の進捗は標準エラーへ即時出力され、Jenkinsではコンソールログから確認できる。storage、download、ingest、metadata、coverage、optimize、validation、publishの開始・完了と、長時間処理中の30秒ごとのheartbeatを表示する。downloadはファイル数・バイト数、ingestは500局以下のバッチをトランザクション単位として、各バッチの確定後に処理棋譜数・accepted・excludedも表示する。正常終了時の最終JSONは従来どおり標準出力へ1行だけ出力される。

pilot は取得日時を固定した floodgate 2026、WCSC36、電竜戦6本戦の3アーカイブである。production は公式配布を確認できた floodgate 年度別15本（2011、2012、2014～2026）、WCSC35大会（第1～29回、第31～36回。第30回は中止）、平手開始の電竜戦本戦5大会（第2～6回）、第7回電竜戦TSEC第2部・先手持ち時間0秒戦347棋譜を含む。TSEC7の公式ZIPは指定局面戦と平手戦が混在するため、profileの`member_pattern`で第2部のKIFだけを取り込む。第2部347棋譜のうち341局は合法に取り込め、指し手がない6棋譜は`ingest_error`へ記録してcoverage分母から除外する。manifest上の合計ダウンロード量は約3.55 GiBで、ダウンロード済みファイルはサイズとSHA-256が一致すれば再利用する。古いWCSCのLZH展開には7-Zipが必要である。

実行中の定跡延長が book-extension.lock を保持している場合、および別のcorpus作成が corpus-build.lock を保持している場合は、既存成果物を変更せず終了する。作成は一時SQLite上で行い、検証成功後に corpus.sqlite、snapshot.json、coverage、summaryを配置する。旧 corpus.sqlite は corpus.sqlite.previous に1世代だけ保持する。失敗時はphase別終了コードと corpus-build-summary.json を残す。

入力定跡が別の場所にある場合だけ --input-book D:/path/book.db を追加する。peta_shock と定跡延長ランタイムの起動は自動実行しない。

## 旧 SQLite schema v5 の設計と実測記録

この節はschema v5時点の設計・容量実測を保存する履歴である。現行運用は後述のschema v6 quality frontierを使用する。schema v5では、局面までの累積`history_json`を各局面・候補・出典へ保存しない。`logical_game`が初期SFENと全指し手列を1回だけ保持し、`game_position`は`game_id`、`ply`、`position_id`、実戦手だけを持つ。候補の代表履歴は`representative_game_id + representative_ply`から復元するため、千日手判定に必要な順序付き履歴を失わない。canonical SFENは手数を除いた`position.position_key`へ正規化し、候補と棋譜局面は整数IDで参照する。

schema v5の候補優先度は11要素を88 byteの固定長big-endian BLOBへ符号化した。SQLiteのBLOB辞書順がpriority tuple順と一致し、当時は`candidate_position_priority_idx(position_id, active, priority_key DESC, id)`を局面別上位N検索に使用した。`candidate_source`表は廃止し、出典集合は`game_position -> source_game -> raw_source`から導出する。DDLの正本は`script/corpus_schema.sql`で、Python作成系とRustランタイムが同じファイルを使用する。

schema v5導入時はschema v4以前を自動変更せず、固定manifestからschema v5を再作成した。現行ランタイムはschema v6だけを開き、v5の棋譜データは固定manifestからv6へ再構築する。再構築時にはv5のmutable runtime stateだけを自然キーで新DBへ移送できる。巨大な旧DBのin-place migrationは正式運用にしない。旧DBを比較する場合はread-onlyで次を実行し、logical game、raw source、source-game対応、全棋譜局面、候補集合、優先度、ingest errorの件数とストリーミングSHA-256を比較する。

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

### Rust corpus-builder pilot全件実測（2026-07-15）

更新後の固定pilot manifestをRust Release `corpus-builder.exe`だけで処理した。途中で`stop.request`を2回投入して500局単位checkpointから再開し、既存正本を変更せず終了コード130になることを確認した後、同じfresh stateを最後まで作成した。最終runはwall 1,436.36秒、phase合計1,433.71秒、最大Working Set 106,835,968 bytes（約101.9 MiB）だった。phaseはingest 997.08秒、metadata 183.21秒、coverage 164.14秒、optimize 9.98秒、全件validator 79.21秒である。旧Python実測は18分41.7秒だが、Rust版は安全側parser、全68,089 logical game・7,784,612局面の再生validator、進捗、停止・再開を含むため、wall timeだけの単純比較はしない。メモリは旧Python約1.363 GiBから約92.7%減少した。

3 source、71,445棋譜memberを読み、member filter除外0、accepted 69,353、excluded 2,092だった。error内訳はDecode 114、IllegalMove 4、MissingEndgame 389、NoMoves 1,585である。logical game 68,089、game position 7,784,612、canonical position 5,829,858、candidate 5,947,156、candidate-source occurrence 7,876,895となった。DBは3,638,398,976 bytes、SHA-256は`f6eaa850e6e0120e67fe4381aeab2c8361c643716b7bee5b4d1aa8622a31149cf`、WAL 0 bytes、`integrity_check=ok`、foreign key error 0件、全局面再生件数一致だった。候補lookupは`candidate_position_priority_idx(position_id, active, priority_key DESC, id)`を使用する。

initial coverageはunique 58,632 / 5,829,858（1.005719%）、occurrence 1,316,871 / 7,784,612（16.916334%）だった。7-Zipは`C:\Program Files\7-Zip\7z.exe`、version 26.02、実体SHA-256 `83967f1b02b43c4efeda302795722c809e0ee81b8307de73558d10484d5676a7d`を起動前に検証しsummaryへ保存した。

旧Python schema v5 DBは更新前floodgate snapshot（168,725,129 bytes、SHA-256 `eb9c604c36ee9b94950bf610011fd3b39cf84562b2345ee410d1cc1c56c7680d`）から作られ、現manifestは更新後snapshot（169,801,236 bytes）なので、DB全体の件数・hashは同一入力比較にならない。現在manifestに対するrecord fingerprint差分では、WCSC 295/295件のcoreと終局が一致し、電竜戦459/459件のcoreが一致した。電竜戦の終局32件はRustが`%TIME_UP`、`%KACHI`、`%ILLEGAL_MOVE`を保持するのに対し旧cshogiが`%CHUDAN`へ丸める意図した差である。floodgateは共有68,599 game中core mismatch 0件で、Rustのみ507件をdecode不能、非合法手、終局欠損として安全側に除外した。終局差の主因は、明示`%TORYO`がある異常終了棋譜をcshogiが`%ERROR`へ置き換える点である。
production manifestの圧縮アーカイブは3.547 GiBである。pilotの実測比21.35倍を単純適用した完成DBの中心推定は約75.7 GiB、コードが空き容量検査に使う保守的40倍推定は141.89 GiBである。新規作成ではアーカイブと512 MiB余裕を含め約145.93 GiBの空きを要求する。中心推定では通常のactive + previousが約151.4 GiB、更新中のactive + previous + buildが約227.1 GiBとなる。大会ごとの平均手数・重複率で変動するため、実運用では保守的推定を空き容量判定の正本とする。
### Rust corpus-builder production全件実測（2026-07-15～16）

固定production manifest 56 sourceと`user_book1.2026-07-05`をRust Release `corpus-builder.exe`で処理した。floodgate 2011のtar.xz memberが慣例的な`./` prefixを持つことを初回runで検出し、`./`だけを正規化しつつ絶対path・`..`・colonを引き続き拒否するよう修正・回帰試験した。その後、同じfresh stateのcheckpoint（28,556 record）から再開して正常完了した。monitor wallは45,125.435秒（12時間32分5.435秒）、summaryのphase合計は44,023.773秒（12時間13分43.773秒）、最大Working Setは189,710,336 bytes（約180.9 MiB）だった。download済み56 sourceはsizeとSHA-256一致により全て再利用し、新規downloadは0件だった。

archive内のrecord member 1,571,216件のうちmember filterで394件を除外し、1,570,822 recordを処理した。accepted 1,497,931、excluded 72,891、logical game 1,466,175、game position 174,801,835、canonical position 130,016,499、candidate 132,638,356、candidate-source occurrence 177,779,510である。error内訳はDecode 572、IllegalMove 51、InvalidMove 201、InvalidPositionToken 384、MissingEndgame 50,239、NoMoves 20,866、NonStartpos 326、TimeBeforeFirstMove 41、TurnBeforePosition 211だった。phaseはingest 23,860.995秒、metadata 2,935.369秒、coverage 2,709.400秒、optimize 2,439.052秒、validation 12,076.677秒である。 throughputは全phase基準でrecord 35.681件/秒、game position 3,970.624件/秒、SQLite row 10,041.501件/秒だった。parser/ingest区間だけのmove throughputは7,325.840手/秒である。SQLite rowはANALYZE後の全schema table entry 442,064,750件を合計した。Rust summaryは今後、これらを`records_per_second`、`moves_per_second`、`positions_per_second`、`sqlite_rows_per_second`として自動保存する。

完成DBは`C:\tmp\rust-corpus-production-20260715\corpus.sqlite`、82,832,003,072 bytes（約77.13 GiB）、SHA-256 `9f54059ab4399b5d5d5aeb75866fd6347303143e3fed7ecd674715a55b8770de3`、WAL 0 bytesだった。作成前の保守的DB見積り152,348,437,040 bytesに対し実サイズは約54.4%で、空き容量判定は従来どおり保守的見積りを使用する。`integrity_check=ok`、foreign key error 0件、schema version 5、全1,466,175 logical game・174,801,835局面の再生件数一致を確認した。snapshot SHA-256は`b89514d02f25a940fb767c4c42577005dd34802d58d52291a393e7147302607bc`、coverage SHA-256は`938095621d7482b6badfddb9e5dff1848855807ab820e7fcb69e92282b93873c2`である。

initial coverageはunique 254,055 / 130,016,499（0.195402%）、occurrence 18,159,864 / 174,801,835（10.388829%）だった。site別unique / occurrenceは、電竜戦5.523721% / 16.979304%、floodgate 0.193535% / 10.381359%、WCSC 1.846146% / 8.908554%である。

production DBをread-only immutableで開き、ランタイムの`POSITION_QUERY`と同じjoin、priority順、width 4でlookupを測定した。MCTSの局所訪問を模した連続10局面・warm 100回はp50 36.5 us / p95 46.905 usだった。DB全域へ均等配置した10局面のcold参考値はp50 11.996 ms / p95 14.342 msであり、サンプル数が少ないため容量全域へのランダムI/Oの目安として扱う。`EXPLAIN QUERY PLAN`はposition unique index、`candidate_position_priority_idx`、logical game・raw source・adhoc historyの主キー、search task unique indexを使い、全表走査はなかった。
## 固定 snapshot の収集条件

`script/collect_book_corpus.py` は明示した JSON manifest だけを取得する。manifest の各 source には `site`、`event`、`year`、`retrieved_at`、`url`、`relative_path`、`size`、`sha256` を記録する。巨大データを暗黙に全取得しない。

- floodgate: 完了年度は年度別 `.7z` を全件、進行年度は取得日時を固定した snapshot。rating は棋譜を削る条件にせず、同年度・同 anchor era・同 connected component の信頼できる相対値だけを優先度に使う。
- WCSC: 公式公開の大会・全 stage。到達 stage、指し手側順位、対戦相手順位を ranking JSON から入れる。
- 電竜戦: 大会 ZIP または取得日時固定の個別 CSA/KIF。平手初期局面から始まる棋譜だけを採用し、駒落ちと指定局面開始は除外する。1つの公式アーカイブに対象・対象外の棋譜が混在する場合は、ingestの`member_pattern`をアーカイブ内相対パスへ適用し、対象棋譜だけを解析する。

manifest更新候補の調査・旧Python reference用の収集例（通常のfixed manifest buildでは使用しない）:

```powershell
$python = 'C:\Users\nodchip\AppData\Local\Programs\Python\Python312\python.exe'
& $python script\collect_book_corpus.py --manifest C:\corpus-manifests\pilot.json --destination C:\corpus-data\pilot
```

通常運用では延長を手動停止してから`corpus-builder.exe build`を1回実行する。以下のsite/event/year別Python取り込みは旧版比較・診断専用である。`retrieved-at` は Unix 秒で固定し、壊れた棋譜は `ingest_error` に残りcoverage分母から除外する。

```powershell
& $python script\manage_book_corpus.py ingest --db C:\book-extension-state\pilot\corpus.sqlite --site floodgate --event floodgate-2026-07 --year 2026 --retrieved-at 1783785600 --input C:\corpus-data\pilot\floodgate-2026-07
& $python script\manage_book_corpus.py ingest --db C:\book-extension-state\pilot\corpus.sqlite --site wcsc --event wcsc36 --year 2026 --retrieved-at 1783785600 --input C:\corpus-data\pilot\wcsc36.zip
& $python script\manage_book_corpus.py ingest --db C:\book-extension-state\pilot\corpus.sqlite --site denryu --event denryu6 --year 2025 --retrieved-at 1783785600 --input C:\corpus-data\pilot\denryu6.zip
```

同一棋譜の別 mirror は raw source を両方保持し、完全に同じ対局者・指し手列だけを同一 logical game とする。AI名は Unicode 正規化後の完全一致または明示 alias のみを使う。

## 優先度データ

Rust版`corpus-builder.exe`はprofileに固定されたmetadataを自動投入する。以下の個別Python importは旧版比較・診断専用である。WCSC/電竜戦 ranking JSON は大会 metadata と `official_name, stage, stage_tier, rank, participants` の配列を持つ。floodgate rating JSON は snapshot 品質情報と `player_name, rating, effective_games, connected_to_anchor, anchor_margin, snapshot_percentile` を持つ。

```powershell
& $python script\manage_book_corpus.py ranking-import --db C:\book-extension-state\pilot\corpus.sqlite --json C:\corpus-data\pilot\wcsc36-ranking.json
& $python script\manage_book_corpus.py rating-import --db C:\book-extension-state\pilot\corpus.sqlite --json C:\corpus-data\pilot\floodgate-rating.json --config config\book-extension-pilot.toml
```

3年 bucket を最上位キーとし、bucket 内で大会 stage・指し手側順位・対戦相手順位を比較する。floodgate は anchor 接続と effective games による信頼度を先に比較し、参加者数で rating を直接乗除算しない。site node 予算は WCSC 40%、電竜戦40%、floodgate20%で、空 queue の枠は実消費 node に基づき再配分される。

## Rust定跡延長ランタイムとpilot

長時間稼働する定跡延長、MCTS、USIプロセス管理、corpus lane、保存・停止・再開はRust版`book-extender.exe`が担当する。固定manifestの取得、archive列挙、CSA/KIF解析、取り込み、ranking/rating、initial coverage、検証、公開はRust版`corpus-builder.exe`が担当する。Python/cshogiは通常運用に使わず、manifest更新候補の調査、旧版差分、可視化など本番経路外の補助だけに残す。

```powershell
cargo build --manifest-path script\book-extension-rs\Cargo.toml --release --bin book-extender --bin book-sqlite
$runtime = Resolve-Path script\book-extension-rs\target\release\book-extender.exe
$bookSqlite = Resolve-Path script\book-extension-rs\target\release\book-sqlite.exe
$bookDb = 'C:\book-extension-state\pilot\opening-book.sqlite'
& $bookSqlite import --database $bookDb --input C:\book-extension-state\pilot\peta-shock.db --allow-unsorted
& $bookSqlite import --database $bookDb --input C:\book-extension-state\pilot\tanuki-.2026-07-24.2.db --allow-unsorted
& $runtime `
  --config config\book-extension-pilot.toml `
  --database $bookDb `
  --output C:\book-extension-state\pilot\output-book.db `
  --engine C:\engine\YaneuraOu.exe `
  --nodes 3000000 --multipv 4 `
  --min-eval-cp=-200 `
  --corpus-db C:\book-extension-state\pilot\corpus.sqlite `
  --max-runtime-sec 600
```

24時間pilotは同じコマンドの`--max-runtime-sec`を`86400`へ変更する。SQLiteを定跡の正本とし、`--output`のやねうら王形式はエンジン配布・検証用のexport成果物とする。変換済みSQLiteだけから起動する場合、`--database`は必須で`--input`は省略する。指定したSQLiteが存在しない、必要なテーブルがない、または局面が0件の場合は、新しいDBを作らず起動を拒否する。従来どおりテキスト定跡を起動時に取り込む場合だけ`--input <book>`を指定でき、その場合はSQLiteの新規作成も許可する。Rust runtimeは設定したworker数、先手固定・後手固定・general、corpus同時数を使用する。固定側の手番では登録済み指し手の最大評価値を選び、反対側では`--min-eval-cp`の絶対下限を適用する。互換用の`--eval-diff`も併用でき、両方を指定した場合は厳しい方の下限を使う。`position startpos moves ...`または任意rootの`position sfen ... moves ...`で履歴を渡し、停止時の探索結果は破棄する。 各USI探索には既定3,600秒のwatchdogがあり、`--usi-search-timeout-sec`で変更できる。timeout時は`stop`、設定済みUSI stop timeout後の強制終了、engine再起動、最大3回再試行を行う。

`book-sqlite import`は入力をストリーミングし、一定局面数ごとのtransactionで処理する。既存の`(局面, 指し手)`行は上書きせず、未登録局面と未登録指し手だけを追加するため、新ペタショック定跡の更新版を同じSQLiteへ順次取り込める。`book-extender --import-book <book>`を複数指定して起動時に同じ処理を行うこともできる。探索結果は同じ行の評価値、深さ、応手、visitsを更新する。`--black-target`と`--white-target`は旧コマンドラインとの互換性のため受理するが、敵対探索は行わず、指定ファイルを同じ追加専用規則で取り込む。

保存は設定した間隔（本番・pilotは3600秒）で行う。ロック内ではSQLiteパス、世代、task watermarkだけを取得し、別のread-only接続による一貫したsnapshotをストリームexportする。全件検証とファイル書き込みはロック外で行うため、定跡全体のメモリcloneは発生しない。保存成功後、watermarkまでをbook hash付きcheckpointにする。終了時も最終世代を保存し、3世代backupを維持する。明示的なexportだけを行う場合は`book-sqlite export --database <db> --output <book.db>`を使う。

初期coverageは`corpus-builder.exe build`が生成し、全成果物検証後に`coverage-initial.json`として公開する。以下のPythonコマンドは旧版との比較・調査専用であり、固定manifestの本番作成経路には含めない。

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
  -ProgressIntervalSec 60 `
  -LogRetentionCount 5 `
  --config C:\book-extension\book-extension-pilot.toml `
  --database C:\book-extension-state\pilot\opening-book.sqlite `
  --output C:\book-extension-state\pilot\output-book.db `
  --engine C:\engine\YaneuraOu.exe `
  --nodes 3000000 --multipv 4 `
  --min-eval-cp=-200 `
  --corpus-db C:\book-extension-state\pilot\corpus.sqlite
```

ラッパーはRustランタイムの標準出力・標準エラーをJenkinsコンソールへ継続的に中継する。同じ内容は`StateDir\logs\book-extender-<run-id>.stdout.log`と`StateDir\logs\book-extender-<run-id>.stderr.log`へ保存し、既定では実行中を含む直近5回分を保持する。`ProgressIntervalSec`の既定値は60秒、`LogRetentionCount`の既定値は5である。

Jenkinsコンソールにはランタイムの探索イベントに加え、60秒ごとに次のような要約を出す。

```text
[progress] run_id=... uptime_sec=... searches=... added_positions=... total_nodes=... active_normal=... active_corpus=... band=... n=... corpus_evaluated=... corpus_added=... last_save_age_sec=...
```

`runtime-status.json`が未作成、別runのもの、更新停止、またはatomic置換の瞬間で読み取れない場合、ラッパーは停止せず`[progress-warning]`を出して次回再試行する。Rust側のstatus書き込みも一時的な失敗では探索を停止せず、`[runtime-status] event=write-failed`の後に成功すると`event=recovered`を記録する。statusには`run_id`、PID、開始・更新時刻、総探索回数、追加局面数、総nodes、lane別探索回数・実行中数、corpus実行中数、直近探索、frontier、task件数、直近保存結果を含む。

Jenkins外から確認する場合は、実行中runのログファイルを追尾する。`runtime-status.json`はatomic置換されるため、`Get-Content -Wait`ではなく必要な時点で読み直す。

```powershell
Get-Content -Wait C:\book-extension-state\pilot\logs\book-extender-<run-id>.stderr.log
Get-Content -Raw C:\book-extension-state\pilot\runtime-status.json | ConvertFrom-Json | Format-List
```

ラッパーはRust子プロセスだけに`BUILD_ID=dontKillMe`を継承させる。heartbeatはログ中継と独立したhelperプロセスが更新し、Jenkinsコンソールが一時的に詰まっても更新を継続する。ラッパーとhelperは通常のJenkins cookieを維持するため、Jenkins Abortでは両方が終了してheartbeatが止まる。Rust子プロセスはtimeout後に全USIへ`stop`を送り、中断結果を破棄する。USIが停止timeout後に強制終了され、パイプ切断を返した場合も停止中の想定エラーとして扱い、最終保存を完了する。通常の手動停止は`script/request_extend_book_stop.ps1 -StateDir C:\book-extension-state\pilot`を使う。

実Jenkins停止ボタン試験は別マシンで行う。Abort後に`[stop] reason=jenkins-heartbeat-expired`が永続ログへ記録され、出力DBをRust validatorで読めること、再開時にtaskを重複適用しないことを確認する。

## 搬送

停止後に bundle を作る。共有 registry は全延長マシンから同じパスに見える必要がある。

```powershell
& $python script\manage_book_corpus.py bundle-create --book C:\book-extension-state\pilot\output-book.db --db C:\book-extension-state\pilot\corpus.sqlite --config config\book-extension-pilot.toml --runtime-exe script\book-extension-rs\target\release\book-extender.exe --output C:\transfer\pilot.zip
& $python script\manage_book_corpus.py bundle-verify --bundle C:\transfer\pilot.zip --destination C:\book-extension-work\pilot
& $python script\manage_book_corpus.py bundle-claim --bundle-id '<manifest bundle_id>' --registry \\server\book-extension-claims --machine-id $env:COMPUTERNAME
```

同じ bundle ID の2台目は共有 registry の排他的 claim で拒否される。最新棋譜の追加時は延長を停止し、transactional ingest、coverage snapshot、bundle再作成、再開の順に行う。N は維持し、飽和 counter だけをリセットする。

搬送先ではbundle内のやねうら王形式bookを`book-sqlite import`で新しいSQLite正本へ取り込む。評価値、深さ、応手、visitsはexportに含まれるため探索状態を復元できる。

`peta_shock` はこの一連の処理から自動実行しない。保存済み DB に対し、利用者が任意の時点で手動実行する。

## Rustランタイムの同期規約と検証記録

`script/book-extension-rs/Cargo.lock`をRust依存関係の正本とする。将棋・DBの直接依存は`shogi_core 0.1.5`、`shogi_legality_lite 0.1.3`、`shogi_usi_parser 0.1.0`、`rusqlite 0.32.1`（MIT）である。pipeline追加分は`clap 4.6.1`、`ctrlc 3.5.2`、`encoding_rs 0.8.35`、`regex 1.13.0`、`serde 1.0.228`、`serde_json 1.0.149`、`sha2 0.10.9`、`tar 0.4.44`、`tempfile 3.27.0`、`thiserror 2.0.18`、`toml 0.8.23`、`unicode-normalization 0.1.25`、`ureq 3.3.0`、`windows-sys 0.61.2`、`xz2 0.1.7`（MIT / Apache-2.0系）、`zip 8.6.0`（MIT）である。`encoding_rs`はApache-2.0 OR MITにBSD-3-Clauseを併用する。古いWCSC LZHだけは外部7-Zip 26.02を明示的な事前要件とし、実体versionとSHA-256を検証する。Windows x64で`cargo build --offline --release --bins`に成功している。

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

## schema v6 quality frontier 運用

schema v6では候補に`quality_band`、`source_site`、`recent_occurrences`、`occurrences`を持たせ、ランタイムは`active_quality_band=0`、`progressive_width=1`から開始する。候補検索は`candidate_position_site_quality_idx`を使い、band・site条件を`LIMIT N+1`より前に適用する。v5は巨大なin-place migrationを行わず、固定manifestからv6を再構築する。

通常再起動ではband、N、適格miss、task、site実績を維持する。manifest、profile、基準年、policy、順位またはrating metadataが変わるcorpus再構築では、自然キー`(position_key, move, book_snapshot_id)`でmutable runtime stateを移し、Nと完了済みtaskを維持しつつ、bandとmiss観測をリセットする。`corpus-build-summary.json`の`runtime_state_migration`で`source_tasks = mapped_tasks + unmapped_tasks`、状態別件数、`copied_checkpoints`、`copied_frontier_history`を確認する。

運用手順は次の通り。

1. `book-extender`を手動、またはJenkinsから停止する。
2. プロセス終了、最終定跡DB保存、`runtime-status.json`の最終更新を確認する。
3. 既存のpilot用またはproduction用1コマンドでcorpusを再構築する。
4. `runtime_state_migration`のmapped、unmapped、status件数を確認する。
5. 新しく公開された`corpus.sqlite`を指定して`book-extender`を再開する。
6. `runtime-status.json`と`frontier_change`、`corpus_reserve`、`corpus_complete`、`corpus_failure`、`runtime_stop`、`book_save`イベントを確認する。
7. 必要な場合だけ、保存済み定跡DBへ`peta_shock`を手動実行する。

`peta_shock`は自動実行しない。24時間pilotと実Jenkins停止ボタン試験は、4エンジン4時間pilot合格後に別マシンで行う後続ゲートである。
