param(
    [string]$EnginePath = "source/YaneuraOu-by-gcc.exe",
    [string]$EvalDir = "../eval",
    [string]$OutFile = "eval/nnue-parity-cases.jsonl",
    [string]$SfenListFile = ""
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
        "lnsgkgsnl/1r5b1/pp1pppppp/9/9/2p1P4/PP1P1PPPP/1B3R3/LNSGKGSNL w - 10",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/9/2p1P4/PP1P1PPPP/1B3R3/LNSGKGSNL b - 11",
        "lnsgkgsnl/1r5b1/pp1pppppp/9/9/2p1P4/PP1P1PPPP/1B3R3/LNSGKGSNL w - 12"
    )
}

function Invoke-Eval([string]$enginePath, [string]$evalDir, [string]$sfen) {
    $tmpIn = Join-Path $env:TEMP ("yane_cmd_" + [guid]::NewGuid().ToString("N") + ".txt")
    $tmpOut = Join-Path $env:TEMP ("yane_out_" + [guid]::NewGuid().ToString("N") + ".txt")
    $tmpErr = Join-Path $env:TEMP ("yane_err_" + [guid]::NewGuid().ToString("N") + ".txt")

    try {
        @(
            "setoption name EvalDir value $evalDir",
            "isready",
            "position sfen $sfen",
            "e",
            "quit"
        ) | Set-Content -Encoding ascii $tmpIn

        $process = Start-Process -FilePath $enginePath -RedirectStandardInput $tmpIn -RedirectStandardOutput $tmpOut -RedirectStandardError $tmpErr -NoNewWindow -PassThru -Wait
        if ($process.ExitCode -ne 0) {
            $stderr = if (Test-Path $tmpErr) { (Get-Content $tmpErr -Raw) } else { "" }
            throw "エンジン終了コードが非0です: $($process.ExitCode)`n$stderr"
        }

        $outLines = if (Test-Path $tmpOut) { Get-Content $tmpOut } else { @() }
        $line = $outLines | Where-Object { $_ -match "^eval = " } | Select-Object -Last 1
        if (-not $line) {
            throw "評価値を取得できませんでした。SFEN: $sfen"
        }

        $scoreText = $line -replace "^eval =\s*", ""
        return [int]$scoreText
    }
    finally {
        if (Test-Path $tmpIn) { Remove-Item $tmpIn -Force }
        if (Test-Path $tmpOut) { Remove-Item $tmpOut -Force }
        if (Test-Path $tmpErr) { Remove-Item $tmpErr -Force }
    }
}

if (-not (Test-Path $EnginePath)) {
    throw "EnginePath が見つかりません: $EnginePath"
}

$sfens = if ($SfenListFile -and (Test-Path $SfenListFile)) {
    Get-Content $SfenListFile | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}
else {
    Get-DefaultSfens
}

$records = New-Object System.Collections.Generic.List[string]
foreach ($sfen in $sfens) {
    $score = Invoke-Eval -enginePath $EnginePath -evalDir $EvalDir -sfen $sfen
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
