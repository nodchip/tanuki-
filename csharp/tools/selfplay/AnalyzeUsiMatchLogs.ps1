param(
    [Parameter(Mandatory = $false)]
    [string]$LogDir,

    [Parameter(Mandatory = $false)]
    [string[]]$LogFiles,

    [int]$OverrunGraceMs = 200,

    [string]$OutJson,

    [switch]$FailOnIllegalMove,

    [switch]$FailOnOverrun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-TargetFiles {
    param(
        [string]$LogDir,
        [string[]]$LogFiles
    )

    if ($LogFiles -and $LogFiles.Count -gt 0) {
        return $LogFiles | Where-Object { Test-Path $_ }
    }

    if ([string]::IsNullOrWhiteSpace($LogDir)) {
        throw "Specify LogDir or LogFiles."
    }

    if (-not (Test-Path $LogDir)) {
        throw "LogDir does not exist: $LogDir"
    }

    return Get-ChildItem -Path $LogDir -File | ForEach-Object { $_.FullName }
}

function Parse-ByoyomiFromGoLine {
    param([string]$Line)

    $m = [regex]::Match($Line, "\bbyoyomi\s+(\d+)")
    if ($m.Success) {
        return [int]$m.Groups[1].Value
    }

    return 0
}

$targets = @(Get-TargetFiles -LogDir $LogDir -LogFiles $LogFiles)
if ($targets.Count -eq 0) {
    throw "No log files to analyze."
}

$summary = [ordered]@{
    scanned_files = $targets.Count
    gameover_count = 0
    go_count = 0
    bestmove_count = 0
    resign_count = 0
    duplicate_bestmove_count = 0
    tm_ok_count = 0
    tm_fallback_count = 0
    illegal_move_count = 0
    overrun_suspected_count = 0
    max_info_time_ms = 0
    files = @()
}

foreach ($file in $targets) {
    $lines = Get-Content $file -Encoding UTF8

    $fileStat = [ordered]@{
        file = $file
        gameover_count = 0
        go_count = 0
        bestmove_count = 0
        resign_count = 0
        duplicate_bestmove_count = 0
        tm_ok_count = 0
        tm_fallback_count = 0
        illegal_move_count = 0
        overrun_suspected_count = 0
        max_info_time_ms = 0
    }

    $lastGoByoyomiMs = 0
    $lastGoSeen = $false
    $lastBestmove = ""

    foreach ($line in $lines) {
        if ($line -match "\bgameover\b") {
            $fileStat.gameover_count++
        }

        if ($line -match "^\s*[▶>]\s*go\b" -or $line -match "\bgo\s+") {
            $fileStat.go_count++
            $lastGoByoyomiMs = Parse-ByoyomiFromGoLine -Line $line
            $lastGoSeen = $true
            $lastBestmove = ""
        }

        if ($line -match "^\s*[◀<]\s*bestmove\s+(.+)$" -or $line -match "\bbestmove\s+(.+)$") {
            $move = $Matches[1].Trim()
            $fileStat.bestmove_count++

            if ($move -eq "resign") {
                $fileStat.resign_count++
            }

            if ($lastBestmove -eq $move -and $lastGoSeen) {
                $fileStat.duplicate_bestmove_count++
            }

            $lastBestmove = $move
            $lastGoSeen = $false
        }

        if ($line -match "info string tm status=ok") {
            $fileStat.tm_ok_count++
        }

        if ($line -match "info string tm status=fallback:") {
            $fileStat.tm_fallback_count++
        }

        if ($line -match "反則手:") {
            $fileStat.illegal_move_count++
        }

        if ($line -match "\binfo\s+depth\b.*\btime\s+(\d+)") {
            $timeMs = [int]$Matches[1]
            if ($timeMs -gt $fileStat.max_info_time_ms) {
                $fileStat.max_info_time_ms = $timeMs
            }

            if ($lastGoByoyomiMs -gt 0 -and $timeMs -gt ($lastGoByoyomiMs + $OverrunGraceMs)) {
                $fileStat.overrun_suspected_count++
            }
        }
    }

    $summary.gameover_count += $fileStat.gameover_count
    $summary.go_count += $fileStat.go_count
    $summary.bestmove_count += $fileStat.bestmove_count
    $summary.resign_count += $fileStat.resign_count
    $summary.duplicate_bestmove_count += $fileStat.duplicate_bestmove_count
    $summary.tm_ok_count += $fileStat.tm_ok_count
    $summary.tm_fallback_count += $fileStat.tm_fallback_count
    $summary.illegal_move_count += $fileStat.illegal_move_count
    $summary.overrun_suspected_count += $fileStat.overrun_suspected_count
    if ($fileStat.max_info_time_ms -gt $summary.max_info_time_ms) {
        $summary.max_info_time_ms = $fileStat.max_info_time_ms
    }

    $summary.files += [pscustomobject]$fileStat
}

$result = [pscustomobject]$summary
$result | ConvertTo-Json -Depth 6 | Write-Output

if (-not [string]::IsNullOrWhiteSpace($OutJson)) {
    $dir = Split-Path -Parent $OutJson
    if ($dir -and -not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir | Out-Null
    }

    $result | ConvertTo-Json -Depth 6 | Set-Content -Path $OutJson -Encoding UTF8
}

if ($FailOnIllegalMove -and $summary.illegal_move_count -gt 0) {
    throw "Illegal move detected. count=$($summary.illegal_move_count)"
}

if ($FailOnOverrun -and $summary.overrun_suspected_count -gt 0) {
    throw "Byoyomi overrun suspected. count=$($summary.overrun_suspected_count)"
}
