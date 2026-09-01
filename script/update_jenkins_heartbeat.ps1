param(
    [Parameter(Mandatory = $true)]
    [string]$HeartbeatPath,
    [Parameter(Mandatory = $true)]
    [int]$OwnerProcessId,
    [Parameter(Mandatory = $true)]
    [long]$OwnerStartedAtTicks,
    [Parameter(Mandatory = $true)]
    [string]$PidPath,
    [int]$IntervalSec = 2,
    [Parameter(Mandatory = $true)]
    [string]$LogPath,
    [Parameter(Mandatory = $true)]
    [string]$RunId,
    [int]$MaxConsecutiveFailures = 20,
    [int]$RetryDelayMilliseconds = 250
)

$ErrorActionPreference = 'Stop'
if ($IntervalSec -le 0) { throw 'IntervalSec must be positive' }
if ($MaxConsecutiveFailures -le 0) { throw 'MaxConsecutiveFailures must be positive' }
if ($RetryDelayMilliseconds -le 0) { throw 'RetryDelayMilliseconds must be positive' }

function Write-HeartbeatLog {
    param([string]$Event, [string]$Detail = '')
    $safeDetail = $Detail.Replace("`r", ' ').Replace("`n", ' ')
    $line = '[heartbeat-helper] timestamp={0} run_id={1} event={2} {3}' -f `
        [DateTimeOffset]::UtcNow.ToString('O'), $RunId, $Event, $safeDetail
    try {
        [System.IO.File]::AppendAllText(
            $LogPath,
            $line.TrimEnd() + [Environment]::NewLine,
            [System.Text.UTF8Encoding]::new($false)
        )
    }
    catch {
        [Console]::Error.WriteLine($line.TrimEnd() + " log_error=$($_.Exception.Message)")
    }
}

$currentProcessId = [System.Diagnostics.Process]::GetCurrentProcess().Id
$exitReason = 'owner-exited'
$exitCode = 0
try {
    [System.IO.File]::WriteAllText($PidPath, $currentProcessId.ToString())
    Write-HeartbeatLog -Event 'started' -Detail "pid=$currentProcessId owner_pid=$OwnerProcessId"
    $consecutiveFailures = 0
    while ($true) {
        try {
            $ownerProcess = [System.Diagnostics.Process]::GetProcessById($OwnerProcessId)
            try {
                $ownerMatches = $ownerProcess.StartTime.ToUniversalTime().Ticks -eq $OwnerStartedAtTicks
            }
            finally {
                $ownerProcess.Dispose()
            }
            if (-not $ownerMatches) {
                $exitReason = 'owner-reused'
                break
            }
            [System.IO.File]::WriteAllText($HeartbeatPath, [DateTimeOffset]::UtcNow.ToString('O'))
            if ($consecutiveFailures -gt 0) {
                Write-HeartbeatLog -Event 'recovered' -Detail "failures=$consecutiveFailures"
            }
            $consecutiveFailures = 0
        }
        catch [System.ArgumentException] {
            $exitReason = 'owner-exited'
            break
        }
        catch {
            $consecutiveFailures++
            if ($consecutiveFailures -eq 1 -or $consecutiveFailures -eq $MaxConsecutiveFailures) {
                Write-HeartbeatLog -Event 'write-failed' -Detail (
                    "failures=$consecutiveFailures error=$($_.Exception.Message)"
                )
            }
            if ($consecutiveFailures -ge $MaxConsecutiveFailures) {
                $exitReason = 'consecutive-failures'
                $exitCode = 1
                break
            }
            Start-Sleep -Milliseconds $RetryDelayMilliseconds
            continue
        }
        Start-Sleep -Seconds $IntervalSec
    }
}
catch {
    $exitReason = 'fatal-error'
    $exitCode = 1
    Write-HeartbeatLog -Event 'fatal' -Detail "error=$($_.Exception.Message)"
}
finally {
    Remove-Item -LiteralPath $PidPath -Force -ErrorAction SilentlyContinue
    Write-HeartbeatLog -Event 'stopped' -Detail "reason=$exitReason exit_code=$exitCode"
}
exit $exitCode
