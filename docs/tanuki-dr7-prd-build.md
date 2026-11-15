# お紅さん狸 Windows 版のビルド

MSYS2 CLANG64 の `clang++` と `make`、Python 3 を PATH に設定し、リポジトリ直下の PowerShell で実行する。ZEN6 のビルドには `-march=znver6` に対応したコンパイラが必要で、Clang 23.1.5 でビルドを確認している。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File script/build-dr7-release.ps1 `
  -EvalPath 'C:\path\to\nn.bin' `
  -ProgressPath 'C:\path\to\progress.bin'
```

既定では `build/tanuki-dr7_prd` に実行ファイルと評価関数を作り、`build/tanuki-dr7_prd-win64.zip` にまとめる。`-OutputRoot` で出力先を変更できる。既存の出力がある場合は上書きせず終了する。毎回別の作業フォルダーを `build` の下に作り、CPU ごとにオブジェクトとビルドログを分ける。

検証は、実行する PC が対応するターゲットだけを指定する。例えば AVX VNNI に対応している場合は次のようにする。

```powershell
python script/test-dr7-release.py build/tanuki-dr7_prd sse2 ssse3 sse41 sse42 avx2 avxvnni
```

すべてのエンジンは公開コミット `c1b80eaa09fe13d5f12b1599d1ae4d53c224de30` を基点にビルドする。
