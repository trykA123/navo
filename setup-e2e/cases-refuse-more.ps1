# Refusal cases S7, S10 and S14 of the installer's VM run (plan W15b), split from cases-refuse.ps1 for its size.
# S7 runs its checks as the admin a, elevated; S10 drives the fake payloads; S14 breaks one install per IW8 reason.

# Runs as the SSH admin before S7: gives a a fresh password for runas and lets a elevate without a prompt.
function Initialize-S7 {
    $folder = Join-Path $e2e 'out\S7'
    $bytes = New-Object byte[] 18
    [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    $password = 'Aa1!' + [Convert]::ToBase64String($bytes)
    Set-LocalUser -Name a -Password (ConvertTo-SecureString $password -AsPlainText -Force)
    [IO.File]::WriteAllText((Join-Path $folder 'a.secret'), $password)
    $policy = 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'
    Run-Checked reg.exe @('export', $policy, (Join-Path $folder 'uac.reg'), '/y') | Out-Null
    $before = (Get-ItemProperty -LiteralPath "Registry::$policy").ConsentPromptBehaviorAdmin
    [IO.File]::WriteAllText((Join-Path $folder 'uac-before.txt'), "$before")
    Set-ItemProperty -LiteralPath "Registry::$policy" -Name ConsentPromptBehaviorAdmin -Value 0
}

# Returns the arguments that start this script's S7 helper step.
function Get-S7HelperArguments {
    Quote-Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $e2e 'scripts\setup-e2e.ps1'), '-Case', 'S7', '-Helper')
}

# S7: as a, elevated, silent exits 7 and the window shows the Elevated page; the checks run in Invoke-S7Helper.
function Test-S7 {
    $secret = Join-Path $script:caseDir 'a.secret'
    try {
        $password = ConvertTo-SecureString ([IO.File]::ReadAllText($secret)) -AsPlainText -Force
        $credential = New-Object Management.Automation.PSCredential('a', $password)
        Start-Process -FilePath $script:powershell -ArgumentList (Get-S7HelperArguments) -Credential $credential -LoadUserProfile -WorkingDirectory $e2e
    } finally { Remove-Item -LiteralPath $secret -ErrorAction SilentlyContinue }
    $result = Join-Path $script:caseDir 'helper-result.txt'
    Wait-Until { Test-Path -LiteralPath $result } 300 'the elevated helper wrote no result'
    $line = [IO.File]::ReadAllText($result).Trim()
    Assert ($line -eq 'OK') "elevated helper: $line"
    $policy = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'
    $before = [IO.File]::ReadAllText((Join-Path $script:caseDir 'uac-before.txt')).Trim()
    Assert ("$($policy.ConsentPromptBehaviorAdmin)" -eq $before) 'ConsentPromptBehaviorAdmin was not restored'
    Assert (-not (Test-Path -LiteralPath (Get-DefaultRoot))) "u's default root exists"
}

# The S7 helper as a: unelevated, it restarts itself elevated; elevated, it checks S7 and writes helper-result.txt.
function Invoke-S7Helper {
    $script:caseDir = Join-Path $e2e 'out\S7'
    $script:WorkDir = $script:caseDir
    $result = Join-Path $script:caseDir 'helper-result.txt'
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        try { Start-Process -FilePath $script:powershell -ArgumentList (Get-S7HelperArguments) -Verb RunAs -WorkingDirectory $e2e }
        catch { [IO.File]::WriteAllText($result, "FAIL: the elevated start failed: $($_.Exception.Message)") }
        return
    }
    $line = 'OK'
    try { Test-S7Elevated }
    catch { $line = "FAIL: $($_.Exception.Message)"; Write-Evidence ($_ | Out-String) }
    try { Run-Checked reg.exe @('import', (Join-Path $script:caseDir 'uac.reg')) | Out-Null }
    catch { if ($line -eq 'OK') { $line = "FAIL: restoring ConsentPromptBehaviorAdmin: $($_.Exception.Message)" } }
    $logs = Join-Path $env:TEMP 'NavoSetup-logs'
    if (Test-Path -LiteralPath $logs) { Copy-Item -LiteralPath $logs -Destination (Join-Path $script:caseDir 'wizard-logs-a') -Recurse -Force }
    [IO.File]::WriteAllText($result, $line)
}

# Saves a PNG of a window's bounds from the screen into the case folder.
function Save-WindowShot([IntPtr]$window, [string]$name) {
    Add-Type -AssemblyName System.Drawing
    [NavoE2E.Native]::Focus($window) | Out-Null
    Start-Sleep -Seconds 1
    $box = [Windows.Automation.AutomationElement]::FromHandle($window).Current.BoundingRectangle
    $bitmap = New-Object Drawing.Bitmap([int]$box.Width, [int]$box.Height)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen([int]$box.X, [int]$box.Y, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $script:caseDir $name), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    Write-Evidence "screenshot $name"
}

# S7's checks, run elevated as a: silent exit 7 with nothing created, then the Elevated page.
function Test-S7Elevated {
    Assert ([NavoE2E.Native]::Elevated($PID)) 'the helper is not elevated'
    $before = Get-Entries $env:LOCALAPPDATA
    $result = Invoke-Silent '0.0.2'
    Assert ($result.Code -eq 7) "elevated silent exit $($result.Code), not 7: $($result.Error)"
    Assert ($result.Seen.Count -eq 0) "an installer process started: $($result.Seen -join '; ')"
    $new = @(Get-Entries $env:LOCALAPPDATA | Where-Object { $before -notcontains $_ -and $_ -match 'navo|Programs' })
    Assert ($new.Count -eq 0) "created: $($new -join ', ')"
    Assert (-not (Test-Path -LiteralPath (Get-DefaultRoot))) "$(Get-DefaultRoot) exists"
    $wizard = Start-Process -FilePath (Join-Path $wizards '0.0.2\Navo-Setup.exe') -PassThru
    try {
        $window = Wait-MainWindow $wizard
        Wait-UiText $window 'Started as administrator' 30
        Save-WindowShot $window 'S7-elevated-page.png'
    } finally { if (-not $wizard.HasExited) { $wizard.Kill() } }
    Assert (-not (Test-Path -LiteralPath (Get-DefaultRoot))) "the window created $(Get-DefaultRoot)"
}

# Returns the names of this user's wizard logs.
function Get-LogNames {
    @(Get-Entries (Join-Path $env:TEMP 'NavoSetup-logs'))
}

# Runs a fake Setup.exe silently into a custom root; records exit 5, the printed log path, the new log and one Setup.exe.
function Invoke-Fake([string]$fake, [string]$root) {
    $before = Get-LogNames
    $result = Invoke-Silent '0.0.2' @('--installto', $root) $null 120 (Join-Path $e2e "fakes\$fake")
    Add-Fault ($result.Code -eq 5) "$fake exit $($result.Code), not 5: $($result.Error.Trim())"
    Add-Fault ($result.Error -match 'NavoSetup-logs\\') "$fake printed no log path: $($result.Error.Trim())"
    Add-Fault (@(Get-LogNames | Where-Object { $before -notcontains $_ }).Count -gt 0) "$fake left no new log file"
    $setups = @($result.Seen | Where-Object { $_ -match '^(NavoApp-win-)?Setup\.exe ' })
    Add-Fault ($setups.Count -le 1) "$fake ran $($setups.Count) Setup.exe processes"
    $result
}

# From Welcome to the Failed page of a fresh install; records whether Back stayed off while the fake ran.
function Step-ToFailed($wizard) {
    Step-ToFolder $wizard 'Install Navo 0\.0\.2'
    Step-Page $wizard.Window 'n' 'Start menu' 60
    Send-Alt $wizard.Window 'i'
    $script:backStates = @()
    Wait-Until {
        $text = Get-UiText $wizard.Window
        if ($text -match 'Setup could not finish') { return $true }
        # A lost Alt+I leaves the Shortcuts page up; press again, as Step-Page does.
        if ($text -match 'Start menu') { Send-Alt $wizard.Window 'i'; Start-Sleep -Seconds 2; return $false }
        if ($text -notmatch 'Installing Navo') { return $false }
        $back = @(Get-UiElements $wizard.Window '^Back')
        $script:backStates += ($back.Count -gt 0 -and $back[0].Current.IsEnabled)
        $false
    } 120 'the Failed page did not show'
    Write-Evidence "Back while the fake ran: $($script:backStates -join ', ')"
    Add-Fault ($script:backStates.Count -gt 0) 'the Installing page was never sampled'
    Add-Fault ($script:backStates -notcontains $true) 'Back was on while the fake ran'
}

# Closes the wizard with Alt+C, pressed again every 3 s like Step-Page, and waits up to 30 s for it to exit.
function Close-Wizard($wizard) {
    $script:closePresses = 0
    try {
        Wait-Until {
            if ($wizard.Process.HasExited) { return $true }
            Send-Alt $wizard.Window 'c'
            $script:closePresses++
            $wizard.Process.WaitForExit(3000)
        } 30 'the wizard did not exit after Close'
    } catch {
        # Evidence only: a UIA press of Close tells lost keys from a Close that does nothing.
        foreach ($button in @(Get-UiElements $wizard.Window '^Close$')) { Invoke-Element $button }
        Write-Evidence "after $($script:closePresses) Alt+C presses and a UIA press, exited: $($wizard.Process.WaitForExit(10000))"
        throw
    }
    Write-Evidence "Close took $($script:closePresses) Alt+C presses"
}

# S10 part a: fail-empty leaves nothing behind, silent and after Close in the window.
function Test-S10a {
    $root = Join-Path $env:USERPROFILE 'FailEmpty\Navo'
    Invoke-Fake 'fail-empty.exe' $root | Out-Null
    Add-Fault (-not (Test-Path -LiteralPath (Split-Path $root))) "silent fail-empty left $(Split-Path $root)"
    $wizard = Start-Wizard 'fail-empty'
    Step-ToFailed $wizard
    Close-Wizard $wizard
    Add-Fault (-not (Test-Path -LiteralPath (Get-DefaultRoot))) 'Close kept the empty default root'
    Write-Evidence "Programs folder after Close: $(Test-Path -LiteralPath (Split-Path (Get-DefaultRoot)))"
    Assert-Faults
}

# S10 part b: fail-partial keeps and names its root, with Retry off, silent and in the window.
function Test-S10b {
    $root = Join-Path $env:USERPROFILE 'FailPart\Navo'
    $result = Invoke-Fake 'fail-partial.exe' $root
    Add-Fault ($result.Error -match 'Setup left files in') "silent fail-partial did not name its root: $($result.Error.Trim())"
    Add-Fault (Test-Path -LiteralPath (Join-Path $root 'partial.txt')) 'silent fail-partial did not keep partial.txt'
    $wizard = Start-Wizard 'fail-partial'
    Step-ToFailed $wizard
    $text = Get-UiText $wizard.Window
    Write-Evidence "Failed page:`n$text"
    Add-Fault ($text -match ('Setup left files in\s*`?' + [regex]::Escape((Get-DefaultRoot)))) 'the Failed page does not name the root'
    $retry = @(Get-UiElements $wizard.Window '^Retry')
    Add-Fault ($retry.Count -eq 0 -or -not $retry[0].Current.IsEnabled) 'Retry is on for a root with files'
    Close-Wizard $wizard
    Add-Fault (Test-Path -LiteralPath (Join-Path (Get-DefaultRoot) 'partial.txt')) 'Close deleted the partial root'
    Assert-Faults
}

# S10 part c: an Update.exe apply that exits 1 keeps 0.0.2, the root and the marker; the Failed page names both logs.
function Test-S10c {
    $root = Install-Default
    $id = [NavoE2E.Native]::FileId($root)
    $installId = (Read-Marker $root).installId
    $package = @(Get-ChildItem -LiteralPath (Join-Path $e2e 'fakes\fail-apply') -Filter '*-full.nupkg')
    Assert ($package.Count -eq 1) 'no fail-apply package in the base; rebuild it'
    $result = Invoke-Silent '0.0.3' @() $null 300 '' $package[0].FullName
    Add-Fault ($result.Code -eq 5) "fail-apply exit $($result.Code), not 5: $($result.Error.Trim())"
    $wizard = Start-Wizard 'fail-apply'
    Step-ToFolder $wizard 'Install Navo 0\.0\.3'
    Step-Page $wizard.Window 'i' 'Setup could not finish' 300
    $text = Get-UiText $wizard.Window
    Write-Evidence "Failed page:`n$text"
    $logs = @([regex]::Matches($text, '[^\s`]+\.log') | ForEach-Object { $_.Value } | Sort-Object -Unique)
    Add-Fault (@($logs | Where-Object { $_ -match '-velopack\.log$' }).Count -ge 1) 'the Failed page names no Velopack log'
    Add-Fault (@($logs | Where-Object { $_ -notmatch '-velopack\.log$' }).Count -ge 1) 'the Failed page names no wizard log'
    Close-Wizard $wizard
    Add-Fault ((Read-SqVersion $root) -eq '0.0.2') 'sq.version is not 0.0.2'
    Add-Fault ([NavoE2E.Native]::FileId($root) -eq $id) 'the root was replaced'
    Add-Fault ((Read-Marker $root).installId -eq $installId) 'the marker changed'
    Assert-Faults
}

# Opens an embedded wizard on an Ambiguous install, records its page and checks the text kind.
function Assert-AmbiguousPage([string]$version, [bool]$unsafe) {
    $wizard = Start-Wizard $version
    try {
        Wait-UiText $wizard.Window 'Setup cannot continue' 60
        $text = Get-UiText $wizard.Window
        Write-Evidence "Ambiguous page:`n$text"
        if ($unsafe) {
            Add-Fault ($text -match 'cannot update a folder whose name contains') 'the UnsafeName text is missing'
            Add-Fault ($text -notmatch 'uninstall') 'the UnsafeName text mentions uninstalling'
        } else { Add-Fault ($text -match 'Repair by uninstalling') 'the Ambiguous text is missing' }
        try { Close-Wizard $wizard } catch { Add-Fault $false "Ambiguous page: $($_.Exception.Message)" }
    } finally { if (-not $wizard.Process.HasExited) { $wizard.Process.Kill() } }
}

# Breaks the install one way, expects exit 6 with the folders unchanged, mends it, then expects exit 8 (the control).
function Test-Ambiguous([string]$name, [string[]]$roots, [scriptblock]$break, [scriptblock]$mend, [bool]$window = $false) {
    & $break
    try {
        $before = @(foreach ($folder in $roots) { Get-HashListing $folder })
        $result = Invoke-Silent '0.0.3'
        Add-Fault ($result.Code -eq 6) "${name}: exit $($result.Code), not 6: $($result.Error.Trim())"
        Add-Fault ($result.Seen.Count -eq 0) "${name}: an installer process started"
        Add-Fault ((Compare-Object $before @(foreach ($folder in $roots) { Get-HashListing $folder })) -eq $null) "${name}: a folder changed"
        if ($window) { Assert-AmbiguousPage '0.0.3' $false }
    } finally { & $mend }
    $control = Invoke-Silent '0.0.2'
    Add-Fault ($control.Code -eq 8) "${name}: after the repair, wizard 0.0.2 exited $($control.Code), not 8"
}

# S14 part a: one wizard install broken five ways in turn: missing folder, foreign id, damaged sq.version, no marker, junction.
function Test-S14a {
    $root = Install-Default
    $location = (Get-ItemProperty -LiteralPath $script:uninstallKey).InstallLocation
    $version = Join-Path $root 'current\sq.version'
    $saved = [IO.File]::ReadAllBytes($version)
    $foreign = [Text.Encoding]::UTF8.GetString($saved) -replace '<id>NavoApp</id>', '<id>Other</id>'
    Assert ($foreign -match '<id>Other</id>') 'sq.version has no <id>NavoApp</id> to replace'
    $marker = Join-Path $root 'navo-install.json'
    $link = Join-Path $env:USERPROFILE 'JL'
    $restore = { Set-ItemProperty -LiteralPath $script:uninstallKey -Name InstallLocation -Value $location }
    Test-Ambiguous 'missing folder' @($root) { Set-ItemProperty -LiteralPath $script:uninstallKey -Name InstallLocation -Value (Join-Path $env:USERPROFILE 'Missing\Navo') } $restore
    Test-Ambiguous 'id Other' @($root) { [IO.File]::WriteAllText($version, $foreign) } { [IO.File]::WriteAllBytes($version, $saved) }
    Test-Ambiguous 'damaged sq.version' @($root) { [IO.File]::WriteAllText($version, 'damaged') } { [IO.File]::WriteAllBytes($version, $saved) } $true
    Test-Ambiguous 'no marker' @($root) { Move-Item -LiteralPath $marker -Destination (Join-Path $script:caseDir 'marker.json') } { Move-Item -LiteralPath (Join-Path $script:caseDir 'marker.json') -Destination $marker }
    Test-Ambiguous 'junction in the chain' @($root) {
        New-Item -ItemType Junction -Path $link -Target (Split-Path $root) | Out-Null
        Set-ItemProperty -LiteralPath $script:uninstallKey -Name InstallLocation -Value (Join-Path $link 'Navo')
    } { & $restore; [IO.Directory]::Delete($link) }
    Assert-Faults
}

# S14 part b: the registry's folder and a valid legacy folder at once.
function Test-S14b {
    $root = Install-Default
    $legacy = Join-Path $env:LOCALAPPDATA 'NavoApp'
    $saved = Join-Path $script:caseDir 'uninstall-key.reg'
    Run-Checked reg.exe @('export', 'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\NavoApp', $saved, '/y') | Out-Null
    Install-Raw (Get-PackFile '0.0.1' '*Setup.exe') '0.0.1'
    # The raw Setup.exe points the key at the legacy root; putting it back leaves two valid folders.
    Run-Checked reg.exe @('import', $saved) | Out-Null
    Assert ((Get-ItemProperty -LiteralPath $script:uninstallKey).InstallLocation.TrimEnd('\') -eq $root) 'the key does not name the wizard root'
    $before = @((Get-HashListing $root) + (Get-HashListing $legacy))
    $result = Invoke-Silent '0.0.3'
    Assert ($result.Code -eq 6) "exit $($result.Code), not 6: $($result.Error.Trim())"
    Assert ($result.Seen.Count -eq 0) "an installer process started: $($result.Seen -join '; ')"
    Assert ((Compare-Object $before @((Get-HashListing $root) + (Get-HashListing $legacy))) -eq $null) 'a folder changed'
}

# S14 part c: a raw Setup.exe install under R&D is Ambiguous with the UnsafeName text, which never mentions uninstalling.
function Test-S14c {
    $root = Join-Path $env:USERPROFILE 'R&D\Navo'
    Run-Checked (Get-PackFile '0.0.2' '*Setup.exe') @('--silent', '--installto', $root) 300 | Out-Null
    Wait-Until { Test-Path -LiteralPath (Join-Path $root 'current\navo.exe') } 60 'Setup did not install into R&D'
    Stop-Root $root
    $before = Get-HashListing $root
    $result = Invoke-Silent '0.0.3'
    Add-Fault ($result.Code -eq 6) "exit $($result.Code), not 6: $($result.Error.Trim())"
    Add-Fault ($result.Seen.Count -eq 0) "an installer process started: $($result.Seen -join '; ')"
    Add-Fault ($result.Error -notmatch 'uninstall') "the silent UnsafeName text mentions uninstalling: $($result.Error.Trim())"
    Assert-AmbiguousPage '0.0.3' $true
    Add-Fault ((Compare-Object $before (Get-HashListing $root)) -eq $null) 'the R&D root changed'
    Assert-Faults
}
