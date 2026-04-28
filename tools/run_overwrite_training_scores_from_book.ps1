param(
    [Parameter(Mandatory = $true)]
    [string]$InputTrainingPath,

    [Parameter(Mandatory = $true)]
    [string]$BookPath,

    [Parameter(Mandatory = $true)]
    [string]$OutputTrainingPath,

    [double]$ProgressIntervalSec = 5.0,
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SourcePath = Join-Path $ScriptDir "convert_yaneuraou_book_to_packed_sfen.cpp"
$ExePath = Join-Path $ScriptDir "convert_yaneuraou_book_to_packed_sfen.exe"
$GxxPath = "C:\msys64\ucrt64\bin\g++.exe"

if (-not (Test-Path -LiteralPath $InputTrainingPath)) {
    throw "Input training file not found: $InputTrainingPath"
}
if (-not (Test-Path -LiteralPath $BookPath)) {
    throw "Book file not found: $BookPath"
}
if (-not (Test-Path -LiteralPath $GxxPath)) {
    throw "g++ not found: $GxxPath"
}

if ($Rebuild -or -not (Test-Path -LiteralPath $ExePath) -or
    ((Get-Item -LiteralPath $SourcePath).LastWriteTime -gt (Get-Item -LiteralPath $ExePath -ErrorAction SilentlyContinue).LastWriteTime)) {
    & $GxxPath -O3 -DNDEBUG -std=c++17 -Wall -Wextra -pedantic -o $ExePath $SourcePath
    if ($LASTEXITCODE -ne 0) {
        throw "Build failed: $SourcePath"
    }
}

$OutputDir = Split-Path -Parent $OutputTrainingPath
if ($OutputDir -and -not (Test-Path -LiteralPath $OutputDir)) {
    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
}

& $ExePath overwrite-scores $InputTrainingPath $BookPath $OutputTrainingPath --progress-interval $ProgressIntervalSec
if ($LASTEXITCODE -ne 0) {
    throw "Score overwrite failed."
}

$item = Get-Item -LiteralPath $OutputTrainingPath
Write-Host ("output={0} bytes={1}" -f $item.FullName, $item.Length)
