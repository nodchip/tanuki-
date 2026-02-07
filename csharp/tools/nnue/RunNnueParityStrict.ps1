param(
    [string]$EnginePath = "source/YaneuraOu-by-gcc.exe",
    [string]$EvalDir = "../eval",
    [string]$CasesOutFile = "eval/nnue-parity-cases.jsonl",
    [string]$SfenListFile = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..\..\..")
$generator = Join-Path $scriptDir "GenerateNnueParityCases.ps1"

if (-not (Test-Path $generator)) {
    throw "GenerateNnueParityCases.ps1 が見つかりません: $generator"
}

Push-Location $repoRoot
try {
    Write-Host "[1/3] parityケース生成を開始します..."
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
    Write-Host "[2/3] parityケース件数: $caseCount"

    Write-Host "[3/3] strict照合テストを実行します..."
    $env:NNUE_PARITY_STRICT = "1"
    dotnet test csharp/YaneuraOu.CSharp.sln --filter "FullyQualifiedName~NnueParityTests" -v minimal

    Write-Host "完了: parity strictワークフロー成功"
}
finally {
    Pop-Location
}
