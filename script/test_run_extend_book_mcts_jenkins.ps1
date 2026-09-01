$ErrorActionPreference = 'Stop'

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$wrapperPath = Join-Path $PSScriptRoot 'run_extend_book_mcts_jenkins.ps1'
$heartbeatHelperPath = Join-Path $PSScriptRoot 'update_jenkins_heartbeat.ps1'
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("book-extension-wrapper-test-" + [Guid]::NewGuid().ToString('N'))
$statePath = Join-Path $temporaryRoot 'state'
$fakeRuntimePath = Join-Path $temporaryRoot 'fake-progress-runtime.exe'
[System.IO.Directory]::CreateDirectory($statePath) | Out-Null

try {
    $source = @"
using System;
using System.IO;
using System.Threading;
public static class FakeProgressRuntime
{
    private static string ValueAfter(string[] args, string name)
    {
        for (int i = 0; i + 1 < args.Length; ++i)
            if (args[i] == name)
                return args[i + 1];
        throw new InvalidOperationException("missing argument: " + name);
    }

    public static int Main(string[] args)
    {
        if (Array.IndexOf(args, "--flood-output") >= 0)
        {
            string floodHeartbeatPath = ValueAfter(args, "--heartbeat-path");
            string statePath = Path.GetDirectoryName(floodHeartbeatPath);
            File.WriteAllText(Path.Combine(statePath, "fake-runtime.pid"),
                System.Diagnostics.Process.GetCurrentProcess().Id.ToString());
            string payload = new string('x', 16384);
            while (true)
            {
                Console.Error.WriteLine(payload);
                Console.Error.Flush();
                Thread.Sleep(1);
            }
        }
        if (Array.IndexOf(args, "--wait-for-stop-request") >= 0)
        {
            string stopHeartbeatPath = ValueAfter(args, "--heartbeat-path");
            string stopRequestPath = ValueAfter(args, "--stop-request-path");
            string stopStatePath = Path.GetDirectoryName(stopHeartbeatPath);
            File.WriteAllText(Path.Combine(stopStatePath, "fake-runtime.pid"),
                System.Diagnostics.Process.GetCurrentProcess().Id.ToString());
            while (!File.Exists(stopRequestPath))
                Thread.Sleep(50);
            return 0;
        }
        Console.Out.WriteLine("fake-out-1");
        Console.Out.Flush();
        Console.Error.WriteLine("fake-err-1");
        Console.Error.Flush();
        Thread.Sleep(1200);
        string runId = ValueAfter(args, "--run-id");
        string heartbeatPath = ValueAfter(args, "--heartbeat-path");
        string statusPath = Path.Combine(Path.GetDirectoryName(heartbeatPath), "runtime-status.json");
        long now = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        string status = "{\"run_id\":\"" + runId + "\",\"started_at\":" + (now - 2) +
            ",\"updated_at\":" + now + ",\"searches\":12,\"added_positions\":3," +
            "\"total_nodes\":1200,\"corpus_active\":1,\"lane_active\":{\"normal\":2}," +
            "\"active_quality_band\":0,\"progressive_width\":4," +
            "\"tasks\":{\"evaluated\":5},\"book_add_successes\":2,\"last_book_save\":null}";
        File.WriteAllText(statusPath, status);
        Console.Error.Write(new string('x', 16384) + "[search-finish] lane=normal de");
        Console.Error.Flush();
        Thread.Sleep(4000);
        Console.Error.WriteLine("pth=32 status=ok");
        Console.Error.WriteLine("fake-err-2");
        Console.Error.Write("tail-without-newline");
        Console.Out.WriteLine("args=" + String.Join("|", args));
        Console.Error.Flush();
        Console.Out.Flush();
        return 7;
    }
}
"@
    Add-Type -TypeDefinition $source -Language CSharp -OutputAssembly $fakeRuntimePath -OutputType ConsoleApplication

    $logDirectory = Join-Path $statePath 'logs'
    [System.IO.Directory]::CreateDirectory($logDirectory) | Out-Null
    0..5 | ForEach-Object {
        $runId = "old-run-$_"
        $stdoutPath = Join-Path $logDirectory "book-extender-$runId.stdout.log"
        $stderrPath = Join-Path $logDirectory "book-extender-$runId.stderr.log"
        [System.IO.File]::WriteAllText($stdoutPath, "old stdout $_")
        [System.IO.File]::WriteAllText($stderrPath, "old stderr $_")
        $timestamp = [DateTime]::UtcNow.AddMinutes(-20 - $_)
        [System.IO.File]::SetLastWriteTimeUtc($stdoutPath, $timestamp)
        [System.IO.File]::SetLastWriteTimeUtc($stderrPath, $timestamp)
    }

    $captured = & powershell.exe `
        -NoProfile `
        -ExecutionPolicy Bypass `
        -File $wrapperPath `
        -RuntimeExe $fakeRuntimePath `
        -StateDir $statePath `
        -HeartbeatIntervalSec 1 `
        -GracefulStopTimeoutSec 5 `
        -ProgressIntervalSec 1 `
        -LogRetentionCount 5 `
        --probe-value 'value with spaces' 2>&1
    $wrapperExitCode = $LASTEXITCODE
    $capturedText = ($captured | Out-String)

    Assert-True ($wrapperExitCode -eq 7) "expected wrapper exit code 7, got $wrapperExitCode`n$capturedText"
    Assert-True $capturedText.Contains('fake-out-1') "stdout was not relayed`n$capturedText"
    Assert-True $capturedText.Contains('fake-err-1') "first stderr line was not relayed`n$capturedText"
    Assert-True $capturedText.Contains('fake-err-2') "final stderr line was not drained`n$capturedText"
    Assert-True (@($capturedText -split "`r?`n") -contains 'tail-without-newline') "unterminated final stderr fragment was not relayed`n$capturedText"
    Assert-True $capturedText.Contains('--probe-value|value with spaces') "space-containing argument was corrupted`n$capturedText"
    Assert-True $capturedText.Contains('[progress-warning]') "missing progress warning`n$capturedText"
    Assert-True $capturedText.Contains('[progress]') "missing progress summary`n$capturedText"
    Assert-True $capturedText.Contains('searches=12') "progress summary did not use runtime status`n$capturedText"
    Assert-True $capturedText.Contains('[search-finish] lane=normal depth=32 status=ok') "split runtime line was corrupted`n$capturedText"
    Assert-True (-not $capturedText.Contains('de[progress]')) "progress was inserted into a runtime line`n$capturedText"
    $progressLines = @(($capturedText -split "`r?`n") | Where-Object { $_.Contains('[progress]') })
    Assert-True ($progressLines.Count -gt 0) "progress was not emitted`n$capturedText"
    Assert-True (@($progressLines | Where-Object { -not $_.StartsWith('[progress] ') }).Count -eq 0) "progress was inserted into a runtime line`n$capturedText"
    Assert-True (-not $capturedText.Contains('System.Threading.Tasks.VoidTaskResult')) "async task result leaked to console`n$capturedText"
    Assert-True (Test-Path -LiteralPath (Join-Path $statePath 'jenkins.heartbeat')) 'heartbeat was not created'

    $stdoutLogs = @(Get-ChildItem -LiteralPath $logDirectory -Filter 'book-extender-*.stdout.log' -File)
    $stderrLogs = @(Get-ChildItem -LiteralPath $logDirectory -Filter 'book-extender-*.stderr.log' -File)
    Assert-True ($stdoutLogs.Count -eq 5) "expected five stdout logs, got $($stdoutLogs.Count)"
    Assert-True ($stderrLogs.Count -eq 5) "expected five stderr logs, got $($stderrLogs.Count)"
    Assert-True (($stdoutLogs | Get-Content -Raw) -join "`n").Contains('fake-out-1') 'durable stdout log missing fake output'
    Assert-True (($stderrLogs | Get-Content -Raw) -join "`n").Contains('fake-err-2') 'durable stderr log missing final output'
    Assert-True (($stderrLogs | Get-Content -Raw) -join "`n").Contains('tail-without-newline') 'durable stderr log lost unterminated final fragment'
    Assert-True (($stderrLogs | Get-Content -Raw) -join "`n").Contains('[search-finish] lane=normal depth=32 status=ok') 'durable stderr log lost the split line'

    $retryStatePath = Join-Path $temporaryRoot 'heartbeat-retry-state'
    [System.IO.Directory]::CreateDirectory($retryStatePath) | Out-Null
    $retryHeartbeatPath = Join-Path $retryStatePath 'jenkins.heartbeat'
    $retryPidPath = Join-Path $retryStatePath 'jenkins-heartbeat-helper.pid'
    $retryLogPath = Join-Path $retryStatePath 'heartbeat-helper-retry.log'
    [System.IO.File]::WriteAllText($retryHeartbeatPath, 'locked')
    $heartbeatLock = [System.IO.File]::Open(
        $retryHeartbeatPath,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read
    )
    $retryHelperStartInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $retryHelperStartInfo.FileName = 'powershell.exe'
    $retryHelperStartInfo.Arguments = (
        '-NoProfile -ExecutionPolicy Bypass -File "{0}" -HeartbeatPath "{1}" ' +
        '-OwnerProcessId {2} -OwnerStartedAtTicks {3} -PidPath "{4}" ' +
        '-IntervalSec 1 -LogPath "{5}" -RunId retry-test ' +
        '-MaxConsecutiveFailures 20 -RetryDelayMilliseconds 100'
    ) -f $heartbeatHelperPath, $retryHeartbeatPath, $PID,
        ([System.Diagnostics.Process]::GetCurrentProcess().StartTime.ToUniversalTime().Ticks),
        $retryPidPath, $retryLogPath
    $retryHelperStartInfo.UseShellExecute = $false
    $retryHelperStartInfo.CreateNoWindow = $true
    $retryHelper = [System.Diagnostics.Process]::new()
    $retryHelper.StartInfo = $retryHelperStartInfo
    try {
        [void]$retryHelper.Start()
        $deadline = [DateTime]::UtcNow.AddSeconds(3)
        while (-not (Test-Path -LiteralPath $retryPidPath) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 50
        }
        Assert-True (Test-Path -LiteralPath $retryPidPath) 'retry helper pid was not recorded'
        Start-Sleep -Milliseconds 400
        $heartbeatLock.Dispose()
        $heartbeatLock = $null
        $deadline = [DateTime]::UtcNow.AddSeconds(4)
        do {
            Start-Sleep -Milliseconds 100
            $retryLog = if (Test-Path -LiteralPath $retryLogPath) {
                Get-Content -LiteralPath $retryLogPath -Raw
            } else {
                ''
            }
        } while (-not $retryLog.Contains('event=recovered') -and [DateTime]::UtcNow -lt $deadline)
        Assert-True $retryLog.Contains('event=write-failed') "heartbeat helper did not log the transient failure`n$retryLog"
        Assert-True $retryLog.Contains('event=recovered') "heartbeat helper did not recover from the transient failure`n$retryLog"
        Assert-True (-not $retryHelper.HasExited) 'heartbeat helper exited after a transient write failure'
    }
    finally {
        if ($null -ne $heartbeatLock) { $heartbeatLock.Dispose() }
        if ($null -ne $retryHelper -and -not $retryHelper.HasExited) {
            Stop-Process -Id $retryHelper.Id -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $retryHelper) { $retryHelper.Dispose() }
    }

    $helperFailureStatePath = Join-Path $temporaryRoot 'helper-failure-state'
    [System.IO.Directory]::CreateDirectory($helperFailureStatePath) | Out-Null
    $helperFailureStartInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $helperFailureStartInfo.FileName = 'powershell.exe'
    $helperFailureStartInfo.Arguments = (
        '-NoProfile -ExecutionPolicy Bypass -File "{0}" -RuntimeExe "{1}" ' +
        '-StateDir "{2}" -HeartbeatIntervalSec 1 -GracefulStopTimeoutSec 5 ' +
        '-ProgressIntervalSec 60 -LogRetentionCount 1 --wait-for-stop-request'
    ) -f $wrapperPath, $fakeRuntimePath, $helperFailureStatePath
    $helperFailureStartInfo.UseShellExecute = $false
    $helperFailureStartInfo.CreateNoWindow = $true
    $helperFailureStartInfo.RedirectStandardOutput = $true
    $helperFailureStartInfo.RedirectStandardError = $true
    $helperFailureWrapper = [System.Diagnostics.Process]::new()
    $helperFailureWrapper.StartInfo = $helperFailureStartInfo
    try {
        [void]$helperFailureWrapper.Start()
        $helperFailurePidPath = Join-Path $helperFailureStatePath 'jenkins-heartbeat-helper.pid'
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (-not (Test-Path -LiteralPath $helperFailurePidPath) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 50
        }
        Assert-True (Test-Path -LiteralPath $helperFailurePidPath) 'heartbeat helper pid was not recorded for failure test'
        $helperFailurePid = [int](Get-Content -LiteralPath $helperFailurePidPath -Raw)
        Stop-Process -Id $helperFailurePid -Force
        Assert-True ($helperFailureWrapper.WaitForExit(10000)) 'wrapper did not stop after heartbeat helper exited'
        $helperFailureOutput = $helperFailureWrapper.StandardOutput.ReadToEnd() + $helperFailureWrapper.StandardError.ReadToEnd()
        Assert-True ($helperFailureWrapper.ExitCode -eq 23) "expected helper failure exit code 23, got $($helperFailureWrapper.ExitCode)`n$helperFailureOutput"
        Assert-True $helperFailureOutput.Contains('[heartbeat-helper] event=unexpected-exit') "wrapper did not report helper exit`n$helperFailureOutput"
        Assert-True (Test-Path -LiteralPath (Join-Path $helperFailureStatePath 'stop.request')) 'wrapper did not request a graceful stop after helper exit'
    }
    finally {
        $fakePidPath = Join-Path $helperFailureStatePath 'fake-runtime.pid'
        if (Test-Path -LiteralPath $fakePidPath) {
            Stop-Process -Id ([int](Get-Content -LiteralPath $fakePidPath -Raw)) -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $helperFailureWrapper -and -not $helperFailureWrapper.HasExited) {
            $helperFailureWrapper.Kill()
        }
        if ($null -ne $helperFailureWrapper) { $helperFailureWrapper.Dispose() }
    }

    $blockedStatePath = Join-Path $temporaryRoot 'blocked-state'
    [System.IO.Directory]::CreateDirectory($blockedStatePath) | Out-Null
    $blockedStartInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $blockedStartInfo.FileName = 'powershell.exe'
    $blockedStartInfo.Arguments = (
        '-NoProfile -ExecutionPolicy Bypass -File "{0}" -RuntimeExe "{1}" ' +
        '-StateDir "{2}" -HeartbeatIntervalSec 1 -GracefulStopTimeoutSec 1 ' +
        '-ProgressIntervalSec 60 -LogRetentionCount 1 --flood-output'
    ) -f $wrapperPath, $fakeRuntimePath, $blockedStatePath
    $blockedStartInfo.UseShellExecute = $false
    $blockedStartInfo.CreateNoWindow = $true
    $blockedStartInfo.RedirectStandardOutput = $true
    $blockedStartInfo.RedirectStandardError = $true
    $blockedWrapper = [System.Diagnostics.Process]::new()
    $blockedWrapper.StartInfo = $blockedStartInfo
    try {
        [void]$blockedWrapper.Start()
        $blockedHeartbeatPath = Join-Path $blockedStatePath 'jenkins.heartbeat'
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        while (-not (Test-Path -LiteralPath $blockedHeartbeatPath) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 50
        }
        Assert-True (Test-Path -LiteralPath $blockedHeartbeatPath) 'blocked-console heartbeat was not created'
        Start-Sleep -Seconds 4
        $heartbeatAgeSec = ([DateTime]::UtcNow - (Get-Item -LiteralPath $blockedHeartbeatPath).LastWriteTimeUtc).TotalSeconds
        Assert-True ($heartbeatAgeSec -lt 2.5) "heartbeat stalled behind console relay: age=$heartbeatAgeSec"
        $helperPidPath = Join-Path $blockedStatePath 'jenkins-heartbeat-helper.pid'
        Assert-True (Test-Path -LiteralPath $helperPidPath) 'heartbeat helper pid was not recorded'
        $helperProcessId = [int](Get-Content -LiteralPath $helperPidPath -Raw)
        $blockedWrapper.Kill()
        $blockedWrapper.WaitForExit()
        $helperExitDeadline = [DateTime]::UtcNow.AddSeconds(4)
        do {
            try {
                $helperProbe = [System.Diagnostics.Process]::GetProcessById($helperProcessId)
                try {
                    $helperStillRunning = -not $helperProbe.HasExited
                }
                finally {
                    $helperProbe.Dispose()
                }
            }
            catch [System.ArgumentException] {
                $helperStillRunning = $false
            }
            if ($helperStillRunning) { Start-Sleep -Milliseconds 100 }
        } while ($helperStillRunning -and [DateTime]::UtcNow -lt $helperExitDeadline)
        Assert-True (-not $helperStillRunning) 'heartbeat helper survived its owner wrapper'
    }
    finally {
        $fakePidPath = Join-Path $blockedStatePath 'fake-runtime.pid'
        if (Test-Path -LiteralPath $fakePidPath) {
            Stop-Process -Id ([int](Get-Content -LiteralPath $fakePidPath -Raw)) -Force -ErrorAction SilentlyContinue
        }
        $helperPidPath = Join-Path $blockedStatePath 'jenkins-heartbeat-helper.pid'
        if (Test-Path -LiteralPath $helperPidPath) {
            Stop-Process -Id ([int](Get-Content -LiteralPath $helperPidPath -Raw)) -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $blockedWrapper -and -not $blockedWrapper.HasExited) {
            $blockedWrapper.Kill()
        }
        if ($null -ne $blockedWrapper) { $blockedWrapper.Dispose() }
    }

    Write-Output 'PASS: Jenkins wrapper durable log relay and independent heartbeat'
}
finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
    }
}
