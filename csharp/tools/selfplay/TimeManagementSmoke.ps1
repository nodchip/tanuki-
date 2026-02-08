param(
    [ValidateSet("Custom", "ShogiHomeByoyomi3s", "ShogidokoroFischer")]
    [string]$Profile = "Custom",
    [int]$ByoyomiMs = 3000,
    [int]$BTimeMs = 15000,
    [int]$WTimeMs = 15000,
    [int]$IncMs = 0
)

$project = "csharp/src/YaneuraOu.CSharp.Engine/YaneuraOu.CSharp.Engine.csproj"

switch ($Profile) {
    "ShogiHomeByoyomi3s" {
        $ByoyomiMs = 3000
        $BTimeMs = 0
        $WTimeMs = 0
        $IncMs = 0
    }
    "ShogidokoroFischer" {
        $ByoyomiMs = 0
        $BTimeMs = 300000
        $WTimeMs = 300000
        $IncMs = 5000
    }
}

$goPrimary = "go btime $BTimeMs wtime $WTimeMs binc $IncMs winc $IncMs byoyomi $ByoyomiMs"
$goSecondary = if ($ByoyomiMs -gt 0) {
    "go btime 0 wtime 0 byoyomi $ByoyomiMs"
} else {
    "go btime $BTimeMs wtime $WTimeMs binc $IncMs winc $IncMs"
}

$script = @(
    "usi",
    "isready",
    "setoption name DebugLog value true",
    "usinewgame",
    "position startpos",
    $goPrimary,
    $goSecondary,
    "setoption name MoveOverhead value 5000",
    "go byoyomi 1000",
    "quit"
)

$scriptText = ($script -join [Environment]::NewLine)
$output = $scriptText | dotnet run --project $project

$tmOk = ($output | Select-String -SimpleMatch "info string tm status=ok" | Measure-Object).Count
if ($tmOk -lt 2) {
    Write-Error ("tm status=ok count mismatch. expected>=2 actual={0}" -f $tmOk)
    exit 1
}

$tmFallback = ($output | Select-String -SimpleMatch "info string tm status=fallback:" | Measure-Object).Count
if ($tmFallback -lt 1) {
    Write-Error "tm fallback log was not emitted."
    exit 1
}

$bestmoves = ($output | Select-String "^bestmove " | Measure-Object).Count
if ($bestmoves -lt 3) {
    Write-Error ("bestmove count mismatch. expected>=3 actual={0}" -f $bestmoves)
    exit 1
}

if ($output | Select-String "Unhandled exception") {
    Write-Error "unhandled exception detected in engine output."
    exit 1
}

$output
Write-Host ("time management smoke success: profile={0} byoyomiMs={1} btime={2} wtime={3} inc={4} tm_ok={5} tm_fallback={6}" -f $Profile, $ByoyomiMs, $BTimeMs, $WTimeMs, $IncMs, $tmOk, $tmFallback)
