$ErrorActionPreference = 'Stop'

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

$wrapperPath = Join-Path $PSScriptRoot 'run_extend_book_mcts_jenkins.ps1'
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
        Thread.Sleep(1200);
        Console.Error.WriteLine("fake-err-2");
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
    Assert-True $capturedText.Contains('--probe-value|value with spaces') "space-containing argument was corrupted`n$capturedText"
    Assert-True $capturedText.Contains('[progress-warning]') "missing progress warning`n$capturedText"
    Assert-True $capturedText.Contains('[progress]') "missing progress summary`n$capturedText"
    Assert-True $capturedText.Contains('searches=12') "progress summary did not use runtime status`n$capturedText"
    Assert-True (Test-Path -LiteralPath (Join-Path $statePath 'jenkins.heartbeat')) 'heartbeat was not created'

    $stdoutLogs = @(Get-ChildItem -LiteralPath $logDirectory -Filter 'book-extender-*.stdout.log' -File)
    $stderrLogs = @(Get-ChildItem -LiteralPath $logDirectory -Filter 'book-extender-*.stderr.log' -File)
    Assert-True ($stdoutLogs.Count -eq 5) "expected five stdout logs, got $($stdoutLogs.Count)"
    Assert-True ($stderrLogs.Count -eq 5) "expected five stderr logs, got $($stderrLogs.Count)"
    Assert-True (($stdoutLogs | Get-Content -Raw) -join "`n").Contains('fake-out-1') 'durable stdout log missing fake output'
    Assert-True (($stderrLogs | Get-Content -Raw) -join "`n").Contains('fake-err-2') 'durable stderr log missing final output'

    Write-Output 'PASS: Jenkins wrapper durable log relay'
}
finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
    }
}