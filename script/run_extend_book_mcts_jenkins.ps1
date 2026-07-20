param(
    [Parameter(Mandatory = $true)]
    [string]$RuntimeExe,
    [Parameter(Mandatory = $true)]
    [string]$StateDir,
    [int]$HeartbeatIntervalSec = 2,
    [int]$GracefulStopTimeoutSec = 75,
    [int]$ProgressIntervalSec = 60,
    [int]$LogRetentionCount = 5,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RuntimeArgs
)

$ErrorActionPreference = 'Stop'

function ConvertTo-CommandLineArgument {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Argument)
    if ($Argument.Length -gt 0 -and $Argument -notmatch '[\s"]') {
        return $Argument
    }
    $escaped = [System.Text.RegularExpressions.Regex]::Replace($Argument, '(\\*)"', '$1$1\"')
    $escaped = [System.Text.RegularExpressions.Regex]::Replace($escaped, '(\\+)$', '$1$1')
    return '"' + $escaped + '"'
}
function Read-NewLogChunk {
    param([string]$Path, [long]$Offset)
    if (-not [System.IO.File]::Exists($Path)) {
        return [pscustomobject]@{ Text = ''; Offset = $Offset }
    }
    $stream = [System.IO.File]::Open(
        $Path,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::ReadWrite
    )
    try {
        if ($stream.Length -lt $Offset) { $Offset = 0 }
        [void]$stream.Seek($Offset, [System.IO.SeekOrigin]::Begin)
        $reader = [System.IO.StreamReader]::new(
            $stream,
            [System.Text.UTF8Encoding]::new($false),
            $true,
            4096,
            $true
        )
        try {
            $chunk = $reader.ReadToEnd()
            return [pscustomobject]@{ Text = $chunk; Offset = $stream.Position }
        }
        finally {
            $reader.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}

function Write-NewLogChunks {
    param(
        [string]$StdoutPath,
        [string]$StderrPath,
        [ref]$StdoutOffset,
        [ref]$StderrOffset
    )
    $stdoutChunk = Read-NewLogChunk -Path $StdoutPath -Offset $StdoutOffset.Value
    $StdoutOffset.Value = $stdoutChunk.Offset
    if ($stdoutChunk.Text.Length -gt 0) {
        [Console]::Out.Write($stdoutChunk.Text)
        [Console]::Out.Flush()
    }
    $stderrChunk = Read-NewLogChunk -Path $StderrPath -Offset $StderrOffset.Value
    $StderrOffset.Value = $stderrChunk.Offset
    if ($stderrChunk.Text.Length -gt 0) {
        [Console]::Out.Write($stderrChunk.Text)
        [Console]::Out.Flush()
    }
}

function Write-ProgressSummary {
    param(
        [string]$StatusPath,
        [string]$RunId,
        [int]$StaleAfterSec
    )
    if (-not [System.IO.File]::Exists($StatusPath)) {
        Write-Output "[progress-warning] run_id=$RunId reason=status-missing path=$StatusPath"
        return
    }
    try {
        $status = Get-Content -LiteralPath $StatusPath -Raw | ConvertFrom-Json
        if ($status.run_id -ne $RunId) {
            Write-Output "[progress-warning] run_id=$RunId reason=run-id-mismatch actual=$($status.run_id)"
            return
        }
        $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0
        $statusAge = [Math]::Max(0, [long]($now - [double]$status.updated_at))
        if ($statusAge -gt $StaleAfterSec) {
            Write-Output "[progress-warning] run_id=$RunId reason=status-stale age_sec=$statusAge"
            return
        }
        $activeNormal = 0L
        if ($null -ne $status.lane_active) {
            foreach ($property in $status.lane_active.PSObject.Properties) {
                $activeNormal += [long]$property.Value
            }
        }
        $lastSaveAge = -1L
        if ($null -ne $status.last_book_save) {
            $lastSaveAge = [Math]::Max(0, [long]($now - [double]$status.last_book_save.saved_at))
        }
        $uptime = [Math]::Max(0, [long]($now - [double]$status.started_at))
        Write-Output (
            "[progress] run_id=$RunId uptime_sec=$uptime searches=$($status.searches) " +
            "added_positions=$($status.added_positions) total_nodes=$($status.total_nodes) " +
            "active_normal=$activeNormal active_corpus=$($status.corpus_active) " +
            "band=$($status.active_quality_band) n=$($status.progressive_width) " +
            "corpus_evaluated=$($status.tasks.evaluated) corpus_added=$($status.book_add_successes) " +
            "last_save_age_sec=$lastSaveAge"
        )
    }
    catch {
        Write-Output "[progress-warning] run_id=$RunId reason=status-read-failed error=$($_.Exception.Message)"
    }
}

function Remove-OldRunLogs {
    param(
        [string]$LogDirectory,
        [int]$RetentionCount,
        [string]$ActiveRunId
    )
    $completedRunIds = @(
        Get-ChildItem -LiteralPath $LogDirectory -Filter 'book-extender-*.stderr.log' -File |
            Where-Object { $_.BaseName -ne "book-extender-$ActiveRunId.stderr" } |
            Sort-Object LastWriteTimeUtc -Descending |
            ForEach-Object {
                $_.Name.Substring('book-extender-'.Length).Replace('.stderr.log', '')
            } |
            Select-Object -Unique
    )
    $completedToKeep = [Math]::Max(0, $RetentionCount - 1)
    $completedRunIds | Select-Object -Skip $completedToKeep | ForEach-Object {
        Remove-Item -LiteralPath (Join-Path $LogDirectory "book-extender-$_.stdout.log") -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath (Join-Path $LogDirectory "book-extender-$_.stderr.log") -Force -ErrorAction SilentlyContinue
    }
}

if ($HeartbeatIntervalSec -le 0) { throw 'HeartbeatIntervalSec must be positive' }
if ($GracefulStopTimeoutSec -le 0) { throw 'GracefulStopTimeoutSec must be positive' }
if ($ProgressIntervalSec -le 0) { throw 'ProgressIntervalSec must be positive' }
if ($LogRetentionCount -le 0) { throw 'LogRetentionCount must be positive' }

$statePath = [System.IO.Path]::GetFullPath($StateDir)
[System.IO.Directory]::CreateDirectory($statePath) | Out-Null
$heartbeatPath = Join-Path $statePath 'jenkins.heartbeat'
$heartbeatHelperPidPath = Join-Path $statePath 'jenkins-heartbeat-helper.pid'
$heartbeatHelperPath = Join-Path $PSScriptRoot 'update_jenkins_heartbeat.ps1'
$stopRequestPath = Join-Path $statePath 'stop.request'
$lockPath = Join-Path $statePath 'book-extension.lock'
$statusPath = Join-Path $statePath 'runtime-status.json'
$logDirectory = Join-Path $statePath 'logs'
[System.IO.Directory]::CreateDirectory($logDirectory) | Out-Null
$runId = '{0}-{1}' -f [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssZ'), ([Guid]::NewGuid().ToString('N').Substring(0, 8))
$stdoutLogPath = Join-Path $logDirectory "book-extender-$runId.stdout.log"
$stderrLogPath = Join-Path $logDirectory "book-extender-$runId.stderr.log"
Remove-Item -LiteralPath $stopRequestPath -Force -ErrorAction SilentlyContinue
Remove-OldRunLogs -LogDirectory $logDirectory -RetentionCount $LogRetentionCount -ActiveRunId $runId

$arguments = @(
    '--heartbeat-path', $heartbeatPath
    '--stop-request-path', $stopRequestPath
    '--lock-path', $lockPath
    '--run-id', $runId
)
if ($null -ne $RuntimeArgs) {
    $arguments += $RuntimeArgs
}

if (-not (Test-Path -LiteralPath $heartbeatHelperPath -PathType Leaf)) {
    throw "Heartbeat helper not found: $heartbeatHelperPath"
}
[System.IO.File]::WriteAllText($heartbeatPath, [DateTimeOffset]::UtcNow.ToString('O'))
$ownerProcess = [System.Diagnostics.Process]::GetCurrentProcess()
$heartbeatArguments = @(
    '-NoProfile'
    '-ExecutionPolicy', 'Bypass'
    '-File', $heartbeatHelperPath
    '-HeartbeatPath', $heartbeatPath
    '-OwnerProcessId', $ownerProcess.Id.ToString()
    '-OwnerStartedAtTicks', $ownerProcess.StartTime.ToUniversalTime().Ticks.ToString()
    '-PidPath', $heartbeatHelperPidPath
    '-IntervalSec', $HeartbeatIntervalSec.ToString()
)
$heartbeatStartInfo = [System.Diagnostics.ProcessStartInfo]::new()
$heartbeatStartInfo.FileName = 'powershell.exe'
$heartbeatStartInfo.Arguments = (($heartbeatArguments | ForEach-Object { ConvertTo-CommandLineArgument $_ }) -join ' ')
$heartbeatStartInfo.UseShellExecute = $false
$heartbeatStartInfo.CreateNoWindow = $true
$heartbeatProcess = [System.Diagnostics.Process]::new()
$heartbeatProcess.StartInfo = $heartbeatStartInfo
if (-not $heartbeatProcess.Start()) {
    throw 'Failed to start Jenkins heartbeat helper'
}

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $RuntimeExe
$startInfo.Arguments = (($arguments | ForEach-Object { ConvertTo-CommandLineArgument $_ }) -join ' ')
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
# Freestyle ProcessTreeKiller must leave only the Rust child alive when Abort kills this wrapper.
$startInfo.EnvironmentVariables['BUILD_ID'] = 'dontKillMe'

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $startInfo
$processStarted = $false
$stdoutWriteStream = $null
$stderrWriteStream = $null
$stdoutCopyTask = $null
$stderrCopyTask = $null
$stdoutOffset = 0L
$stderrOffset = 0L
$nextProgress = [DateTimeOffset]::UtcNow.AddSeconds($ProgressIntervalSec)
try {
    $stdoutWriteStream = [System.IO.File]::Open(
        $stdoutLogPath,
        [System.IO.FileMode]::Create,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::ReadWrite
    )
    $stderrWriteStream = [System.IO.File]::Open(
        $stderrLogPath,
        [System.IO.FileMode]::Create,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::ReadWrite
    )
    if (-not $process.Start()) {
        throw "Failed to start book extension runtime: $RuntimeExe"
    }
    $processStarted = $true
    $stdoutCopyTask = $process.StandardOutput.BaseStream.CopyToAsync($stdoutWriteStream)
    $stderrCopyTask = $process.StandardError.BaseStream.CopyToAsync($stderrWriteStream)

    while ($true) {
        Write-NewLogChunks `
            -StdoutPath $stdoutLogPath `
            -StderrPath $stderrLogPath `
            -StdoutOffset ([ref]$stdoutOffset) `
            -StderrOffset ([ref]$stderrOffset)
        if ($process.HasExited) { break }
        if ([DateTimeOffset]::UtcNow -ge $nextProgress) {
            Write-ProgressSummary `
                -StatusPath $statusPath `
                -RunId $runId `
                -StaleAfterSec ($ProgressIntervalSec * 2)
            $nextProgress = [DateTimeOffset]::UtcNow.AddSeconds($ProgressIntervalSec)
        }
        Start-Sleep -Seconds $HeartbeatIntervalSec
    }
    $process.WaitForExit()
    $childExitCode = $process.ExitCode
    [void]$stdoutCopyTask.GetAwaiter().GetResult()
    [void]$stderrCopyTask.GetAwaiter().GetResult()
    $stdoutWriteStream.Flush()
    $stderrWriteStream.Flush()
    Write-NewLogChunks `
        -StdoutPath $stdoutLogPath `
        -StderrPath $stderrLogPath `
        -StdoutOffset ([ref]$stdoutOffset) `
        -StderrOffset ([ref]$stderrOffset)
    Remove-OldRunLogs -LogDirectory $logDirectory -RetentionCount $LogRetentionCount -ActiveRunId $runId
    exit $childExitCode
}
finally {
    if ($null -ne $heartbeatProcess -and -not $heartbeatProcess.HasExited) {
        Stop-Process -Id $heartbeatProcess.Id -Force -ErrorAction SilentlyContinue
    }
    if ($null -ne $heartbeatProcess) { $heartbeatProcess.Dispose() }
    Remove-Item -LiteralPath $heartbeatHelperPidPath -Force -ErrorAction SilentlyContinue
    if ($processStarted -and -not $process.HasExited) {
        [System.IO.File]::WriteAllText($stopRequestPath, 'jenkins-wrapper-stopping')
        if (-not $process.WaitForExit($GracefulStopTimeoutSec * 1000)) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            throw "Book extension did not stop within $GracefulStopTimeoutSec seconds"
        }
    }
    if ($null -ne $stdoutCopyTask -and -not $stdoutCopyTask.IsCompleted) {
        [void]$stdoutCopyTask.GetAwaiter().GetResult()
    }
    if ($null -ne $stderrCopyTask -and -not $stderrCopyTask.IsCompleted) {
        [void]$stderrCopyTask.GetAwaiter().GetResult()
    }
    if ($null -ne $stdoutWriteStream) { $stdoutWriteStream.Flush() }
    if ($null -ne $stderrWriteStream) { $stderrWriteStream.Flush() }
    Write-NewLogChunks `
        -StdoutPath $stdoutLogPath `
        -StderrPath $stderrLogPath `
        -StdoutOffset ([ref]$stdoutOffset) `
        -StderrOffset ([ref]$stderrOffset)
    if ($null -ne $stdoutWriteStream) { $stdoutWriteStream.Dispose() }
    if ($null -ne $stderrWriteStream) { $stderrWriteStream.Dispose() }
    if ($null -ne $process) { $process.Dispose() }
}