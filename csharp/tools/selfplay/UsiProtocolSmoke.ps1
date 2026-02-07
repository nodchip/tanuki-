param(
    [int]$Cycles = 50,
    [int]$Depth = 1,
    [int]$ByoyomiMs = 300
)

$project = "csharp/src/YaneuraOu.CSharp.Engine/YaneuraOu.CSharp.Engine.csproj"

$script = @(
    "usi",
    "isready",
    "setoption name DebugLog value true",
    "usinewgame",
    "position startpos"
)

for ($i = 0; $i -lt $Cycles; $i++) {
    $script += "go infinite"
    $script += "stop"

    $script += "go ponder btime 1000 wtime 1000 byoyomi $ByoyomiMs"
    $script += "ponderhit"
    $script += "stop"

    $script += "go btime 0 wtime 0 byoyomi $ByoyomiMs depth $Depth"
    $script += "gameover draw"
    $script += "ucinewgame"
    $script += "position startpos"
}

$script += "quit"

$scriptText = ($script -join [Environment]::NewLine)
$output = $scriptText | dotnet run --project $project

$expectedBestMoves = $Cycles * 3
$bestmoves = ($output | Select-String "^bestmove " | Measure-Object).Count
if ($bestmoves -lt $expectedBestMoves) {
    Write-Error ("bestmove count mismatch. expected>={0} actual={1}" -f $expectedBestMoves, $bestmoves)
    exit 1
}

if ($output | Select-String -SimpleMatch "Exception") {
    Write-Error "exception text detected in engine output."
    exit 1
}

if ($output | Select-String -SimpleMatch "illegal bestmove filtered") {
    Write-Error "illegal bestmove detected during protocol smoke."
    exit 1
}

$output
Write-Host ("usi protocol smoke success: cycles={0} bestmoves={1} depth={2} byoyomiMs={3}" -f $Cycles, $bestmoves, $Depth, $ByoyomiMs)
