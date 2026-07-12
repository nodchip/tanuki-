param(
    [Parameter(Mandatory = $true)]
    [string]$StateDir
)
$ErrorActionPreference = 'Stop'
$statePath = [System.IO.Path]::GetFullPath($StateDir)
[System.IO.Directory]::CreateDirectory($statePath) | Out-Null
[System.IO.File]::WriteAllText((Join-Path $statePath 'stop.request'), [DateTimeOffset]::UtcNow.ToString('O'))