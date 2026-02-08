param(
    [ValidateSet("ShogiHome", "Shogidokoro", "Custom")]
    [string]$Profile = "Custom",

    [string]$LogDir,

    [string[]]$LogFiles,

    [int]$OverrunGraceMs = 200,

    [string]$OutDir = "logs/usi-audit",

    [switch]$Strict
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($LogDir) -and (-not $LogFiles -or $LogFiles.Count -eq 0)) {
    throw "Specify LogDir or LogFiles."
}

if ($Profile -eq "ShogiHome" -and $OverrunGraceMs -eq 200) {
    $OverrunGraceMs = 150
}

if ($Profile -eq "Shogidokoro" -and $OverrunGraceMs -eq 200) {
    $OverrunGraceMs = 300
}

if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$outJson = Join-Path $OutDir ("usi-audit-{0}-{1}.json" -f $Profile.ToLowerInvariant(), $timestamp)

$analyzer = "csharp/tools/selfplay/AnalyzeUsiMatchLogs.ps1"
$args = @(
    "-ExecutionPolicy", "Bypass",
    "-File", $analyzer,
    "-OverrunGraceMs", $OverrunGraceMs,
    "-OutJson", $outJson
)

if (-not [string]::IsNullOrWhiteSpace($LogDir)) {
    $args += @("-LogDir", $LogDir)
}

if ($LogFiles -and $LogFiles.Count -gt 0) {
    $args += "-LogFiles"
    $args += $LogFiles
}

if ($Strict) {
    $args += "-FailOnIllegalMove"
    $args += "-FailOnOverrun"
}

try {
    $jsonText = & powershell @args
} catch {
    throw "AnalyzeUsiMatchLogs.ps1 failed: $($_.Exception.Message)"
}

if ([string]::IsNullOrWhiteSpace($jsonText)) {
    throw "AnalyzeUsiMatchLogs.ps1 returned empty output."
}

try {
    $result = $jsonText | ConvertFrom-Json
} catch {
    throw "AnalyzeUsiMatchLogs.ps1 did not return valid JSON. Output: $jsonText"
}

Write-Host ("usi log audit summary: profile={0} files={1} go={2} bestmove={3} resign={4} tm_ok={5} tm_fallback={6} illegal={7} overrun_suspected={8}" -f `
    $Profile, $result.scanned_files, $result.go_count, $result.bestmove_count, $result.resign_count, $result.tm_ok_count, $result.tm_fallback_count, $result.illegal_move_count, $result.overrun_suspected_count)
Write-Host ("audit json: {0}" -f $outJson)

if (-not $Strict) {
    if ($result.illegal_move_count -gt 0 -or $result.overrun_suspected_count -gt 0) {
        Write-Warning "Potential issues detected. Re-run with -Strict to fail on these conditions."
    }
}
