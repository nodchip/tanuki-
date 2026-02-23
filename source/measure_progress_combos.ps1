param(
    [string]$EvalFile = "eval/nn.bin",
    [string]$OutputTsv = "c:\home\nodchip\hakubishin-private\source\tmp_progress_combo_results.tsv"
)

$ErrorActionPreference = "Stop"

Set-Location "c:\home\nodchip\hakubishin-private\source"

if (!(Test-Path ".\Makefile")) {
    throw "Makefile が見つかりません。"
}

$combos = @(
    # numeric_mode: 0=double, 1=float, 2=Q0.15, 3=Q16.16(int32)
    @{ n = 0; d = 0; t = 0 }, @{ n = 0; d = 0; t = 1 }, @{ n = 0; d = 0; t = 2 },
    @{ n = 0; d = 1; t = 0 }, @{ n = 0; d = 1; t = 1 }, @{ n = 0; d = 1; t = 2 },
    @{ n = 1; d = 0; t = 0 }, @{ n = 1; d = 0; t = 1 }, @{ n = 1; d = 0; t = 2 },
    @{ n = 1; d = 1; t = 0 }, @{ n = 1; d = 1; t = 1 }, @{ n = 1; d = 1; t = 2 },
    @{ n = 2; d = 0; t = 0 }, @{ n = 2; d = 0; t = 1 }, @{ n = 2; d = 0; t = 2 },
    @{ n = 2; d = 1; t = 0 }, @{ n = 2; d = 1; t = 1 }, @{ n = 2; d = 1; t = 2 },
    @{ n = 3; d = 0; t = 0 }, @{ n = 3; d = 0; t = 1 }, @{ n = 3; d = 0; t = 2 },
    @{ n = 3; d = 1; t = 0 }, @{ n = 3; d = 1; t = 1 }, @{ n = 3; d = 1; t = 2 }
)

if (!(Test-Path $OutputTsv)) {
    "numeric_mode`tdiff`ttable_mode`trun`tnps" | Out-File -FilePath $OutputTsv -Encoding utf8
}

$totalCombos = $combos.Count
$comboIndex = 0

foreach ($c in $combos) {
    $comboIndex++
    $label = "n=$($c.n), d=$($c.d), t=$($c.t)"

    Write-Host ""
    Write-Host ("[{0:HH:mm:ss}] [{1}/{2}] 開始: {3}" -f (Get-Date), $comboIndex, $totalCombos, $label)

    make clean | Out-Null
    $extra = "-DTANUKI_PROGRESS_NUMERIC_MODE=$($c.n) -DTANUKI_PROGRESS_USE_DIFF=$($c.d) -DTANUKI_PROGRESS_TABLE_MODE=$($c.t)"
    make normal EVALFILE=$EvalFile EXTRA_CPPFLAGS="$extra" | Out-Null

    if (!(Test-Path ".\YaneuraOu-by-gcc.exe")) {
        throw "ビルド後に YaneuraOu-by-gcc.exe が見つかりません。 combo=$label"
    }

    for ($r = 1; $r -le 5; $r++) {
        Write-Host ("[{0:HH:mm:ss}] [{1}/{2}] Run {3}/5 実行中..." -f (Get-Date), $comboIndex, $totalCombos, $r)

        $log = ".\tmp_combo_n$($c.n)_d$($c.d)_t$($c.t)_run$($r).txt"
        "bench 1024 23 15000 default movetime`n" | & ".\YaneuraOu-by-gcc.exe" *> $log

        $line = Select-String -Path $log -Pattern "Nodes/second" | Select-Object -Last 1
        if (-not $line) {
            throw "Nodes/second が見つかりません。 log=$log"
        }
        if ($line.Line -notmatch "([0-9,]+)") {
            throw "Nodes/second の解析に失敗しました。 log=$log"
        }

        $nps = ($matches[1] -replace ",", "")
        "$($c.n)`t$($c.d)`t$($c.t)`t$r`t$nps" | Add-Content -Path $OutputTsv -Encoding utf8

        Write-Host ("[{0:HH:mm:ss}] [{1}/{2}] Run {3}/5 完了: NPS={4}" -f (Get-Date), $comboIndex, $totalCombos, $r, $nps)
    }

    Write-Host ("[{0:HH:mm:ss}] [{1}/{2}] 完了: {3}" -f (Get-Date), $comboIndex, $totalCombos, $label)
}

Write-Host ""
Write-Host ("[{0:HH:mm:ss}] 全組み合わせの測定が完了しました。" -f (Get-Date))
