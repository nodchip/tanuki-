param(
    [int]$Moves = 40,
    [int]$Depth = 1,
    [int]$ByoyomiMs = 0
)

$project = "csharp/src/YaneuraOu.CSharp.Engine/YaneuraOu.CSharp.Engine.csproj"

$script = @(
    "usi",
    "isready",
    "usinewgame",
    "position startpos"
)

for ($i = 0; $i -lt $Moves; $i++) {
    if ($ByoyomiMs -gt 0) {
        $script += "go btime 0 wtime 0 byoyomi $ByoyomiMs"
    }
    else {
        $script += "go depth $Depth"
    }
}

$script += "quit"

$scriptText = ($script -join [Environment]::NewLine)
$output = $scriptText | dotnet run --project $project

$bestmoves = ($output | Select-String "^bestmove " | Measure-Object).Count
if ($bestmoves -lt $Moves) {
    Write-Error ("bestmove count mismatch. expected={0} actual={1}" -f $Moves, $bestmoves)
    exit 1
}

if ($output | Select-String -SimpleMatch "Exception") {
    Write-Error "exception text detected in engine output."
    exit 1
}

$output
Write-Host ("selfplay smoke success: moves={0} bestmoves={1} depth={2} byoyomiMs={3}" -f $Moves, $bestmoves, $Depth, $ByoyomiMs)
