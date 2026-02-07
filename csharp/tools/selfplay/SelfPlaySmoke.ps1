param(
    [int]$Moves = 40
)

$project = "csharp/src/YaneuraOu.CSharp.Engine/YaneuraOu.CSharp.Engine.csproj"

$script = @(
    "usi",
    "isready",
    "usinewgame",
    "position startpos",
    "go depth 1"
)

for ($i = 0; $i -lt $Moves; $i++) {
    $script += "go depth 1"
}

$script += "quit"

$scriptText = ($script -join [Environment]::NewLine)
$scriptText | dotnet run --project $project
