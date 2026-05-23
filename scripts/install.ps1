$ErrorActionPreference = "Stop"

$Repo   = "aodihis/tet"
$BinDir = "$env:USERPROFILE\.local\bin"

$Release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
$Version = $Release.tag_name

if (-not $Version) {
    Write-Error "Could not determine latest release version."
    exit 1
}

$Url = "https://github.com/$Repo/releases/download/$Version/tet-x86_64-pc-windows-msvc.exe"

New-Item -ItemType Directory -Force $BinDir | Out-Null
Invoke-WebRequest $Url -OutFile "$BinDir\tet.exe"

Write-Host ""
Write-Host "tet $Version installed to $BinDir\tet.exe"
Write-Host ""
Write-Host "Make sure $BinDir is in your PATH:"
Write-Host "  [Environment]::SetEnvironmentVariable('PATH', `$env:PATH + ';$BinDir', 'User')"
