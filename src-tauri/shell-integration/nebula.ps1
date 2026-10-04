# Nebula Terminal shell integration for PowerShell 5.1 and 7.
#
# Runs after your profile. It marks each prompt and command (OSC 133), which
# command jumps and notifications use, and reports the current folder (OSC 7) for
# new tabs and splits. A prompt that already prints its own marks keeps them.

if (-not $global:__NebulaShell) {
    try {
        $global:__NebulaShell = @{ Prompt = $function:prompt; ReadLine = $null; Running = $false; Marks = $true }

        function global:prompt {
            $success = $global:?
            $shell = $global:__NebulaShell
            $esc = [char]27
            $bel = [char]7
            $out = ''
            if ($shell.Running) {
                $code = 0
                if (-not $success) {
                    $code = 1
                    if ($global:LASTEXITCODE) { $code = $global:LASTEXITCODE }
                }
                $out += "$esc]133;D;$code$bel"
                $shell.Running = $false
            }
            if ($PWD.Provider.Name -eq 'FileSystem' -and $PWD.ProviderPath -notmatch '[\x00-\x1f]') {
                $folder = $PWD.ProviderPath.Replace('%', '%25').Replace('\', '/')
                if ($folder.StartsWith('//')) { $out += "$esc]7;file:$folder$bel" }
                else { $out += "$esc]7;file://$env:COMPUTERNAME/$folder$bel" }
            }
            if ($shell.Marks) { $out += "$esc]133;A$bel" }
            [Console]::Write($out)

            # Hand the original prompt the same $? it would have seen.
            if (-not $success) { Write-Error 'failure' -ErrorAction Ignore }
            $text = & $shell.Prompt
            if ($shell.Marks -and (@($text) -join '').Contains("$esc]133;")) { $shell.Marks = $false }
            $text
        }

        if (Get-Command PSConsoleHostReadLine -CommandType Function -ErrorAction Ignore) {
            $global:__NebulaShell.ReadLine = $function:PSConsoleHostReadLine
            function global:PSConsoleHostReadLine {
                $shell = $global:__NebulaShell
                $line = & $shell.ReadLine
                if ($shell.Marks -and -not [string]::IsNullOrWhiteSpace($line)) {
                    $shell.Running = $true
                    [Console]::Write("$([char]27)]133;C$([char]7)")
                }
                $line
            }
        }
    } catch {
    }
}
