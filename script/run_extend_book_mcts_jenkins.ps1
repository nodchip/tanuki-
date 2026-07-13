param(
    [Parameter(Mandatory = $true)]
    [string]$RuntimeExe,
    [Parameter(Mandatory = $true)]
    [string]$StateDir,
    [int]$HeartbeatIntervalSec = 2,
    [int]$GracefulStopTimeoutSec = 75,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RuntimeArgs
)

$ErrorActionPreference = 'Stop'
$statePath = [System.IO.Path]::GetFullPath($StateDir)
[System.IO.Directory]::CreateDirectory($statePath) | Out-Null
$heartbeatPath = Join-Path $statePath 'jenkins.heartbeat'
$stopRequestPath = Join-Path $statePath 'stop.request'
$lockPath = Join-Path $statePath 'book-extension.lock'
Remove-Item -LiteralPath $stopRequestPath -Force -ErrorAction SilentlyContinue

# Freestyle ProcessTreeKiller must leave the Rust child alive when Abort kills this wrapper.
# The stale heartbeat then asks the child to send USI stop and checkpoint normally.
$previousBuildId = $env:BUILD_ID
$env:BUILD_ID = 'dontKillMe'
$arguments = @(
    '--heartbeat-path', $heartbeatPath
    '--stop-request-path', $stopRequestPath
    '--lock-path', $lockPath
) + $RuntimeArgs

[System.IO.File]::WriteAllText($heartbeatPath, [DateTimeOffset]::UtcNow.ToString('O'))
$process = Start-Process -FilePath $RuntimeExe -ArgumentList $arguments -PassThru -WindowStyle Hidden
# Only the child inherited dontKillMe. Restore the wrapper cookie before waiting so Abort can kill it.
$env:BUILD_ID = $previousBuildId
try {
    while (-not $process.HasExited) {
        [System.IO.File]::WriteAllText($heartbeatPath, [DateTimeOffset]::UtcNow.ToString('O'))
        Start-Sleep -Seconds $HeartbeatIntervalSec
        $process.Refresh()
    }
    exit $process.ExitCode
}
finally {
    $env:BUILD_ID = $previousBuildId
    if (-not $process.HasExited) {
        [System.IO.File]::WriteAllText($stopRequestPath, 'jenkins-wrapper-stopping')
        if (-not $process.WaitForExit($GracefulStopTimeoutSec * 1000)) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            throw "Book extension did not stop within $GracefulStopTimeoutSec seconds"
        }
    }
}