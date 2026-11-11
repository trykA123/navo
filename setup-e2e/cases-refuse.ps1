# Refusal cases S3b, S5, S6 and S15 of the installer's VM run (plan W15b); S7, S10 and S14 are in cases-refuse-more.ps1.
# Also holds the soft checks (faults) and the small UI and folder helpers both refusal files share.

Add-Type @'
// Reads a known folder the way the wizard does (KF_FLAG_DEFAULT), so S6 can prove its set-up took effect.
using System;
using System.Runtime.InteropServices;
namespace NavoE2E {
    public static class Known {
        [DllImport("shell32")] static extern int SHGetKnownFolderPath(ref Guid id, uint flags, IntPtr token, out IntPtr path);
        // Returns the folder's path, or null when Windows reports an error for it.
        public static string Path(string id) {
            var guid = new Guid(id); IntPtr path;
            if (SHGetKnownFolderPath(ref guid, 0, IntPtr.Zero, out path) != 0) return null;
            try { return Marshal.PtrToStringUni(path); } finally { Marshal.FreeCoTaskMem(path); }
        }
    }
}
'@

$script:faults = New-Object 'System.Collections.Generic.List[string]'
$script:uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NavoApp'
$script:shellKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'
$script:shellUndo = @()
$script:FOLDER_ONEDRIVE = '{A52BBA46-E9E1-435F-B3D9-28DAA648C0F6}'

# Records a failed check without stopping the case, so one run shows every broken condition.
function Add-Fault($condition, [string]$reason) {
    if (-not $condition) { $script:faults.Add($reason); Write-Evidence "FAULT: $reason" }
}

# Throws one line naming every recorded fault; the last step of a case that uses Add-Fault.
function Assert-Faults {
    Assert ($script:faults.Count -eq 0) ($script:faults -join '; ')
}

# Returns the UIA elements under a window whose name matches a regex.
function Get-UiElements([IntPtr]$window, [string]$pattern) {
    $all = [Windows.Automation.AutomationElement]::FromHandle($window).FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    @($all | Where-Object { $_.Current.Name -match $pattern })
}

# Returns the full paths of a folder's direct entries; an empty list when the folder is missing.
function Get-Entries([string]$folder) {
    if (-not (Test-Path -LiteralPath $folder)) { return @() }
    @([IO.Directory]::GetFileSystemEntries($folder))
}

# Installs 0.0.2 silently into the default root and returns the root; throws on a nonzero exit.
function Install-Default {
    $result = Invoke-Silent '0.0.2'
    Assert ($result.Code -eq 0) "install exit $($result.Code): $($result.Error)"
    Get-DefaultRoot
}

# Runs wizard 0.0.3 with --installto a second folder; asserts exit 3 and that nothing changed.
function Assert-SecondRefused([string]$root) {
    $other = Join-Path $env:USERPROFILE 'Other'
    New-Item -ItemType Directory -Path $other | Out-Null
    $location = (Get-ItemProperty -LiteralPath $script:uninstallKey).InstallLocation
    $before = Get-HashListing $root
    $result = Invoke-Silent '0.0.3' @('--installto', (Join-Path $other 'Navo'))
    Assert ($result.Code -eq 3) "second folder exit $($result.Code), not 3: $($result.Error)"
    Assert ($result.Seen.Count -eq 0) "an installer process started: $($result.Seen -join '; ')"
    Assert (@(Get-Entries $other).Count -eq 0) "$other is not empty"
    Assert ((Get-ItemProperty -LiteralPath $script:uninstallKey).InstallLocation -eq $location) 'InstallLocation changed'
    Assert ((Compare-Object $before (Get-HashListing $root)) -eq $null) 'the existing root changed'
    Write-Evidence "refused a second folder; InstallLocation stays $location"
}

# S3b part 1: the wizard's own install in the default root refuses a second folder.
function Test-S3b1 {
    Assert-SecondRefused (Install-Default)
}

# S3b part 2: a raw Setup.exe install in the legacy root refuses a second folder.
function Test-S3b2 {
    Install-Raw (Get-PackFile '0.0.1' '*Setup.exe') '0.0.1'
    Assert-SecondRefused (Join-Path $env:LOCALAPPDATA 'NavoApp')
}

# Starts Navo from the root with update checks off, so it writes nothing into the root; returns its process.
function Start-Idle([string]$root) {
    $profile = Join-Path $script:caseDir ('profile-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path (Join-Path $profile 'config') | Out-Null
    "[updates]`ncheck = false`n[tray]`nkeep-running = false" | Set-Content -LiteralPath (Join-Path $profile 'config\navo.toml') -Encoding Ascii
    $process = Start-Interactive (Join-Path $root 'current\navo.exe') @('--profile', $profile) $false
    Assert $process 'Navo exited at once'
    Wait-MainWindow $process | Out-Null
    $process
}

# Returns every process whose name starts with navo, as "name pid session path", one per line.
function Get-NavoProcesses {
    @(Get-CimInstance Win32_Process -Filter "Name LIKE 'navo%'" | ForEach-Object {
        '{0} {1} {2} {3}' -f $_.Name, $_.ProcessId, $_.SessionId, $_.ExecutablePath
    }) -join "`n"
}

# S5: a running Navo blocks the upgrade, silent (exit 4) and in the window, until it closes.
function Test-S5 {
    $root = Install-Default
    $app = Start-Idle $root
    $before = Get-HashListing $root
    $result = Invoke-Silent '0.0.3'
    Assert ($result.Code -eq 4) "silent exit $($result.Code), not 4: $($result.Error)"
    Assert ($result.Seen.Count -eq 0) "an installer process started: $($result.Seen -join '; ')"
    $wizard = Start-Wizard '0.0.3'
    Step-ToFolder $wizard 'Install Navo 0\.0\.3'
    Step-Page $wizard.Window 'i' 'Close Navo to continue' 120
    $app.Refresh()
    Assert (-not $app.HasExited) 'Navo stopped while Setup was blocked'
    Assert ((Compare-Object $before (Get-HashListing $root)) -eq $null) 'the root changed while Navo ran'
    Write-Evidence "blocked twice; root unchanged across $($before.Count) files"
    Stop-Root $root
    Write-Evidence "navo processes after the stop:`n$(Get-NavoProcesses)"
    try { Step-Page $wizard.Window 't' 'Navo is installed in' 120 }
    catch {
        # Evidence only: a silent run now tells a window bug (exit 0) from a process still running (exit 4).
        Write-Evidence "navo processes after Try again:`n$(Get-NavoProcesses)"
        # A UIA press rules out lost keys: if the button works, the page moves.
        foreach ($button in @(Get-UiElements $wizard.Window '^Try again')) { Invoke-Element $button }
        try { Wait-UiText $wizard.Window 'Navo is installed in' 30 } catch { Write-Evidence 'a UIA press of Try again did not move the page either' }
        Send-Alt $wizard.Window 'c'
        $wizard.Process.WaitForExit(30000) | Out-Null
        Invoke-Silent '0.0.3' | Out-Null
        throw
    }
    Finish-Wizard $wizard $false
    Assert ((Read-SqVersion $root) -eq '0.0.3') 'Try again did not upgrade to 0.0.3'
}

# Runs as the SSH admin before S6: attaches a small VHD as the fixed drive D:, writable by u.
function Initialize-S6 {
    Assert (-not (Test-Path -LiteralPath 'D:\')) 'D: is already in use'
    $vhd = Join-Path $e2e 'd-drive.vhdx'
    $commands = Join-Path $e2e 'out\S6\diskpart.txt'
    @("create vdisk file=`"$vhd`" maximum=256 type=expandable", "select vdisk file=`"$vhd`"", 'attach vdisk', 'convert mbr',
        'create partition primary', 'format fs=ntfs quick label=E2E', 'assign letter=D') | Set-Content -LiteralPath $commands -Encoding Ascii
    $process = Start-Process -FilePath diskpart.exe -ArgumentList "/s `"$commands`"" -Wait -PassThru -WindowStyle Hidden
    Assert ($process.ExitCode -eq 0) "diskpart exited $($process.ExitCode)"
    Wait-Until { Test-Path -LiteralPath 'D:\' } 60 'D: did not appear'
    Run-Checked icacls.exe @('D:\', '/grant', 'u:(OI)(CI)M') | Out-Null
}

# Sets one User Shell Folders value and remembers the old one for Restore-ShellFolders.
function Set-ShellFolder([string]$name, [string]$value) {
    $item = Get-Item -LiteralPath $script:shellKey
    $old = $item.GetValue($name, $null, 'DoNotExpandEnvironmentNames')
    $kind = if ($null -ne $old) { $item.GetValueKind($name) } else { $null }
    $script:shellUndo += , @($name, $old, $kind)
    Set-ItemProperty -LiteralPath $script:shellKey -Name $name -Value $value -Type ExpandString
}

# Puts back every User Shell Folders value Set-ShellFolder changed, newest first; removes added ones.
function Restore-ShellFolders {
    for ($i = $script:shellUndo.Count - 1; $i -ge 0; $i--) {
        $name, $old, $kind = $script:shellUndo[$i]
        if ($null -eq $old) { Remove-ItemProperty -LiteralPath $script:shellKey -Name $name }
        else { Set-ItemProperty -LiteralPath $script:shellKey -Name $name -Value $old -Type $kind }
    }
    $script:shellUndo = @()
}

# Makes S6's set-up: junction C:\J, subst X: and Y:, Documents on D:\Docs and a OneDrive folder; each is checked.
function New-S6Fixtures {
    foreach ($drive in @('X:\', 'Y:\')) { Assert (-not (Test-Path -LiteralPath $drive)) "$drive is in use" }
    Assert (Test-Path -LiteralPath 'D:\') 'D: was not prepared'
    # KF_FLAG_DEFAULT verifies that the folder exists, so a redirected Documents must exist to resolve.
    New-Item -ItemType Directory -Path 'D:\Docs' | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $env:USERPROFILE 'Prog'), (Join-Path $env:USERPROFILE 'Empty') | Out-Null
    New-Item -ItemType Junction -Path 'C:\J' -Target 'C:\Windows' | Out-Null
    Invoke-Subst 'X: C:\Windows' $true
    Invoke-Subst "Y: `"$(Join-Path $env:USERPROFILE 'Prog')`"" $true
    Backup-Registry 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders'
    Set-ShellFolder 'Personal' 'D:\Docs'
    # A fresh process, as the wizard is: the shell may cache known folders inside this one.
    $documents = Run-Checked $script:powershell @('-NoProfile', '-Command', "[Environment]::GetFolderPath('MyDocuments')")
    Assert ($documents -eq 'D:\Docs') "Documents resolves to '$documents', not D:\Docs"
    $script:oneDrive = $env:OneDrive
    if (-not $script:oneDrive) {
        $script:oneDrive = Join-Path $env:USERPROFILE 'OneDrive'
        New-Item -ItemType Directory -Path $script:oneDrive -Force | Out-Null
        Set-ShellFolder $script:FOLDER_ONEDRIVE $script:oneDrive
    }
    $resolved = [NavoE2E.Known]::Path($script:FOLDER_ONEDRIVE)
    Assert ($resolved -eq $script:oneDrive) "OneDrive resolves to '$resolved', not $($script:oneDrive)"
    Write-Evidence "fixtures: C:\J -> C:\Windows, X: -> C:\Windows, Y: -> Prog, Documents -> D:\Docs, OneDrive -> $($script:oneDrive)"
}

# Runs subst.exe with a raw argument line, since it reads its own command line; checks the drive appeared or went.
function Invoke-Subst([string]$line, [bool]$create) {
    $process = Start-Process -FilePath subst.exe -ArgumentList $line -Wait -PassThru -NoNewWindow
    Assert ($process.ExitCode -eq 0) "subst $line exited $($process.ExitCode)"
    $drive = $line.Substring(0, 2) + '\'
    Assert ((Test-Path -LiteralPath $drive) -eq $create) "subst $line did not take effect"
}

# Removes S6's set-up; tries every step and throws the first error afterwards.
function Remove-S6Fixtures {
    $errors = @()
    foreach ($step in @({ if (Test-Path -LiteralPath 'X:\') { Invoke-Subst 'X: /D' $false } }, { if (Test-Path -LiteralPath 'Y:\') { Invoke-Subst 'Y: /D' $false } },
            { if (Test-Path -LiteralPath 'C:\J') { [IO.Directory]::Delete('C:\J') } }, { Restore-ShellFolders })) {
        try { & $step } catch { $errors += $_.Exception.Message }
    }
    Assert ($errors.Count -eq 0) "S6 cleanup: $($errors -join '; ')"
}

# Returns S6's refused candidates in the plan's order.
function Get-S6Rows {
    $user = $env:USERPROFILE
    @('C:\', 'C:\Windows\Navo', 'C:\PROGRA~1\Navo', 'C:\Program Files\Navo', 'C:\J\Navo', 'X:\Navo', 'Y:\Navo',
        (Join-Path $script:oneDrive 'Navo'), 'D:\Docs', (Join-Path $user 'Empty'), '\\localhost\c$\N', 'C:\a\..\Navo',
        "$user\a%TEMP%b\Navo", "$user\R&D\Navo", "$user\a^b\Navo", "$user\Hi!\Navo",
        (Join-Path $env:LOCALAPPDATA 'Navo'), ($env:LOCALAPPDATA.ToUpperInvariant() + '\navo\x\Navo'),
        (Join-Path $env:APPDATA 'Navo'), (Join-Path $env:LOCALAPPDATA 'NavoApp'))
}

# Returns the entries of every folder an S6 row could create something in.
function Get-S6Listing {
    $folders = @('C:\', 'C:\Windows', 'C:\Program Files', 'D:\', $env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA,
        $script:oneDrive, (Join-Path $env:USERPROFILE 'Empty'), (Join-Path $env:USERPROFILE 'Prog'))
    @(foreach ($folder in $folders) { Get-Entries $folder })
}

# Runs one S6 row: exit 3 and no new entry; stops the case when a row installed, since later rows would lie.
function Test-S6Row([string]$path) {
    Assert (-not (Test-Path -LiteralPath $script:uninstallKey)) "an install exists before row $path"
    $before = Get-S6Listing
    $result = Invoke-Silent '0.0.2' @('--installto', $path) $null 180
    $new = @(Get-S6Listing | Where-Object { $before -notcontains $_ })
    Add-Fault ($result.Code -eq 3) "$path exited $($result.Code), not 3: $($result.Error.Trim())"
    Add-Fault ($new.Count -eq 0) "$path created $($new -join ', ')"
    Add-Fault ($result.Seen.Count -eq 0) "$path started an installer: $($result.Seen -join '; ')"
    Assert ($result.Code -ne 0) "$path was installed; the later rows would not be valid"
}

# S6: every refused folder exits 3 and creates nothing; the four profile folders are checked absent first.
function Test-S6 {
    foreach ($folder in @('Navo', 'NavoApp')) {
        Assert (-not (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA $folder))) "%LocalAppData%\$folder exists"
    }
    Assert (-not (Test-Path -LiteralPath (Join-Path $env:APPDATA 'Navo'))) '%AppData%\Navo exists'
    try {
        New-S6Fixtures
        foreach ($path in (Get-S6Rows)) { Test-S6Row $path }
    } catch {
        # The first error is the verdict; a cleanup error after it only goes to the evidence.
        try { Remove-S6Fixtures } catch { Write-Evidence "cleanup after the failure: $($_.Exception.Message)" }
        throw
    }
    Remove-S6Fixtures
    Assert-Faults
}

# Evidence only, after a wrong exit: the same run with a deny of only file and folder creation.
function Get-S15Diagnosis([string]$root) {
    Run-Checked icacls.exe @($root, '/deny', 'u:(WD,AD)') | Out-Null
    try { Invoke-Silent '0.0.3' | Out-Null }
    finally { Run-Checked icacls.exe @($root, '/remove:d', 'u') | Out-Null }
}

# S15: a root u cannot write refuses the upgrade with exit 3, asks for no UAC and starts nothing.
function Test-S15 {
    $root = Install-Default
    $before = Get-HashListing $root
    $script:consentSeen = @()
    $poll = { $script:consentSeen += @(Get-CimInstance Win32_Process -Filter "Name='consent.exe'" | ForEach-Object { $_.ProcessId }) }
    Run-Checked icacls.exe @($root, '/deny', 'u:(W,AD)') | Out-Null
    try {
        Write-Evidence "root ACL during the run:`n$(Run-Checked icacls.exe @($root))"
        $result = Invoke-Silent '0.0.3' @() $poll
    } finally { Run-Checked icacls.exe @($root, '/remove:d', 'u') | Out-Null }
    if ($result.Code -ne 3) { Get-S15Diagnosis $root }
    Assert ($result.Code -eq 3) "exit $($result.Code), not 3: $($result.Error)"
    Assert ($result.Error -match 'Setup cannot write') "no 'Setup cannot write' in: $($result.Error.Trim())"
    Assert ($script:consentSeen.Count -eq 0) "consent.exe ran: $($script:consentSeen -join ', ')"
    Assert (@($result.Seen | Where-Object { $_ -match '^Update\.exe ' }).Count -eq 0) 'an Update.exe process started'
    Assert ((Compare-Object $before (Get-HashListing $root)) -eq $null) 'the root changed'
    Assert (-not (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'NavoApp'))) '%LocalAppData%\NavoApp appeared'
}
