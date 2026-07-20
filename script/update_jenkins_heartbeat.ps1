param(
    [Parameter(Mandatory = $true)]
    [string]$HeartbeatPath,
    [Parameter(Mandatory = $true)]
    [int]$OwnerProcessId,
    [Parameter(Mandatory = $true)]
    [long]$OwnerStartedAtTicks,
    [Parameter(Mandatory = $true)]
    [string]$PidPath,
    [int]$IntervalSec = 2
)

$ErrorActionPreference = 'Stop'
if ($IntervalSec -le 0) { throw 'IntervalSec must be positive' }

$currentProcessId = [System.Diagnostics.Process]::GetCurrentProcess().Id
[System.IO.File]::WriteAllText($PidPath, $currentProcessId.ToString())
try {
    while ($true) {
        try {
            $ownerProcess = [System.Diagnostics.Process]::GetProcessById($OwnerProcessId)
            $ownerMatches = $ownerProcess.StartTime.ToUniversalTime().Ticks -eq $OwnerStartedAtTicks
            $ownerProcess.Dispose()
            if (-not $ownerMatches) { break }
        }
        catch [System.ArgumentException] {
            break
        }
        [System.IO.File]::WriteAllText($HeartbeatPath, [DateTimeOffset]::UtcNow.ToString('O'))
        Start-Sleep -Seconds $IntervalSec
    }
}
finally {
    Remove-Item -LiteralPath $PidPath -Force -ErrorAction SilentlyContinue
}
