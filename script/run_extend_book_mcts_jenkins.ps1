param(
    [Parameter(Mandatory = $true)]
    [string]$PythonExe,
    [Parameter(Mandatory = $true)]
    [string]$ExtensionScript,
    [Parameter(Mandatory = $true)]
    [string]$StateDir,
    [int]$HeartbeatIntervalSec = 2,
    [int]$GracefulStopTimeoutSec = 75,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ExtensionArgs
)

$ErrorActionPreference = 'Stop'
$statePath = [System.IO.Path]::GetFullPath($StateDir)
[System.IO.Directory]::CreateDirectory($statePath) | Out-Null
$heartbeatPath = Join-Path $statePath 'jenkins.heartbeat'
$stopRequestPath = Join-Path $statePath 'stop.request'
$lockPath = Join-Path $statePath 'book-extension.lock'
Remove-Item -LiteralPath $stopRequestPath -Force -ErrorAction SilentlyContinue

# Freestyle ProcessTreeKiller must leave the child alive when Abort kills this wrapper.
# The stale heartbeat then asks the child to send USI stop and checkpoint normally.
$previousBuildId = $env:BUILD_ID
$env:BUILD_ID = 'dontKillMe'
$arguments = @(
    $ExtensionScript
    '--heartbeat-path', $heartbeatPath
    '--stop-request-path', $stopRequestPath
    '--lock-path', $lockPath
) + $ExtensionArgs

$process = Start-Process -FilePath $PythonExe -ArgumentList $arguments -PassThru -WindowStyle Hidden
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