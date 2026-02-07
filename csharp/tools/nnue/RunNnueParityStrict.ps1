param(
    [string]$EnginePath = "source/YaneuraOu-by-gcc.exe",
    [string]$EvalDir = "../eval",
    [string]$CasesOutFile = "eval/nnue-parity-cases.jsonl",
    [string]$SfenListFile = "",
    [int]$GenerateSfenCount = 600,
    [int]$GenerateSfenSeed = 20260207,
    [int]$GenerateSfenMinPlies = 8,
    [int]$GenerateSfenMaxPlies = 40
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$minimumStrictCases = 500
$minimumCoverageEach = 1
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..\..\..")
$generator = Join-Path $scriptDir "GenerateNnueParityCases.ps1"

if (-not (Test-Path $generator)) {
    throw "GenerateNnueParityCases.ps1 が見つかりません: $generator"
}

Push-Location $repoRoot
try {
    if (-not (Test-Path $EnginePath)) {
        throw "EnginePath が見つかりません: $EnginePath"
    }

    $evalDirPath = Resolve-Path $EvalDir -ErrorAction SilentlyContinue
    if (-not $evalDirPath) {
        throw "EvalDir が見つかりません: $EvalDir"
    }

    if (-not [string]::IsNullOrWhiteSpace($SfenListFile) -and -not (Test-Path $SfenListFile)) {
        throw "SfenListFile が見つかりません: $SfenListFile"
    }

    if ($GenerateSfenCount -gt 0) {
        $sfenOut = if ([string]::IsNullOrWhiteSpace($SfenListFile)) { "eval/nnue-parity-sfens.txt" } else { $SfenListFile }
        Write-Host "[0/4] parity用SFENを生成します.."
        $parityOutput = dotnet run --project csharp/src/YaneuraOu.CSharp.Engine -- parity-sfen $GenerateSfenCount $GenerateSfenSeed $GenerateSfenMinPlies $GenerateSfenMaxPlies $sfenOut
        $parityOutput | ForEach-Object { Write-Host $_ }

        $coverageLine = $parityOutput | Where-Object { $_ -match "^info string parity-sfen coverage " } | Select-Object -Last 1
        if (-not $coverageLine) {
            throw "parity-sfen coverage 行が出力されませんでした。"
        }

        if ($coverageLine -notmatch "kingmove\s+(\d+)\s+promotion\s+(\d+)\s+drop\s+(\d+)") {
            throw "parity-sfen coverage 行の形式が不正です: $coverageLine"
        }

        $kingMoveCount = [int]$Matches[1]
        $promotionCount = [int]$Matches[2]
        $dropCount = [int]$Matches[3]
        if ($kingMoveCount -lt $minimumCoverageEach -or $promotionCount -lt $minimumCoverageEach -or $dropCount -lt $minimumCoverageEach) {
            throw "parity-sfen coverage不足: kingmove=$kingMoveCount promotion=$promotionCount drop=$dropCount"
        }

        $SfenListFile = $sfenOut
    }

    Write-Host "[1/3] parityケースを生成します.."
    $genArgs = @{
        EnginePath = $EnginePath
        EvalDir = $EvalDir
        OutFile = $CasesOutFile
    }

    if (-not [string]::IsNullOrWhiteSpace($SfenListFile)) {
        $genArgs["SfenListFile"] = $SfenListFile
    }

    & $generator @genArgs

    $casesPath = Resolve-Path $CasesOutFile
    $caseCount = (Get-Content $casesPath | Measure-Object -Line).Lines
    Write-Host "[2/3] parityケース数: $caseCount"
    if ($caseCount -lt $minimumStrictCases) {
        throw "parityケース数が不足しています。required>=$minimumStrictCases actual=$caseCount"
    }

    Write-Host "[3/3] strictテストを実行します.."
    $previousStrict = $env:NNUE_PARITY_STRICT
    try {
        $env:NNUE_PARITY_STRICT = "1"
        dotnet test csharp/YaneuraOu.CSharp.sln --filter "FullyQualifiedName~NnueParityTests" -v minimal
    }
    finally {
        $env:NNUE_PARITY_STRICT = $previousStrict
    }

    Write-Host "完了: parity strictワークフロー成功"
}
finally {
    Pop-Location
}
