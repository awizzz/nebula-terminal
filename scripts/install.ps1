[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Nebula"),
    [switch]$NoPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Repository = "awizzz/nebula-shell"
$Headers = @{ "User-Agent" = "Nebula-Installer" }

function Get-NormalizedPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    try {
        return [IO.Path]::GetFullPath($Path).TrimEnd('\').ToLowerInvariant()
    }
    catch {
        return $Path.TrimEnd('\').ToLowerInvariant()
    }
}

function Get-Release {
    if ($Version -eq "latest") {
        $uri = "https://api.github.com/repos/$Repository/releases/latest"
    }
    else {
        $tag = if ($Version.StartsWith("v")) { $Version } else { "v$Version" }
        $uri = "https://api.github.com/repos/$Repository/releases/tags/$tag"
    }

    return Invoke-RestMethod -Uri $uri -Headers $Headers
}

Write-Host "Nebula installer" -ForegroundColor Cyan
Write-Host "Resolving release..."

$release = Get-Release
$exeAsset = $release.assets | Where-Object { $_.name -eq "Nebula.exe" } | Select-Object -First 1
$hashAsset = $release.assets | Where-Object { $_.name -eq "Nebula.exe.sha256" } | Select-Object -First 1

if (-not $exeAsset -or -not $hashAsset) {
    throw "The selected release does not contain Nebula.exe and Nebula.exe.sha256."
}

$tempDir = Join-Path ([IO.Path]::GetTempPath()) ("nebula-install-" + [guid]::NewGuid().ToString("N"))
$downloadedExe = Join-Path $tempDir "Nebula.exe"
$downloadedHash = Join-Path $tempDir "Nebula.exe.sha256"

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    Write-Host "Downloading $($release.tag_name)..."
    Invoke-WebRequest -Uri $exeAsset.browser_download_url -Headers $Headers -OutFile $downloadedExe -UseBasicParsing
    Invoke-WebRequest -Uri $hashAsset.browser_download_url -Headers $Headers -OutFile $downloadedHash -UseBasicParsing

    $expectedHash = ((Get-Content $downloadedHash -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    $actualHash = (Get-FileHash $downloadedExe -Algorithm SHA256).Hash.ToLowerInvariant()

    if ($expectedHash -ne $actualHash) {
        throw "SHA-256 verification failed. Expected $expectedHash but downloaded $actualHash."
    }

    Write-Host "SHA-256 verified." -ForegroundColor Green

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $destination = Join-Path $InstallDir "Nebula.exe"
    Copy-Item $downloadedExe $destination -Force

    if (-not $NoPath) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $entries = @()
        if (-not [string]::IsNullOrWhiteSpace($userPath)) {
            $entries = @($userPath.Split(';') | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
        }

        $normalizedInstallDir = Get-NormalizedPath $InstallDir
        $alreadyOnPath = $false
        foreach ($entry in $entries) {
            if ((Get-NormalizedPath $entry) -eq $normalizedInstallDir) {
                $alreadyOnPath = $true
                break
            }
        }

        if (-not $alreadyOnPath) {
            $newUserPath = (@($entries) + $InstallDir) -join ';'
            [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
            $env:Path = "$env:Path;$InstallDir"
            Write-Host "Added Nebula to the user PATH." -ForegroundColor Green
        }
    }

    Write-Host "Installed to $destination" -ForegroundColor Green
    & $destination --version
    Write-Host "Open a new terminal and run 'nebula'."
}
finally {
    Remove-Item $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}
