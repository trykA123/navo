# UI and network cases S11 and S12 of the installer's VM run (plan W15d).
# S11 installs with all outbound traffic blocked; S12 walks the rehearsal wizard with keys in both themes.

$script:THEME_KEY = 'HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
$script:ARTIFACT = 'C:\e2e\artifact\Navo-Setup.exe'
$script:NAMED_TYPES = 'Button|CheckBox|Edit|Hyperlink|RadioButton|ComboBox|ListItem|TabItem|Slider|MenuItem'

# Returns whether a TCP connection to the address and port opens within three seconds.
function Test-Outbound([string]$address, [int]$port) {
    $client = New-Object Net.Sockets.TcpClient
    try { $client.ConnectAsync($address, $port).Wait(3000) -and $client.Connected }
    catch { $false }
    finally { $client.Dispose() }
}

# Runs as the SSH admin before S11: saves each profile's outbound default, then blocks all outbound traffic.
function Initialize-S11 {
    $folder = Join-Path $e2e 'out\S11'
    $saved = @(Get-NetFirewallProfile | ForEach-Object { @{ Name = $_.Name; Action = $_.DefaultOutboundAction.ToString() } })
    [IO.File]::WriteAllText((Join-Path $folder 'firewall.json'), (ConvertTo-Json -InputObject $saved), (New-Object Text.UTF8Encoding($false)))
    Add-Content -LiteralPath (Join-Path $folder 'evidence.txt') -Value "before: outbound to 1.1.1.1:443 opens=$(Test-Outbound '1.1.1.1' 443); saved $(($saved | ForEach-Object { $_.Name + '=' + $_.Action }) -join ', ')"
    Set-NetFirewallProfile -All -DefaultOutboundAction Block
}

# Runs as the SSH admin after S11: puts each profile's saved outbound default back and checks it.
function Finalize-S11 {
    $saved = [IO.File]::ReadAllText((Join-Path $e2e 'out\S11\firewall.json')) | ConvertFrom-Json
    foreach ($profile in @($saved)) { Set-NetFirewallProfile -Name $profile.Name -DefaultOutboundAction $profile.Action }
    foreach ($profile in @($saved)) {
        $now = (Get-NetFirewallProfile -Name $profile.Name).DefaultOutboundAction.ToString()
        Assert ($now -eq $profile.Action) "profile $($profile.Name) is $now, not $($profile.Action)"
    }
}

# S11: with every outbound connection blocked, the S1a install still exits 0 and the SSH session survives.
function Test-S11 {
    $open = Test-Outbound '1.1.1.1' 443
    Write-Evidence "during the block: outbound to 1.1.1.1:443 opens=$open"
    Assert (-not $open) 'outbound traffic is not blocked'
    Test-S1a
    Write-Evidence 'S1a passed offline; the host kept polling this VM over SSH while the block was on'
}

# Sends one key to the window after giving it focus; throws when it cannot take focus.
function Send-Key([IntPtr]$window, [uint16]$key) {
    Wait-Until { [NavoE2E.Native]::Focus($window) } 10 'the window did not take focus'
    [NavoE2E.Native]::Press($key)
}

# Writes setup-uia-<theme>-<page>.json (Name, ControlType, AccessKey, IsEnabled, toggle state) and records unnamed controls.
function Save-UiDump([IntPtr]$window, [string]$theme, [string]$page) {
    $root = [Windows.Automation.AutomationElement]::FromHandle($window)
    $all = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    $rows = @(foreach ($element in $all) {
        $c = $element.Current
        $toggle = $null
        $pattern = $null
        if ($element.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) { $toggle = $pattern.Current.ToggleState.ToString() }
        $type = $c.ControlType.ProgrammaticName
        Add-Fault (($type -notmatch "ControlType\.($script:NAMED_TYPES)$") -or $c.Name) "$theme/$page`: a $type has no name"
        Add-Fault ([bool]$type) "$theme/$page`: an element has no control type"
        [ordered]@{ Name = $c.Name; ControlType = $type; AccessKey = $c.AccessKey; IsEnabled = $c.IsEnabled; Toggle = $toggle }
    })
    $file = Join-Path $script:caseDir "setup-uia-$theme-$page.json"
    [IO.File]::WriteAllText($file, (ConvertTo-Json -InputObject $rows -Depth 4), (New-Object Text.UTF8Encoding($false)))
    Write-Evidence "uia dump $theme/$page`: $($rows.Count) elements"
}

# Dumps the page and shoots the window: the two files of one page in one theme.
function Save-Page([IntPtr]$window, [string]$theme, [string]$page) {
    Save-UiDump $window $theme $page
    Save-WindowShot $window "S12-$theme-$page.png"
}

# Presses Tab until the focused element's name matches a regex, then returns it; records each stop.
function Tab-To([IntPtr]$window, [string]$pattern) {
    for ($i = 1; $i -le 12; $i++) {
        Send-Key $window 0x09
        Start-Sleep -Milliseconds 300
        $focused = [Windows.Automation.AutomationElement]::FocusedElement
        Write-Evidence "Tab $i focus: '$($focused.Current.Name)' $($focused.Current.ControlType.ProgrammaticName)"
        if ($focused.Current.Name -match $pattern) { return $focused }
    }
    $null
}

# Returns a checkbox's toggle state as text, or the empty string when it has none.
function Get-ToggleState($element) {
    $pattern = $null
    if ($element.TryGetCurrentPattern([Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) { return $pattern.Current.ToggleState.ToString() }
    ''
}

# Ticks the licence box with Tab and Space; falls back to its access key and records a fault when Tab never reaches it.
function Accept-Licence([IntPtr]$window, [string]$theme) {
    $box = Tab-To $window 'accept'
    if ($box) {
        Send-Key $window 0x20
        Start-Sleep -Milliseconds 300
        Add-Fault ((Get-ToggleState $box) -eq 'On') "$theme`: Space did not tick the licence box"
    } else {
        Add-Fault $false "$theme`: Tab did not reach the licence box"
        Send-Alt $window 'a'
    }
}

# Esc on Welcome closes the wizard, and nothing is installed or registered.
function Test-EscOnWelcome {
    $process = Start-Interactive $script:ARTIFACT @() $false
    Assert $process 'the wizard exited at once'
    $window = Wait-MainWindow $process
    Wait-UiText $window 'Install Navo' 60
    Send-Key $window 0x1B
    Assert ($process.WaitForExit(15000)) 'Esc on Welcome did not close the wizard'
    Add-Fault ($process.ExitCode -eq 0) "Esc exit code was $($process.ExitCode)"
    Add-Fault (-not (Test-Path -LiteralPath (Get-DefaultRoot))) 'Esc on Welcome created the default root'
    Add-Fault (-not (Test-Path -LiteralPath $script:uninstallKey)) 'Esc on Welcome wrote the uninstall key'
    Write-Evidence "Esc on Welcome: exit=$($process.ExitCode)"
}

# Waits through the install after Alt+I, shooting the Installing page if it is seen; ends on the Finish page.
function Wait-Installed([IntPtr]$window, [string]$theme) {
    $shot = $false
    Wait-Until {
        $text = Get-UiText $window
        if ($text -match 'Navo is installed in') { return $true }
        if (-not $shot -and $text -notmatch 'Start menu') { $script:shot = $true; Save-UiDump $window $theme 'installing' }
        $false
    } 300 'the Finish page did not show'
    Write-Evidence "Installing page seen: $shot"
}

# Walks one wizard run by keys only: Enter, Alt+N, Alt+B, Tab, Space; dumps and shoots each page.
function Walk-Pages([string]$theme) {
    $process = Start-Interactive $script:ARTIFACT @() $false
    Assert $process 'the wizard exited at once'
    $window = Wait-MainWindow $process
    Wait-UiText $window 'Install Navo' 60
    Save-Page $window $theme 'welcome'
    Send-Key $window 0x0D
    Wait-UiText $window 'I accept the licence' 30
    Save-Page $window $theme 'licence'
    Accept-Licence $window $theme
    Save-Page $window $theme 'licence-accepted'
    Step-Page $window 'n' 'Uninstalling Navo deletes it|Install folder'
    Save-Page $window $theme 'folder'
    Step-Page $window 'n' 'Start menu'
    Save-Page $window $theme 'shortcuts'
    Step-Page $window 'b' 'Uninstalling Navo deletes it|Install folder'
    Step-Page $window 'n' 'Start menu'
    Send-Alt $window 'i'
    Wait-Installed $window $theme
    Save-Page $window $theme 'finish'
    Send-Alt $window 's'
    Start-Sleep -Milliseconds 300
    Send-Alt $window 'f'
    Add-Fault ($process.WaitForExit(30000)) "$theme`: Finish did not close the wizard"
}

# S12: both themes, pages walked with keys, UIA dump and picture per page; Esc on Welcome changes nothing.
function Test-S12 {
    Assert (Test-Path -LiteralPath $script:ARTIFACT) "no rehearsal wizard at $script:ARTIFACT"
    Backup-Registry $script:THEME_KEY
    Test-EscOnWelcome
    foreach ($theme in @(@('dark', 0), @('light', 1))) {
        Set-ItemProperty -LiteralPath "Registry::$script:THEME_KEY" -Name AppsUseLightTheme -Value $theme[1] -Type DWord
        Walk-Pages $theme[0]
        if ($theme[0] -eq 'dark') { Uninstall-Root (Get-DefaultRoot) }
    }
    Assert-Faults
}
