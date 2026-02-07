param(
    [string]$EnginePath = "source/YaneuraOu-by-gcc.exe",
    [string]$EvalDir = "../eval",
    [string]$OutFile = "eval/nnue-parity-cases.jsonl",
    [string]$SfenListFile = "",
    [string]$FailureLogDir = "eval/parity-failures",
    [switch]$KeepTempOnError
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-DefaultSfens {
    return @(
        "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1",
        "lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2",
        "lnsgkgsnl/1r5b1/pp1pppppp/2p6/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL b - 3",
        "lnsgkgsnl/1r5b1/pp1pppppp/2p6/9/2P1P4/PP1P1PPPP/1B5R1/LNSGKGSNL w - 4",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/2p6/2P1P4/PP1P1PPPP/1B5R1/LNSGKGSNL b - 5",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/2p6/2P1P4/PP1P1PPPP/1B3R3/LNSGKGSNL w - 6",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/2p6/2P1P4/PP1P1PPPP/1B3R3/LNSGKGSNL b - 7",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/2p6/2P1P4/PP1P1PPPP/1B3R3/LNSGKGSNL w - 8",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/9/2p1P4/PP1P1PPPP/1B3R3/LNSGKGSNL b - 9",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/9/2p1P4/PP1P1PPPP/1B3R3/LNSGKGSNL w - 10"
    )
}

function Save-FailureLog(
    [string]$failureLogDir,
    [string]$sfen,
    [string]$inputText,
    [string]$stdoutText,
    [string]$stderrText,
    [int]$exitCode,
    [string]$reason
) {
    if (-not (Test-Path $failureLogDir)) {
        New-Item -ItemType Directory -Path $failureLogDir | Out-Null
    }

    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
    $id = [guid]::NewGuid().ToString("N").Substring(0, 8)
    $basePath = Join-Path $failureLogDir ("parity-failure-{0}-{1}" -f $timestamp, $id)

    $meta = @{
        reason = $reason
        exitCode = $exitCode
        sfen = $sfen
        generatedAt = (Get-Date).ToString("o")
    } | ConvertTo-Json -Depth 4

    Set-Content -Encoding UTF8 ($basePath + ".meta.json") $meta
    Set-Content -Encoding UTF8 ($basePath + ".usi.txt") $inputText
    Set-Content -Encoding UTF8 ($basePath + ".stdout.txt") $stdoutText
    Set-Content -Encoding UTF8 ($basePath + ".stderr.txt") $stderrText
    return $basePath
}

function Invoke-Eval(
    [string]$enginePath,
    [string]$evalDir,
    [string]$sfen,
    [string]$failureLogDir,
    [switch]$keepTempOnError
) {
    $tmpIn = Join-Path $env:TEMP ("yane_cmd_" + [guid]::NewGuid().ToString("N") + ".txt")
    $tmpOut = Join-Path $env:TEMP ("yane_out_" + [guid]::NewGuid().ToString("N") + ".txt")
    $tmpErr = Join-Path $env:TEMP ("yane_err_" + [guid]::NewGuid().ToString("N") + ".txt")
    $keepTemp = $false
    $inputText = @(
        "setoption name EvalDir value $evalDir",
        "isready",
        "position sfen $sfen",
        "e",
        "quit"
    ) -join [Environment]::NewLine

    try {
        $inputText | Set-Content -Encoding ascii $tmpIn

        $process = Start-Process -FilePath $enginePath -RedirectStandardInput $tmpIn -RedirectStandardOutput $tmpOut -RedirectStandardError $tmpErr -NoNewWindow -PassThru -Wait
        $stdoutText = if (Test-Path $tmpOut) { (Get-Content $tmpOut -Raw) } else { "" }
        $stderrText = if (Test-Path $tmpErr) { (Get-Content $tmpErr -Raw) } else { "" }

        if ($process.ExitCode -ne 0) {
            $logBase = Save-FailureLog -failureLogDir $failureLogDir -sfen $sfen -inputText $inputText -stdoutText $stdoutText -stderrText $stderrText -exitCode $process.ExitCode -reason "non-zero-exit"
            if ($keepTempOnError) { $keepTemp = $true }
            throw "エンジン終了コードが非0です: $($process.ExitCode)`nlog: $logBase"
        }

        $outLines = if (Test-Path $tmpOut) { Get-Content $tmpOut } else { @() }
        $line = $outLines | Where-Object { $_ -match "^eval = " } | Select-Object -Last 1
        if (-not $line) {
            $logBase = Save-FailureLog -failureLogDir $failureLogDir -sfen $sfen -inputText $inputText -stdoutText $stdoutText -stderrText $stderrText -exitCode $process.ExitCode -reason "missing-eval-line"
            if ($keepTempOnError) { $keepTemp = $true }
            throw "評価値を取得できませんでした。SFEN: $sfen`nlog: $logBase"
        }

        $scoreText = $line -replace "^eval =\s*", ""
        try {
            return [int]$scoreText
        }
        catch {
            $logBase = Save-FailureLog -failureLogDir $failureLogDir -sfen $sfen -inputText $inputText -stdoutText $stdoutText -stderrText $stderrText -exitCode $process.ExitCode -reason "invalid-eval-format"
            if ($keepTempOnError) { $keepTemp = $true }
            throw "評価値の形式が不正です: '$scoreText'`nlog: $logBase"
        }
    }
    finally {
        if (-not $keepTemp) {
            if (Test-Path $tmpIn) { Remove-Item $tmpIn -Force }
            if (Test-Path $tmpOut) { Remove-Item $tmpOut -Force }
            if (Test-Path $tmpErr) { Remove-Item $tmpErr -Force }
        }
    }
}

if (-not (Test-Path $EnginePath)) {
    throw "EnginePath が見つかりません: $EnginePath"
}

$sfens = if ($SfenListFile -and (Test-Path $SfenListFile)) {
    [System.IO.File]::ReadAllLines((Resolve-Path $SfenListFile), [System.Text.Encoding]::UTF8) |
        ForEach-Object { $_.Trim() } |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}
else {
    Get-DefaultSfens
}

$records = New-Object System.Collections.Generic.List[string]
foreach ($sfen in $sfens) {
    $score = Invoke-Eval -enginePath $EnginePath -evalDir $EvalDir -sfen $sfen -failureLogDir $FailureLogDir -keepTempOnError:$KeepTempOnError
    $json = @{ sfen = $sfen; expectedScore = $score } | ConvertTo-Json -Compress
    $records.Add($json)
    Write-Host "ok: $sfen => $score"
}

$dir = Split-Path -Parent $OutFile
if ($dir -and -not (Test-Path $dir)) {
    New-Item -ItemType Directory -Path $dir | Out-Null
}

$records | Set-Content -Encoding UTF8 $OutFile
Write-Host "generated: $OutFile ($($records.Count) cases)"
