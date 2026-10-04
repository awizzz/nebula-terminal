# Writes the Scoop manifest for one release. The Release workflow publishes it next to
# the installers, so this address always installs the latest version:
#   scoop install https://github.com/awizzz/nebula-terminal/releases/latest/download/nebula-terminal.json
# Scoop keeps that address and reads it again on `scoop update`.
param(
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [string] $Zip,
    [Parameter(Mandatory)] [string] $Repository,
    [Parameter(Mandatory)] [string] $Out
)

$ErrorActionPreference = 'Stop'

$manifest = [ordered]@{
    version      = $Version
    description  = 'A fast, good-looking Windows terminal with its own Linux-style shell.'
    homepage     = 'https://nebula.awizz.space'
    license      = [ordered]@{
        identifier = 'PolyForm-Shield-1.0.0'
        url        = "https://github.com/$Repository/blob/main/LICENSE"
    }
    notes        = 'Settings > Behavior can add Open in Nebula Terminal to File Explorer and make it the default terminal of Windows 11.'
    architecture = [ordered]@{
        '64bit' = [ordered]@{
            url  = "https://github.com/$Repository/releases/download/v$Version/$(Split-Path $Zip -Leaf)"
            hash = (Get-FileHash $Zip -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    # The leading commas keep each pair as one entry instead of two.
    bin          = @(, @('Nebula Terminal.exe', 'nebula-terminal')) + @('nebula-sh.exe')
    shortcuts    = @(, @('Nebula Terminal.exe', 'Nebula Terminal'))
}

$manifest | ConvertTo-Json -Depth 5 | Set-Content $Out -Encoding utf8NoBOM
