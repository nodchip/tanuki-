param(
    [string]$InputPath = "C:\Users\nodchip\Downloads\new_petabook_20260423_10279729\user_book1.db",
    [string]$OutputPath = "C:\Users\nodchip\Downloads\new_petabook_20260423_10279729\user_book1.packed_sfen_value.bin",
    [double]$ProgressIntervalSec = 5.0,
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SourcePath = Join-Path $ScriptDir "convert_yaneuraou_book_to_packed_sfen.cpp"
$ExePath = Join-Path $ScriptDir "convert_yaneuraou_book_to_packed_sfen.exe"
$GxxPath = "C:\msys64\ucrt64\bin\g++.exe"

if (-not (Test-Path -LiteralPath $InputPath)) {
    throw "Input file not found: $InputPath"
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

$OutputDir = Split-Path -Parent $OutputPath
if ($OutputDir -and -not (Test-Path -LiteralPath $OutputDir)) {
    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
}

& $ExePath $InputPath $OutputPath --progress-interval $ProgressIntervalSec
if ($LASTEXITCODE -ne 0) {
    throw "Conversion failed."
}

$item = Get-Item -LiteralPath $OutputPath
Write-Host ("output={0} bytes={1}" -f $item.FullName, $item.Length)
