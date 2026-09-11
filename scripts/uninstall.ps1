[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Nebula"),
    [switch]$KeepPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-NormalizedPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    try {
        return [IO.Path]::GetFullPath($Path).TrimEnd('\').ToLowerInvariant()
    }
    catch {
        return $Path.TrimEnd('\').ToLowerInvariant()
    }
}

$exe = Join-Path $InstallDir "Nebula.exe"
if (Test-Path $exe) {
    Remove-Item $exe -Force
    Write-Host "Removed $exe" -ForegroundColor Green
}
else {
    Write-Host "Nebula.exe was not found in $InstallDir."
}

if (-not $KeepPath) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not [string]::IsNullOrWhiteSpace($userPath)) {
        $target = Get-NormalizedPath $InstallDir
        $entries = @($userPath.Split(';') | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
        $kept = @($entries | Where-Object { (Get-NormalizedPath $_) -ne $target })

        if ($kept.Count -ne $entries.Count) {
            [Environment]::SetEnvironmentVariable("Path", ($kept -join ';'), "User")
            Write-Host "Removed Nebula from the user PATH." -ForegroundColor Green
        }
    }
}

if (Test-Path $InstallDir) {
    $remaining = @(Get-ChildItem -LiteralPath $InstallDir -Force)
    if ($remaining.Count -eq 0) {
        Remove-Item $InstallDir -Force
    }
}

Write-Host "Nebula uninstall complete."
