# Uninstall cases S4a, S4b and S4c and the planted-DLL case S13 of the installer's VM run (plan W15c).
# The service is installed by the admin a (UAC off for a); the uninstall runs as a (S4a) or as u (S4b, S4c).

$script:POLICY = 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'

# Runs as the SSH admin before a case: new password for a, UAC auto-elevates a, and u's request policy is set.
# $userConsent is ConsentPromptBehaviorUser (0 denies, 3 asks for credentials); -1 leaves it alone.
function Initialize-AdminAccount([string]$name, [int]$userConsent) {
    $folder = Join-Path $e2e "out\$name"
    $bytes = New-Object byte[] 18
    [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    $password = 'Aa1!' + [Convert]::ToBase64String($bytes)
    Set-LocalUser -Name a -Password (ConvertTo-SecureString $password -AsPlainText -Force)
    [IO.File]::WriteAllText((Join-Path $folder 'a.secret'), $password)
    Run-Checked reg.exe @('export', $script:POLICY, (Join-Path $folder 'uac.reg'), '/y') | Out-Null
    $key = "Registry::$($script:POLICY)"
    Set-ItemProperty -LiteralPath $key -Name ConsentPromptBehaviorAdmin -Value 0
    if ($userConsent -ge 0) { Set-ItemProperty -LiteralPath $key -Name ConsentPromptBehaviorUser -Value $userConsent }
}

# Runs as the SSH admin after the case is done: puts the UAC policy back and removes a's password file.
function Finalize-AdminAccount([string]$name) {
    $folder = Join-Path $e2e "out\$name"
    Remove-Item -LiteralPath (Join-Path $folder 'a.secret') -ErrorAction SilentlyContinue
    Run-Checked reg.exe @('import', (Join-Path $folder 'uac.reg')) | Out-Null
}

function Initialize-S4a { Initialize-AdminAccount 'S4a' -1 }
function Initialize-S4b { Initialize-AdminAccount 'S4b' 0 }
function Initialize-S4c { Initialize-AdminAccount 'S4c' 3 }
function Finalize-S4a { Finalize-AdminAccount 'S4a' }
function Finalize-S4b { Finalize-AdminAccount 'S4b' }
function Finalize-S4c { Finalize-AdminAccount 'S4c' }

# Runs the case's helper step as the unelevated admin a and waits for its result; throws unless it is OK.
function Invoke-AsA([string]$name) {
    $result = Join-Path $script:caseDir 'helper-result.txt'
    Remove-Item -LiteralPath $result -ErrorAction SilentlyContinue
    $password = ConvertTo-SecureString ([IO.File]::ReadAllText((Join-Path $script:caseDir 'a.secret'))) -AsPlainText -Force
    $credential = New-Object Management.Automation.PSCredential('a', $password)
    $arguments = Quote-Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $e2e 'scripts\setup-e2e.ps1'), '-Case', $name, '-Helper')
    Start-Process -FilePath $script:powershell -ArgumentList $arguments -Credential $credential -LoadUserProfile -WorkingDirectory $e2e
    Wait-Until { Test-Path -LiteralPath $result } 300 "the helper of $name as a wrote no result"
    $line = [IO.File]::ReadAllText($result).Trim()
    Assert ($line -eq 'OK') "helper as a: $line"
}

# Runs a helper step as a: writes helper-result.txt (OK or FAIL: reason) and a's wizard logs into the evidence.
function Invoke-HelperBody([string]$name, [scriptblock]$body) {
    $script:caseDir = Join-Path $e2e "out\$name"
    $script:WorkDir = $script:caseDir
    $line = 'OK'
    try { & $body }
    catch { $line = "FAIL: $($_.Exception.Message)"; Write-Evidence ($_ | Out-String) }
    $logs = Join-Path $env:TEMP 'NavoSetup-logs'
    if (Test-Path -LiteralPath $logs) { Copy-Item -LiteralPath $logs -Destination (Join-Path $script:caseDir 'wizard-logs-a') -Recurse -Force }
    [IO.File]::WriteAllText((Join-Path $script:caseDir 'helper-result.txt'), $line)
}

# Installs 0.0.2 silently into this user's default root, then the index service from it; returns the root.
function Install-RootWithService {
    $root = Get-DefaultRoot
    $result = Invoke-Silent '0.0.2'
    Assert ($result.Code -eq 0) "install exit $($result.Code): $($result.Error)"
    Assert-Installed $root '0.0.2'
    Install-ServiceFrom (Join-Path $root 'current\navo.exe')
    $root
}

# Returns the Navo log text of this user, from both Navo data folders, and copies the files into the evidence.
function Get-NavoLogText {
    $files = @(foreach ($base in @($env:LOCALAPPDATA, $env:APPDATA)) {
        $folder = Join-Path $base 'Navo'
        if (Test-Path -LiteralPath $folder) { Get-ChildItem -LiteralPath $folder -Recurse -Filter 'navo*.log' -File }
    })
    $copies = Join-Path $script:caseDir 'navo-logs'
    New-Item -ItemType Directory -Path $copies -Force | Out-Null
    $files | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $copies ([guid]::NewGuid().ToString('N') + '.log')) }
    (@($files | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n")
}

# Waits for the "Navo Uninstall" box that the interactive uninstall shows, and presses its OK.
function Close-UninstallBox([Diagnostics.Process]$process) {
    $desktop = [Windows.Automation.AutomationElement]::RootElement
    $title = New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty, 'Navo Uninstall')
    $ok = New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::NameProperty, 'OK')
    $state = @{ answered = $false }
    Wait-Until {
        $process.Refresh()
        $box = $desktop.FindFirst([Windows.Automation.TreeScope]::Children, $title)
        $button = if ($box) { $box.FindFirst([Windows.Automation.TreeScope]::Descendants, $ok) }
        if ($button) {
            Invoke-Element $button
            Start-Sleep -Seconds 2
            if ($desktop.FindFirst([Windows.Automation.TreeScope]::Children, $title)) { [NavoE2E.Native]::Focus([IntPtr]$box.Current.NativeWindowHandle) | Out-Null; [NavoE2E.Native]::Press($script:VK_RETURN) }
            $state.answered = $true
        }
        $state.answered -or $process.HasExited
    } 120 'the Uninstall Complete box did not appear'
    Write-Evidence "Uninstall Complete box answered: $($state.answered)"
}

# S4a as a: install, service, user files and a user-made link; run the UninstallString; check every outcome.
function Test-S4aAsA {
    $root = Install-RootWithService
    $links = Get-LinkPaths
    $mine = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Mine.lnk'
    $link = (New-Object -ComObject WScript.Shell).CreateShortcut($mine)
    $link.TargetPath = Join-Path $root 'current\navo.exe'
    $link.Save()
    Set-Content -LiteralPath (Join-Path $root 'inside.txt') -Value 'inside'
    $beside = Join-Path (Split-Path $root -Parent) 'beside.txt'
    Set-Content -LiteralPath $beside -Value 'beside'
    $data = @(foreach ($base in @($env:APPDATA, $env:LOCALAPPDATA)) { Join-Path $base 'Navo\keep.txt' })
    foreach ($file in $data) { New-Item -ItemType Directory -Path (Split-Path $file -Parent) -Force | Out-Null; Set-Content -LiteralPath $file -Value 'keep' }
    $command = (Get-ItemProperty -LiteralPath $script:uninstallKey).UninstallString
    $match = [regex]::Match($command, '^"([^"]+)"\s*(.*)$')
    Assert $match.Success "unexpected UninstallString: $command"
    Write-Evidence "UninstallString: $command"
    $process = Start-Process -FilePath $match.Groups[1].Value -ArgumentList $match.Groups[2].Value -PassThru
    Close-UninstallBox $process
    try { Wait-Until { -not (Test-Path -LiteralPath $root) } 90 'the root is still there' }
    catch { Write-Evidence "uninstall process ended=$($process.HasExited); processes: $((Get-InstallerProcesses) -join '; ')"; throw }
    Write-Evidence "root gone; uninstall process ended=$($process.HasExited)"
    Add-Fault (-not (Test-Path -LiteralPath (Join-Path $root 'inside.txt'))) 'inside.txt survived'
    Add-Fault (-not (Test-Path -LiteralPath $mine)) 'Mine.lnk survived (IW20)'
    Add-Fault (-not (Test-Path -LiteralPath $links.Desktop)) 'the Desktop Navo link survived'
    Add-Fault (-not (Test-Path -LiteralPath $links.Start)) 'the Start menu Navo link survived'
    Add-Fault (Test-Path -LiteralPath $beside) 'beside.txt was deleted'
    foreach ($file in $data) { Add-Fault (Test-Path -LiteralPath $file) "user data $file was deleted" }
    Add-Fault (-not (Test-Path -LiteralPath $script:uninstallKey)) 'the HKCU uninstall key survived'
    $query = Run-Process sc.exe @('query', 'Navo Index')
    Add-Fault ($query.Code -ne 0) "sc.exe query Navo Index exited 0: $($query.Text.Trim())"
    Write-Evidence "sc query exit=$($query.Code) log:`n$(Get-NavoLogText)"
    Assert-Faults
}

function Invoke-S4aHelper { Invoke-HelperBody 'S4a' { Test-S4aAsA } }
function Invoke-S4bHelper { Invoke-HelperBody 'S4b' { Install-RootWithService | Out-Null } }
function Invoke-S4cHelper { Invoke-HelperBody 'S4c' { Install-RootWithService | Out-Null } }

# S4a: as a with UAC off for a, the uninstall removes the root, the links and the service; user files stay.
function Test-S4a {
    try { Invoke-AsA 'S4a' }
    finally { Remove-Item -LiteralPath (Join-Path $script:caseDir 'a.secret') -ErrorAction SilentlyContinue }
}

# Installs 0.0.2 as u after a installed its own copy with the service; returns u's root.
function Install-WithServiceByA([string]$name) {
    $policy = Get-ItemProperty -LiteralPath "Registry::$($script:POLICY)"
    Write-Evidence "UAC policy: ConsentPromptBehaviorAdmin=$($policy.ConsentPromptBehaviorAdmin) ConsentPromptBehaviorUser=$($policy.ConsentPromptBehaviorUser) PromptOnSecureDesktop=$($policy.PromptOnSecureDesktop)"
    try { Invoke-AsA $name }
    finally { Remove-Item -LiteralPath (Join-Path $script:caseDir 'a.secret') -ErrorAction SilentlyContinue }
    $root = Get-DefaultRoot
    $result = Invoke-Silent '0.0.2'
    Assert ($result.Code -eq 0) "install as u exit $($result.Code): $($result.Error)"
    Assert-Installed $root '0.0.2'
    Assert ((Get-IndexService).State -eq 'Running') 'the service installed by a is not running'
    $root
}

# S4b: u's elevation request is denied; the uninstall removes the root, the service stays, the log names the skip.
function Test-S4b {
    $root = Install-WithServiceByA 'S4b'
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Uninstall-Root $root
    Write-Evidence "uninstall took $([math]::Round($clock.Elapsed.TotalSeconds, 1)) s; consent.exe prompts seen: $($script:consents.Count)"
    Assert ((Get-IndexService).State -eq 'Running') 'the service did not stay'
    $log = Get-NavoLogText
    Write-Evidence "Navo log:`n$log"
    # The log redacts the user name u everywhere, so "uninstall" is written "<user>ninstall".
    Assert ($log -match 'ninstall: index service left in place') 'the Navo log does not name the skipped removal'
}

# Asks the host to save a screen picture, which sees the secure desktop; waits for its answer, at most 120 s.
function Request-HostShot([string]$name) {
    Set-Content -LiteralPath (Join-Path $script:caseDir "shot-$name.req") -Value $name
    Wait-Until { Test-Path -LiteralPath (Join-Path $script:caseDir "shot-$name.ok") } 120 "the host saved no screen picture $name"
    Write-Evidence "screen picture $name.png saved by the host"
}

# S4c: the UAC prompt stays unanswered; the uninstall ends after about 25 s and the service stays.
function Test-S4c {
    $root = Install-WithServiceByA 'S4c'
    $updater = Join-Path $root 'Update.exe'
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $updater -ArgumentList 'uninstall --silent' -PassThru
    Wait-Until { @(Get-Process consent -ErrorAction SilentlyContinue).Count -gt 0 } 30 'no UAC prompt (consent.exe) appeared'
    Write-Evidence "consent.exe up after $([int]$clock.Elapsed.TotalSeconds) s"
    Request-HostShot 'uac-prompt'
    Assert (-not $process.HasExited) 'the uninstall ended before the prompt was shot'
    Wait-Until { $clock.Elapsed.TotalSeconds -ge 32 } 60 'the 32 s mark did not come'
    Write-Evidence "at $([int]$clock.Elapsed.TotalSeconds) s: consent.exe present=$(@(Get-Process consent -ErrorAction SilentlyContinue).Count -gt 0) uninstall ended=$($process.HasExited)"
    Assert ($process.WaitForExit(120000)) 'the uninstall did not end in 120 s'
    $length = ($process.ExitTime - $process.StartTime).TotalSeconds
    Write-Evidence "uninstall exit=$($process.ExitCode) after $([math]::Round($length, 1)) s"
    Assert ($process.ExitCode -eq 0) "Update.exe exited $($process.ExitCode)"
    Assert ($length -ge 20 -and $length -le 40) "the uninstall took $([math]::Round($length, 1)) s, not about 25 s"
    Wait-Until { -not (Test-Path -LiteralPath $root) } 60 'the root is still there'
    Assert ((Get-IndexService).State -eq 'Running') 'the service did not stay'
    $log = Get-NavoLogText
    Write-Evidence "Navo log:`n$log"
    Assert ($log -match 'no answer within 25 s') 'the Navo log does not name the 25 s timeout'
}

# DLL names S13 plants beside the wizard: four always, then the delay-load list of crates/navo-setup/build.rs.
$script:PLANT_NAMES = @('version.dll', 'dwmapi.dll', 'uxtheme.dll', 'cryptbase.dll', 'bcrypt.dll', 'bcryptprimitives.dll',
    'comctl32.dll', 'd3d11.dll', 'dcomp.dll', 'dwrite.dll', 'dxgi.dll', 'icuuc.dll', 'uiautomationcore.dll', 'userenv.dll', 'winmm.dll')

# Rust source of plant.dll: DllMain writes plant-out\<own file name>.txt; Ping gives rundll32 an export.
$script:PLANT_SOURCE = @'
use std::ffi::c_void;
extern "system" {
    fn GetModuleHandleExW(flags: u32, address: *const c_void, module: *mut isize) -> i32;
    fn GetModuleFileNameW(module: isize, buffer: *mut u16, size: u32) -> u32;
}
fn note() {
    let mut module = 0isize;
    let mut buffer = [0u16; 520];
    unsafe {
        if GetModuleHandleExW(6, note as *const c_void, &mut module) == 0 { return; }
        let length = GetModuleFileNameW(module, buffer.as_mut_ptr(), 520) as usize;
        let path = String::from_utf16_lossy(&buffer[..length]);
        let name = path.rsplit('\\').next().unwrap_or("unknown");
        let _ = std::fs::write(format!("C:\\Users\\u\\plant-out\\{name}.txt"), path.as_bytes());
    }
}
#[no_mangle]
pub extern "system" fn DllMain(_module: isize, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == 1 { note(); }
    1
}
#[no_mangle]
pub extern "system" fn Ping() {}
'@

# Runs as the SSH admin before S13: builds plant.dll and creates plant-out, owned by u.
function Initialize-S13 {
    $folder = Join-Path $e2e 'out\S13'
    $source = Join-Path $folder 'plant.rs'
    [IO.File]::WriteAllText($source, $script:PLANT_SOURCE, (New-Object Text.UTF8Encoding($false)))
    Run-Checked rustc.exe @('--edition', '2021', '--crate-type', 'cdylib', '-C', 'opt-level=1', '-C', 'target-feature=+crt-static', '-o', (Join-Path $folder 'plant.dll'), $source) 300 | Out-Null
    $out = 'C:\Users\u\plant-out'
    New-Item -ItemType Directory -Path $out -Force | Out-Null
    Run-Checked icacls.exe @($out, '/setowner', 'u') | Out-Null
    Run-Checked icacls.exe @($out, '/grant', 'u:(OI)(CI)F') | Out-Null
}

# Returns the file names in plant-out.
function Get-PlantOut { @(Get-ChildItem -LiteralPath 'C:\Users\u\plant-out' -Force | ForEach-Object Name) }

# S13: planted DLLs beside the wizard stay unloaded; a positive control proves that the plant would load.
function Test-S13 {
    $downloads = Join-Path $env:USERPROFILE 'Downloads'
    New-Item -ItemType Directory -Path $downloads -Force | Out-Null
    $exe = Join-Path $downloads 'Navo-Setup.exe'
    Copy-Item -LiteralPath (Join-Path $wizards '0.0.2\Navo-Setup.exe') -Destination $exe
    foreach ($name in $script:PLANT_NAMES) { Copy-Item -LiteralPath (Join-Path $script:caseDir 'plant.dll') -Destination (Join-Path $downloads $name) }
    Assert (@(Get-PlantOut).Count -eq 0) "plant-out is not empty: $((Get-PlantOut) -join ', ')"
    Start-Process -FilePath rundll32.exe -ArgumentList "`"$(Join-Path $downloads 'version.dll')`",Ping" -Wait -WindowStyle Hidden
    Wait-Until { @(Get-PlantOut) -contains 'version.dll.txt' } 10 'the positive control wrote no version.dll.txt'
    Write-Evidence "positive control wrote: $((Get-PlantOut) -join ', ')"
    Get-ChildItem -LiteralPath 'C:\Users\u\plant-out' -Force | Remove-Item -Force
    $process = Start-Interactive $exe @() $false
    Assert $process 'the wizard exited at once'
    $wizard = [pscustomobject]@{ Process = $process; Window = (Wait-MainWindow $process) }
    Step-ToFolder $wizard 'Install Navo 0\.0\.2'
    Step-FreshInstall $wizard
    Finish-Wizard $wizard $false
    $loaded = @(Get-PlantOut)
    Write-Evidence "plant-out after the wizard: [$($loaded -join ', ')]; planted: $($script:PLANT_NAMES -join ', ')"
    Assert ($loaded.Count -eq 0) "a planted DLL ran: $($loaded -join ', ')"
}
