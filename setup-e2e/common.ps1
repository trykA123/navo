# Shared helpers for the installer's VM cases; dot-sourced by scripts/setup-e2e.ps1 (plan W15a).
# update-e2e.ps1 runs a harness at its top level, so its helpers are adapted copies, each naming its source.

$script:e2e = 'C:\e2e'
$script:repo = $script:e2e
$script:WorkDir = Join-Path $script:e2e 'work'
$script:packages = Join-Path $script:e2e 'packages'
$script:wizards = Join-Path $script:e2e 'wizard'
$script:caseStart = Get-Date
$script:consents = New-Object 'System.Collections.Generic.HashSet[int]'
$script:registryBackups = @()
$script:powershell = Join-Path $PSHOME 'powershell.exe'

# Source: scripts/update-e2e.ps1:31-33, unchanged.
function Assert($condition, [string]$reason) {
    if (-not $condition) { throw $reason }
}

# Source: scripts/update-e2e.ps1:35-39, unchanged.
function Quote-Arguments([string[]]$values) {
    ($values | ForEach-Object {
        '"' + [regex]::Replace([regex]::Replace($_, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
    }) -join ' '
}

# Source: scripts/update-e2e.ps1:41-59, unchanged. GUI executables need redirected streams through a Process object.
function Run-Process([string]$exe, [string[]]$arguments, [int]$seconds = 120, [hashtable]$environment = @{}) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo($exe, (Quote-Arguments $arguments))
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $repo
    foreach ($key in $environment.Keys) { $psi.EnvironmentVariables[$key] = $environment[$key] }
    $process = [Diagnostics.Process]::Start($psi)
    try {
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($seconds * 1000)) {
            $process.Kill()
            throw "$exe timed out after $seconds seconds"
        }
        [pscustomobject]@{ Code = $process.ExitCode; Text = $stdout.Result; Error = $stderr.Result }
    } finally { $process.Dispose() }
}

# Source: scripts/update-e2e.ps1:61-66, unchanged. Runs a command and throws on a nonzero exit.
function Run-Checked([string]$exe, [string[]]$arguments, [int]$seconds = 120, [hashtable]$environment = @{}) {
    $result = Run-Process $exe $arguments $seconds $environment
    $result.Text + $result.Error | Add-Content -LiteralPath (Join-Path $WorkDir 'commands.log')
    Assert ($result.Code -eq 0) "$exe exited $($result.Code): $($result.Error)"
    $result.Text.Trim()
}

# Source: scripts/update-e2e.ps1:68-70, unchanged.
function Get-IndexService {
    Get-CimInstance Win32_Service -Filter "Name='Navo Index'"
}

# Source: scripts/update-e2e.ps1:81-85, unchanged. Records consent.exe prompts started in this case.
function Observe-Consent {
    foreach ($process in @(Get-Process consent -ErrorAction SilentlyContinue)) {
        if ($process.StartTime -ge $script:caseStart) { [void]$script:consents.Add($process.Id) }
    }
}

# Source: scripts/update-e2e.ps1:87-95, unchanged. Polls a condition; throws the reason at the deadline (seconds).
function Wait-Until([scriptblock]$condition, [int]$seconds, [string]$reason) {
    $deadline = [DateTime]::UtcNow.AddSeconds($seconds)
    do {
        Observe-Consent
        if (& $condition) { return }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $reason
}

# Source: scripts/update-e2e.ps1:577-601, unchanged. Builds and packs one tester version; used only for the base.
function Build-Pack([string]$name, [string]$version, [string]$source) {
    $folder = Join-Path $packages $name
    $dist = Join-Path $folder 'dist'
    New-Item -ItemType Directory -Path $dist -Force | Out-Null
    $environment = @{ NAVO_BUILD = "v$version"; CARGO_TARGET_DIR = (Join-Path $WorkDir 'target'); CARGO_PROFILE_RELEASE_LTO = 'thin' }
    Write-Host "Building $name ($version) from $source"
    Push-Location $source
    try {
        $savedRepo = $script:repo
        $script:repo = $source
        Run-Checked cargo @('build', '--locked', '--release', '-p', 'navo-app', '--features', 'tester') 3600 $environment | Out-Null
        Run-Checked cargo @('build', '--locked', '--release', '-p', 'navo-index-service') 1800 $environment | Out-Null
        foreach ($file in @('navo.exe', 'navo-index.exe', 'navo.pdb')) {
            Copy-Item -LiteralPath (Join-Path $environment.CARGO_TARGET_DIR "release\$file") -Destination $dist
        }
        Copy-Item -Path (Join-Path $WorkDir 'runtime\*') -Destination $dist -Recurse
        $arguments = @('pack', '--packId', 'NavoApp', '--packVersion', $version, '--packTitle', 'Navo',
            '--packDir', $dist, '--mainExe', 'navo.exe', '--icon', (Join-Path $source 'assets\brand\navo.ico'),
            '--channel', 'win', '--runtime', 'win-x64', '--outputDir', (Join-Path $folder 'out'))
        Run-Checked $script:vpk $arguments 600 | Out-Null
    } finally { $script:repo = $savedRepo; Pop-Location }
    $feed = Get-Content -LiteralPath (Join-Path $folder 'out\releases.win.json') -Raw | ConvertFrom-Json
    Assert (@($feed.Assets | Where-Object { $_.Version -eq $version -and $_.Type -eq 'Full' }).Count -eq 1) "invalid $name feed"
}

# Source: scripts/update-e2e.ps1:220-255 (Start-App), without --profile, --left and the feed argument.
# Starts an exe in this user's interactive session through a task; returns its Process. $environment is added here.
function Start-Interactive([string]$exe, [string[]]$arguments, [bool]$elevated, [hashtable]$environment = @{}) {
    $launch = Join-Path $script:caseDir ([guid]::NewGuid().ToString('N'))
    $config = @{ exe = $exe; arguments = (Quote-Arguments $arguments); elevated = $elevated; environment = $environment }
    $json = $config | ConvertTo-Json
    [IO.File]::WriteAllText("$launch.json", $json, (New-Object Text.UTF8Encoding($false)))
    @'
param($Config)
$ErrorActionPreference = 'Stop'
try {
    $text = [IO.File]::ReadAllText($Config, [Text.Encoding]::UTF8)
    $config = $text | ConvertFrom-Json
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    $elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if ($elevated -ne $config.elevated) { throw 'unexpected task elevation' }
    $psi = New-Object Diagnostics.ProcessStartInfo($config.exe, $config.arguments)
    $psi.UseShellExecute = $false
    foreach ($item in $config.environment.PSObject.Properties) { $psi.EnvironmentVariables[$item.Name] = [string]$item.Value }
    $process = [Diagnostics.Process]::Start($psi)
    $process.Id | Set-Content -LiteralPath ($PSCommandPath + '.pid')
} catch { $_.ToString() | Set-Content -LiteralPath ($PSCommandPath + '.error') }
'@ | Set-Content -LiteralPath "$launch.ps1" -Encoding UTF8
    $task = 'NavoSetupE2E-' + [guid]::NewGuid().ToString('N')
    $taskArguments = Quote-Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$launch.ps1", '-Config', "$launch.json")
    $action = New-ScheduledTaskAction -Execute $script:powershell -Argument $taskArguments
    $level = if ($elevated) { 'Highest' } else { 'Limited' }
    $userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $principal = New-ScheduledTaskPrincipal -UserId $userSid -LogonType Interactive -RunLevel $level
    Register-ScheduledTask -TaskName $task -Action $action -Principal $principal | Out-Null
    try {
        Start-ScheduledTask -TaskName $task
        Wait-Until { (Test-Path -LiteralPath "$launch.ps1.pid") -or (Test-Path -LiteralPath "$launch.ps1.error") } 30 'interactive launch timed out'
        if (Test-Path -LiteralPath "$launch.ps1.error") { throw (Get-Content -LiteralPath "$launch.ps1.error" -Raw) }
        Get-Process -Id ([int](Get-Content -LiteralPath "$launch.ps1.pid")) -ErrorAction SilentlyContinue
    } finally { Unregister-ScheduledTask -TaskName $task -Confirm:$false }
}

# Source: scripts/update-e2e.ps1:281-291 (Install-Base). Runs a raw Setup.exe --silent; asserts the legacy install's version.
function Install-Raw([string]$setup, [string]$version) {
    $root = Join-Path $env:LOCALAPPDATA 'NavoApp'
    Assert (-not (Test-Path -LiteralPath $root)) "existing install at $root; use a clean base"
    Run-Checked $setup @('--silent') | Out-Null
    Wait-Until { Test-Path -LiteralPath (Join-Path $root 'current\navo.exe') } 60 'Setup did not install current\navo.exe'
    Stop-Root $root
    Assert ((Read-SqVersion $root) -eq $version) "expected raw install $version"
}

# Source: scripts/update-e2e.ps1:317-326 (Install-Service). Installs the service elevated; checks its image is under $navo's root.
function Install-ServiceFrom([string]$navo) {
    Assert (-not (Get-IndexService)) 'refusing to replace an existing service'
    Run-Checked $navo @('--index-service', 'install') 90 | Out-Null
    Wait-Until { Observe-Consent; $service = Get-IndexService; $service -and $service.State -eq 'Running' } 30 'service did not start'
    $root = Split-Path (Split-Path $navo -Parent) -Parent
    Assert ((Get-IndexService).PathName.Trim('"').StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) 'service image is outside the root'
}

# Source: scripts/update-e2e.ps1:419-429 (Uninstall-Copy). Runs <root>\Update.exe uninstall --silent and waits for the root to go.
function Uninstall-Root([string]$root) {
    $updater = Join-Path $root 'Update.exe'
    Assert (Test-Path -LiteralPath $updater) 'the root has no Update.exe for uninstall'
    Run-Checked $updater @('uninstall', '--silent') 120 | Out-Null
    Wait-Until { -not (Test-Path -LiteralPath $root) } 30 'uninstall left its install folder'
}

# Appends one checked observation to the case's evidence file.
function Write-Evidence([string]$message) {
    Add-Content -LiteralPath (Join-Path $script:caseDir 'evidence.txt') -Value $message -Encoding UTF8
}

# Exports a registry key to the case folder, so Restore-Registry can put it back.
function Backup-Registry([string]$key) {
    $file = Join-Path $script:caseDir ('reg-' + [guid]::NewGuid().ToString('N') + '.reg')
    Run-Checked reg.exe @('export', $key, $file, '/y') | Out-Null
    $script:registryBackups += $file
}

# Imports every exported key; run from a case's finally block. Throws when an import fails.
function Restore-Registry {
    foreach ($file in $script:registryBackups) { Run-Checked reg.exe @('import', $file) | Out-Null }
    $script:registryBackups = @()
}

# Returns the folder of one pack by its version (0.0.1 to 0.0.3); throws on any other.
function Get-PackDir([string]$version) {
    $name = @{ '0.0.1' = 'previous'; '0.0.2' = 'base'; '0.0.3' = 'next' }[$version]
    Assert $name "no pack for version $version"
    Join-Path $packages "$name\out"
}

# Returns the path of the pack's single file that matches a filter; throws unless exactly one does.
function Get-PackFile([string]$version, [string]$filter) {
    $found = @(Get-ChildItem -LiteralPath (Get-PackDir $version) -Filter $filter)
    Assert ($found.Count -eq 1) "expected one $filter in pack $version, found $($found.Count)"
    $found[0].FullName
}

# Returns the expected default root, %LocalAppData%\Programs\Navo.
function Get-DefaultRoot { Join-Path $env:LOCALAPPDATA 'Programs\Navo' }

# Returns the version in <root>\current\sq.version; throws when it is missing.
function Read-SqVersion([string]$root) {
    $file = Join-Path $root 'current\sq.version'
    ([xml][IO.File]::ReadAllText($file)).package.metadata.version
}

# Returns the marker JSON of a root as an object; throws when it is missing or malformed.
function Read-Marker([string]$root) {
    [IO.File]::ReadAllText((Join-Path $root 'navo-install.json')) | ConvertFrom-Json
}

# Returns the shell link paths: Desktop and Start menu.
function Get-LinkPaths {
    [pscustomobject]@{
        Desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Navo.lnk'
        Start = Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs\Navo.lnk'
    }
}

# Asserts that a link exists and that its target is inside the root.
function Assert-LinkToRoot([string]$link, [string]$root) {
    Assert (Test-Path -LiteralPath $link) "missing link $link"
    $target = (New-Object -ComObject WScript.Shell).CreateShortcut($link).TargetPath
    Assert ($target.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) "$link targets $target, outside $root"
    Write-Evidence "link $link -> $target"
}

# Returns navo.exe process records whose image sits under the root.
function Get-RootApps([string]$root) {
    @(Get-CimInstance Win32_Process -Filter "Name='navo.exe'" | Where-Object {
        $_.ExecutablePath -and $_.ExecutablePath.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)
    })
}

# Force-stops every navo.exe under the root; throws when one survives 10 s.
function Stop-Root([string]$root) {
    foreach ($record in @(Get-RootApps $root)) {
        $process = Get-Process -Id $record.ProcessId -ErrorAction SilentlyContinue
        if (-not $process) { continue }
        $process.Kill()
        Assert ($process.WaitForExit(10000)) 'Navo did not stop'
    }
}

# Returns a sorted "relative path, SHA-256" listing of every file under a root.
function Get-HashListing([string]$root) {
    @(Get-ChildItem -LiteralPath $root -Recurse -Force -File | Sort-Object FullName | ForEach-Object {
        '{0} {1}' -f $_.FullName.Substring($root.Length), (Get-FileHash -LiteralPath $_.FullName).Hash
    })
}

# Returns installer processes other than the wizard: Setup.exe, Update.exe and Squirrel.exe, as "name pid command".
function Get-InstallerProcesses {
    @(Get-CimInstance Win32_Process -Filter "Name='Setup.exe' or Name='Update.exe' or Name='Squirrel.exe' or Name='NavoApp-win-Setup.exe'" |
        ForEach-Object { '{0} {1} {2}' -f $_.Name, $_.ProcessId, $_.CommandLine })
}

# Runs a silent tester wizard and logs installer processes. Result: Code, Text, Error, Seen. $onPoll runs at each poll.
# $setup and $package replace the pack's own payload files (S10's fakes).
function Invoke-Silent([string]$version, [string[]]$arguments = @(), [scriptblock]$onPoll = $null, [int]$seconds = 600, [string]$setup = '', [string]$package = '') {
    $exe = Join-Path $wizards 'tester\Navo-Setup.exe'
    if (-not $setup) { $setup = Get-PackFile $version '*Setup.exe' }
    if (-not $package) { $package = Get-PackFile $version '*-full.nupkg' }
    $all = @('--silent') + $arguments + @('--payload', $setup, '--payload-package', $package, '--payload-version', $version)
    $psi = New-Object System.Diagnostics.ProcessStartInfo($exe, (Quote-Arguments $all))
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $script:e2e
    $process = [Diagnostics.Process]::Start($psi)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $seen = New-Object 'System.Collections.Generic.HashSet[string]'
    $deadline = [DateTime]::UtcNow.AddSeconds($seconds)
    while (-not $process.HasExited) {
        foreach ($line in @(Get-InstallerProcesses)) { [void]$seen.Add($line) }
        if ($onPoll) { & $onPoll }
        if ([DateTime]::UtcNow -gt $deadline) { $process.Kill(); throw "wizard timed out after $seconds seconds" }
        Start-Sleep -Milliseconds 20
    }
    $process.WaitForExit()
    $result = [pscustomobject]@{ Code = $process.ExitCode; Text = $stdout.Result; Error = $stderr.Result; Seen = @($seen) }
    Write-Evidence "wizard $version $($arguments -join ' ') exit=$($result.Code) stderr=$($result.Error.Trim())"
    foreach ($line in $result.Seen) { Write-Evidence "process: $line" }
    $result
}

# Copies the wizard logs of this user into the evidence folder; a missing folder is not an error.
function Save-WizardLogs {
    $logs = Join-Path $env:TEMP 'NavoSetup-logs'
    if (Test-Path -LiteralPath $logs) { Copy-Item -LiteralPath $logs -Destination (Join-Path $script:caseDir 'wizard-logs') -Recurse -Force }
}

# Asserts the installed state shared by S1a and S1b, for the root and version.
function Assert-Installed([string]$root, [string]$version) {
    Assert (Test-Path -LiteralPath (Join-Path $root 'Update.exe')) "no Update.exe in $root"
    Assert ((Read-SqVersion $root) -eq $version) "sq.version is not $version"
    Assert ((Read-Marker $root).packId -eq 'NavoApp') 'the marker packId is not NavoApp'
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NavoApp'
    $location = (Get-ItemProperty -LiteralPath $key).InstallLocation
    Assert ($location.TrimEnd('\') -eq $root) "InstallLocation is $location"
    Assert (-not (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'NavoApp'))) '%LocalAppData%\NavoApp exists'
    Write-Evidence "installed root=$root version=$version InstallLocation=$location"
}

# Asserts that a -velopack.log and the wizard's own .log share one run stem in the log folder.
function Assert-RunLogs {
    $logs = Join-Path $env:TEMP 'NavoSetup-logs'
    $velopack = @(Get-ChildItem -LiteralPath $logs -Filter '*-velopack.log')
    Assert ($velopack.Count -ge 1) "no -velopack.log in $logs"
    $beside = @($velopack | Where-Object { Test-Path -LiteralPath ($_.FullName -replace '-velopack\.log$', '.log') })
    Assert ($beside.Count -ge 1) "no -velopack.log beside a wizard log in $logs"
    Write-Evidence "logs: $($beside[0].FullName)"
}

# Source: scripts/update-e2e.ps1:210-218 (New-Profile). Writes a Navo profile with update checks on.
function New-Profile {
    $profile = Join-Path $script:caseDir ('profile-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path (Join-Path $profile 'config') | Out-Null
    "[updates]`ncheck = true`n[tray]`nkeep-running = false" | Set-Content -LiteralPath (Join-Path $profile 'config\navo.toml') -Encoding Ascii
    $profile
}

# Source: scripts/update-e2e.ps1:123-127 (Get-Status), with the exe required. Returns --update-status as an object.
function Get-Status([string]$exe) {
    $environment = @{ LOCALAPPDATA = $script:caseDir; APPDATA = $script:caseDir }
    Run-Checked $exe @('--update-status') 20 $environment | ConvertFrom-Json
}

# Starts Navo on the 0.0.3 feed and waits for the staged package; returns the app's process.
function Start-Staged([string]$root, [string]$profile) {
    $navo = Join-Path $root 'current\navo.exe'
    $environment = @{ NAVO_UPDATE_FEED_DIR = (Get-PackDir '0.0.3'); NAVO_LOG = 'info,navo::updater=debug' }
    $process = Start-Interactive $navo @('--profile', $profile, '--left', $script:caseDir) $false $environment
    Assert $process 'Navo exited at once'
    Wait-Until { (Get-Status $navo).staged_version -eq '0.0.3' } 180 '0.0.3 was not staged'
    $process
}

# Waits until the root's sq.version shows the version, tolerating the swap of current\ while it runs.
function Wait-Version([string]$root, [string]$version) {
    Wait-Until {
        try { (Read-SqVersion $root) -eq $version } catch { $false }
    } 180 "the root did not reach $version"
}

# Activates a UIA element: Invoke when it has the pattern, else a mouse click on its centre.
function Invoke-Element($element) {
    $pattern = $null
    if ($element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) { $pattern.Invoke(); return }
    $box = $element.Current.BoundingRectangle
    Assert (-not $box.IsEmpty) "element '$($element.Current.Name)' has no pattern and no bounds"
    [NavoE2E.Native]::Click([int]($box.X + $box.Width / 2), [int]($box.Y + $box.Height / 2))
}

# Opens Settings with Ctrl+comma and presses the Restart button of its update row through UI Automation.
function Invoke-NavoRestart([IntPtr]$window) {
    Wait-Until { [NavoE2E.Native]::Focus($window) } 10 'Navo did not take focus'
    [NavoE2E.Native]::Press([uint16]0xBC, $script:VK_CONTROL)
    $root = [Windows.Automation.AutomationElement]::FromHandle($window)
    $name = [Windows.Automation.AutomationElement]::NameProperty
    $find = { param($text) $root.FindFirst([Windows.Automation.TreeScope]::Descendants, (New-Object Windows.Automation.PropertyCondition($name, $text))) }
    Wait-Until { & $find 'Help' } 30 'Settings has no Help page'
    Invoke-Element (& $find 'Help')
    try { Wait-Until { & $find 'Restart' } 30 'Settings offers no Restart' }
    catch { Write-Evidence "Navo text at timeout:`n$(Get-UiText $window)"; throw }
    Invoke-Element (& $find 'Restart')
}
