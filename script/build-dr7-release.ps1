param(
    [Parameter(Mandatory = $true)][string]$EvalPath,
    [Parameter(Mandatory = $true)][string]$ProgressPath,
    [string]$Python = 'python',
    [string]$OutputRoot = 'build'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repo

$targets = @(
    'SSE2', 'SSSE3', 'SSE41', 'SSE42', 'AVX2', 'AVXVNNI', 'AVX512', 'AVX512VNNI',
    'ZEN1', 'ZEN2', 'ZEN3', 'ZEN3AVX512VNNI', 'ZEN4', 'ZEN5', 'ZEN6'
)
$release = Join-Path $OutputRoot 'tanuki-dr7_prd'
$archive = Join-Path $OutputRoot 'tanuki-dr7_prd-win64.zip'
$workName = 'dr7-build-' + [Guid]::NewGuid().ToString('N')
$workDir = Join-Path $repo "build/$workName"
if ((Test-Path -LiteralPath $release) -or (Test-Path -LiteralPath $archive)) {
    throw 'Release output already exists. Use a clean output directory.'
}
foreach ($path in @($EvalPath, $ProgressPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "File not found: $path" }
}
New-Item -ItemType Directory -Force -Path $release, (Join-Path $release 'eval') | Out-Null
Copy-Item -LiteralPath $EvalPath -Destination (Join-Path $release 'eval/nn.bin')
Copy-Item -LiteralPath $ProgressPath -Destination (Join-Path $release 'eval/progress.bin')
Copy-Item -LiteralPath 'LICENSE' -Destination (Join-Path $release 'gpl-3.0.txt')
Copy-Item -LiteralPath 'docs/tanuki-dr7-prd-release.txt' -Destination (Join-Path $release 'README.txt')

foreach ($target in $targets) {
    $suffix = $target.ToLowerInvariant()
    $targetDir = "../build/$workName/bin/$suffix"
    $objDir = "../build/$workName/obj/$suffix"
    $localTargetDir = Join-Path $workDir "bin/$suffix"
    New-Item -ItemType Directory -Force -Path $localTargetDir | Out-Null
    Write-Host "Building $target"
    # Clang writes progress to stderr; use the exit code to detect failure.
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = (Get-Command make -CommandType Application).Source
    $start.WorkingDirectory = $repo
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.Arguments = @(
        '-C', 'source', '-j8', 'tournament', 'COMPILER=clang++', "PYTHON=$Python",
        "TARGET_CPU=$target", 'EVAL_EMBEDDING=OFF',
        'YANEURAOU_EDITION=YANEURAOU_ENGINE_SFNN_halfkahm2_768_7_32_progress8',
        "OBJDIR=$objDir", "TARGETDIR=$targetDir"
    ) -join ' '
    $process = [System.Diagnostics.Process]::Start($start)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $buildExitCode = $process.ExitCode
    $buildLog = Join-Path $workDir "$suffix.log"
    [System.IO.File]::WriteAllText($buildLog, $stdout.Result + $stderr.Result)
    $process.Dispose()
    if ($buildExitCode -ne 0) {
        Get-Content $buildLog -Tail 30
        throw "Build failed: $target"
    }
    Copy-Item -LiteralPath (Join-Path $localTargetDir 'YaneuraOu-by-gcc.exe') `
        -Destination (Join-Path $release "tanuki-dr7_prd-$suffix.exe")
}

Compress-Archive -Path (Join-Path $release '*') -DestinationPath $archive -CompressionLevel Optimal
Write-Host "Created $archive"
