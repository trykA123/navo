# Install and upgrade cases S1a, S1b, S2, S2b, S3, S8 and S9 of the installer's VM run (plan W15a).
# Also holds the window driver: real keys through SendInput, and page text read through UI Automation.

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @'
// Native calls for the VM cases: keyboard input, window focus, token elevation and file identity.
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
namespace NavoE2E {
    public static class Native {
        [StructLayout(LayoutKind.Explicit, Size = 40)]
        struct Input { [FieldOffset(0)] public uint Type; [FieldOffset(8)] public ushort Vk; [FieldOffset(10)] public ushort Scan; [FieldOffset(12)] public uint Flags; }
        [StructLayout(LayoutKind.Sequential, Pack = 4)]
        struct FileInfo { public uint Attributes; public long Created, Accessed, Written; public uint Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow; }
        delegate bool EnumProc(IntPtr window, IntPtr data);
        [DllImport("user32")] static extern uint SendInput(uint count, Input[] inputs, int size);
        [DllImport("user32")] static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32")] static extern bool SetCursorPos(int x, int y);
        [DllImport("user32")] static extern void mouse_event(uint flags, int x, int y, uint data, UIntPtr extra);
        public static void Click(int x, int y) { SetCursorPos(x, y); mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
        [DllImport("user32")] static extern IntPtr GetForegroundWindow();
        [DllImport("user32")] static extern bool ShowWindow(IntPtr window, int command);
        [DllImport("user32")] static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32")] static extern bool EnumWindows(EnumProc proc, IntPtr data);
        [DllImport("user32")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
        [DllImport("user32", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder name, int size);
        [DllImport("user32")] static extern bool AttachThreadInput(uint from, uint to, bool attach);
        [DllImport("kernel32")] static extern uint GetCurrentThreadId();
        [DllImport("kernel32")] static extern bool CloseHandle(IntPtr handle);
        [DllImport("kernel32", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
        [DllImport("advapi32", SetLastError = true)] static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
        [DllImport("advapi32", SetLastError = true)] static extern bool GetTokenInformation(IntPtr token, int kind, out int value, int size, out int written);
        [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)] static extern IntPtr CreateFile(string path, uint access, uint share, IntPtr security, uint mode, uint flags, IntPtr template);
        [DllImport("kernel32", SetLastError = true)] static extern bool GetFileInformationByHandle(IntPtr file, out FileInfo info);

        // Brings a window to the front by sharing input with the foreground thread; true on success.
        public static bool Focus(IntPtr window) {
            uint ignored; uint foreground = GetWindowThreadProcessId(GetForegroundWindow(), out ignored); uint own = GetCurrentThreadId();
            AttachThreadInput(own, foreground, true);
            ShowWindow(window, 9);
            bool done = SetForegroundWindow(window);
            AttachThreadInput(own, foreground, false);
            return done && GetForegroundWindow() == window;
        }
        static void Send(ushort vk, ushort scan, uint flags) {
            var input = new Input { Type = 1, Vk = vk, Scan = scan, Flags = flags };
            if (SendInput(1, new[] { input }, Marshal.SizeOf(typeof(Input))) != 1) throw new System.ComponentModel.Win32Exception();
            System.Threading.Thread.Sleep(30);
        }
        // Presses a key while holding the given modifier keys (virtual-key codes).
        public static void Press(ushort key, params ushort[] modifiers) {
            foreach (var m in modifiers) Send(m, 0, 0);
            Send(key, 0, 0); Send(key, 0, 2);
            for (int i = modifiers.Length - 1; i >= 0; i--) Send(modifiers[i], 0, 2);
        }
        // Types text as Unicode characters, independent of the keyboard layout.
        public static void Type(string text) {
            foreach (char c in text) { Send(0, c, 4); Send(0, c, 6); }
        }
        // Returns the visible common dialog of a process, or zero when none is open.
        public static IntPtr FindDialog(int pid) {
            IntPtr found = IntPtr.Zero;
            EnumWindows((window, data) => {
                uint owner; GetWindowThreadProcessId(window, out owner);
                var name = new StringBuilder(64); GetClassName(window, name, 64);
                if (owner == pid && IsWindowVisible(window) && name.ToString() == "#32770") { found = window; return false; }
                return true;
            }, IntPtr.Zero);
            return found;
        }
        // Returns whether a process token is elevated; throws when it cannot be read.
        public static bool Elevated(int pid) {
            IntPtr process = OpenProcess(0x1000, false, pid), token; int value, written;
            if (process == IntPtr.Zero || !OpenProcessToken(process, 8, out token)) throw new System.ComponentModel.Win32Exception();
            bool read = GetTokenInformation(token, 20, out value, 4, out written);
            CloseHandle(token); CloseHandle(process);
            if (!read) throw new System.ComponentModel.Win32Exception();
            return value != 0;
        }
        // Returns a folder's volume and file index, which a rename keeps and a replacement changes.
        public static string FileId(string path) {
            IntPtr file = CreateFile(path, 0x80, 7, IntPtr.Zero, 3, 0x02000000, IntPtr.Zero); FileInfo info;
            if (file == new IntPtr(-1)) throw new System.ComponentModel.Win32Exception();
            bool read = GetFileInformationByHandle(file, out info);
            CloseHandle(file);
            if (!read) throw new System.ComponentModel.Win32Exception();
            return info.Volume + ":" + info.IndexHigh + ":" + info.IndexLow;
        }
    }
}
'@

$script:VK_ALT = [uint16]0x12
$script:VK_CONTROL = [uint16]0x11
$script:VK_RETURN = [uint16]0x0D

# Starts a wizard with its pack embedded, without arguments, and returns the process and its window.
function Start-Wizard([string]$version) {
    $process = Start-Interactive (Join-Path $wizards "$version\Navo-Setup.exe") @() $false
    Assert $process "wizard $version exited at once"
    $window = Wait-MainWindow $process
    [pscustomobject]@{ Process = $process; Window = $window }
}

# Waits up to 60 s for a process's main window and returns its handle.
function Wait-MainWindow([Diagnostics.Process]$process) {
    Wait-Until { $process.Refresh(); $process.HasExited -or $process.MainWindowHandle -ne [IntPtr]::Zero } 60 'no main window'
    Assert (-not $process.HasExited) "the process exited with $($process.ExitCode) before showing a window"
    $process.MainWindowHandle
}

# Returns every name and value UI Automation exposes under a window, one per line.
function Get-UiText([IntPtr]$window) {
    $root = [Windows.Automation.AutomationElement]::FromHandle($window)
    $all = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    $lines = foreach ($element in $all) {
        $element.Current.Name
        $pattern = $null
        if ($element.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) { $pattern.Current.Value }
    }
    @($lines | Where-Object { $_ }) -join "`n"
}

# Waits until the window's text matches a regex; records the page text on a timeout.
function Wait-UiText([IntPtr]$window, [string]$pattern, [int]$seconds = 30) {
    try { Wait-Until { (Get-UiText $window) -match $pattern } $seconds "no text /$pattern/ in the window" }
    catch { Write-Evidence "page text at timeout:`n$(Get-UiText $window)"; throw }
    Write-Evidence "page matched /$pattern/"
}

# Sends Alt+letter to the window after giving it focus; throws when it cannot take focus.
function Send-Alt([IntPtr]$window, [char]$letter) {
    Wait-Until { [NavoE2E.Native]::Focus($window) } 10 'the window did not take focus'
    [NavoE2E.Native]::Press([uint16][char]::ToUpper($letter), $script:VK_ALT)
}

# Presses Alt+letter until the next page's text shows; a press is repeated only if the page did not move.
function Step-Page([IntPtr]$window, [char]$letter, [string]$pattern, [int]$seconds = 30) {
    $deadline = [DateTime]::UtcNow.AddSeconds($seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        Send-Alt $window $letter
        try { Wait-UiText $window $pattern 4; return } catch { Start-Sleep -Milliseconds 200 }
    }
    Write-Evidence "page text at timeout:`n$(Get-UiText $window)"
    throw "Alt+$letter did not reach /$pattern/"
}

# Walks Welcome and Licence, ending on the Folder page.
function Step-ToFolder($wizard, [string]$welcome) {
    Wait-UiText $wizard.Window $welcome 60
    Step-Page $wizard.Window 'n' 'I accept the licence'
    Send-Alt $wizard.Window 'a'
    Start-Sleep -Milliseconds 300
    Step-Page $wizard.Window 'n' 'Uninstalling Navo deletes it|is installed in|Install folder'
}

# From the Folder page of a fresh install: Next, Install, then waits for Finish, at most 300 s.
function Step-FreshInstall($wizard) {
    Step-Page $wizard.Window 'n' 'Start menu' 60
    Step-Page $wizard.Window 'i' 'Navo is installed in' 300
}

# Leaves the Finish page, unticking Start Navo first when asked; waits for the wizard to exit.
function Finish-Wizard($wizard, [bool]$startNavo) {
    if (-not $startNavo) { Send-Alt $wizard.Window 's'; Start-Sleep -Milliseconds 300 }
    Send-Alt $wizard.Window 'f'
    Assert ($wizard.Process.WaitForExit(30000)) 'the wizard did not exit after Finish'
}

# Returns the S2 parent folder, whose last letter is U+0102; it is built here because .ps1 files are read as ANSI.
function Get-ProgFolder { Join-Path $env:USERPROFILE ('Prog ' + [char]0x0102) }

# S1a: fresh silent install in the default folder, on a profile with no Programs folder.
function Test-S1a {
    $root = Get-DefaultRoot
    $result = Invoke-Silent '0.0.2'
    Assert ($result.Code -eq 0) "silent exit $($result.Code): $($result.Error)"
    Assert-Installed $root '0.0.2'
    $links = Get-LinkPaths
    Assert-LinkToRoot $links.Desktop $root
    Assert-LinkToRoot $links.Start $root
    Assert-RunLogs
    Start-Sleep -Seconds 5
    Assert (@(Get-RootApps $root).Count -eq 0) 'silent install started navo.exe'
}

# S1b: fresh window install in the default folder; Finish starts Navo unelevated.
function Test-S1b {
    $root = Get-DefaultRoot
    $wizard = Start-Wizard '0.0.2'
    Step-ToFolder $wizard 'Install Navo 0\.0\.2'
    Step-FreshInstall $wizard
    Finish-Wizard $wizard $true
    Assert-Installed $root '0.0.2'
    $links = Get-LinkPaths
    Assert-LinkToRoot $links.Desktop $root
    Assert-LinkToRoot $links.Start $root
    Assert-RunLogs
    Wait-Until { @(Get-RootApps $root).Count -gt 0 } 30 'Finish did not start Navo'
    $app = @(Get-RootApps $root)[0]
    Assert (-not [NavoE2E.Native]::Elevated($app.ProcessId)) 'Navo runs elevated'
    Write-Evidence "navo pid=$($app.ProcessId) TokenElevation=false"
}

# Chooses the parent folder in the Browse dialog by typing its path, then waits for the field to show it.
function Select-Folder($wizard, [string]$parent) {
    Send-Alt $wizard.Window 'r'
    $dialog = [IntPtr]::Zero
    Wait-Until { $script:found = [NavoE2E.Native]::FindDialog($wizard.Process.Id); $script:found -ne [IntPtr]::Zero } 30 'Browse opened no dialog'
    $dialog = $script:found
    Wait-Until { [NavoE2E.Native]::Focus($dialog) } 10 'the dialog did not take focus'
    Start-Sleep -Seconds 1
    [NavoE2E.Native]::Type($parent)
    # The first Enter may only navigate into the folder; a later one selects it.
    Wait-Until {
        if ([NavoE2E.Native]::FindDialog($wizard.Process.Id) -eq [IntPtr]::Zero) { return $true }
        [NavoE2E.Native]::Focus($dialog) | Out-Null
        [NavoE2E.Native]::Press($script:VK_RETURN)
        Start-Sleep -Seconds 2
        $false
    } 30 'the dialog did not close'
    Wait-UiText $wizard.Window ([regex]::Escape((Join-Path $parent 'Navo'))) 15
}

# S2: Browse to a non-ASCII folder, install 0.0.2, then update to 0.0.3 through Navo's Restart.
function Test-S2 {
    $parent = Get-ProgFolder
    $root = Join-Path $parent 'Navo'
    New-Item -ItemType Directory -Path $parent | Out-Null
    $wizard = Start-Wizard '0.0.2'
    Step-ToFolder $wizard 'Install Navo 0\.0\.2'
    Select-Folder $wizard $parent
    Step-FreshInstall $wizard
    Finish-Wizard $wizard $false
    Assert ((Read-SqVersion $root) -eq '0.0.2') 'the install is not 0.0.2'
    $app = Start-Staged $root (New-Profile)
    Invoke-NavoRestart (Wait-MainWindow $app)
    Wait-Version $root '0.0.3'
    Assert ((Read-Marker $root).packId -eq 'NavoApp') 'the marker is gone'
    Assert (-not (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'NavoApp\packages'))) '%LocalAppData%\NavoApp\packages exists'
    Write-Evidence "root=$root version=0.0.3"
    Wait-Until { @(Get-RootApps $root).Count -gt 0 } 60 'Navo did not restart'
    Stop-Root $root
}

# Returns the SHA-256 of the updater inside a pack's full nupkg (the entry named Squirrel.exe).
function Get-PackUpdaterHash([string]$version) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead((Get-PackFile $version '*-full.nupkg'))
    try {
        $entries = @($archive.Entries | Where-Object { $_.Name -eq 'Squirrel.exe' })
        Assert ($entries.Count -eq 1) "expected one Squirrel.exe in the $version nupkg"
        $stream = $entries[0].Open()
        try { (Get-FileHash -InputStream $stream).Hash } finally { $stream.Dispose() }
    } finally { $archive.Dispose() }
}

# Tries to rename a folder while the apply runs; records and returns whether the rename succeeded.
function Test-Rename([string]$path) {
    try {
        Rename-Item -LiteralPath $path -NewName ((Split-Path $path -Leaf) + '-moved')
        Write-Evidence "rename of $path SUCCEEDED during apply"
        $true
    } catch { Write-Evidence "rename of $path refused during apply: $($_.Exception.Message)"; $false }
}

# S2b: upgrade through Update.exe apply; the root and its parent refuse a rename while it runs.
function Test-S2b {
    $parent = Get-ProgFolder
    $root = Join-Path $parent 'Navo'
    New-Item -ItemType Directory -Path $parent | Out-Null
    $result = Invoke-Silent '0.0.2' @('--installto', $root)
    Assert ($result.Code -eq 0) "install exit $($result.Code): $($result.Error)"
    $desktop = (Get-LinkPaths).Desktop
    Remove-Item -LiteralPath $desktop
    $id = [NavoE2E.Native]::FileId($root)
    $installId = (Read-Marker $root).installId
    $script:renames = $null
    $poll = {
        if ($null -eq $script:renames -and @(Get-InstallerProcesses | Where-Object { $_ -match '^Update\.exe .*\bapply\b' }).Count -gt 0) {
            $script:renames = @((Test-Rename $root), (Test-Rename $parent))
        }
    }
    $result = Invoke-Silent '0.0.3' @() $poll
    Assert ($result.Code -eq 0) "upgrade exit $($result.Code): $($result.Error)"
    Assert ($null -ne $script:renames) 'Update.exe apply was never observed'
    Assert ($script:renames -notcontains $true) 'a rename succeeded during apply'
    Assert ((Read-SqVersion $root) -eq '0.0.3') 'sq.version is not 0.0.3'
    Assert ((Get-FileHash -LiteralPath (Join-Path $root 'Update.exe')).Hash -eq (Get-PackUpdaterHash '0.0.3')) 'Update.exe is not the 0.0.3 one'
    Assert (Test-Path -LiteralPath (Join-Path $root 'packages\NavoApp-0.0.3-full.nupkg')) 'packages holds no 0.0.3 nupkg'
    Assert ([NavoE2E.Native]::FileId($root) -eq $id) 'the root was replaced'
    Assert ((Read-Marker $root).installId -eq $installId) 'the installId changed'
    Assert (-not (Test-Path -LiteralPath $desktop)) 'the Desktop link came back'
    Assert (@($result.Seen | Where-Object { $_ -match '^(NavoApp-win-)?Setup\.exe ' }).Count -eq 0) 'a Setup.exe process ran'
    Assert (@(Get-RootApps $root).Count -eq 0) 'Navo started'
    $moved = "$parent-moved"
    Rename-Item -LiteralPath $parent -NewName (Split-Path $moved -Leaf)
    Rename-Item -LiteralPath $moved -NewName (Split-Path $parent -Leaf)
    Write-Evidence 'positive control: renaming the parent after the wizard succeeded and was undone'
}

# S3: the wizard upgrades a raw Setup.exe install in place and keeps its settings.
function Test-S3 {
    $root = Join-Path $env:LOCALAPPDATA 'NavoApp'
    Install-Raw (Get-PackFile '0.0.1' '*Setup.exe') '0.0.1'
    $settings = Join-Path $env:APPDATA 'Navo'
    New-Item -ItemType Directory -Path $settings -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $settings 'e2e-settings.toml') -Value 'kept = true'
    $before = Get-HashListing $settings
    $id = [NavoE2E.Native]::FileId($root)
    $wizard = Start-Wizard '0.0.2'
    Step-ToFolder $wizard 'Install Navo 0\.0\.2'
    Wait-UiText $wizard.Window ([regex]::Escape($root)) 10
    $page = Get-UiText $wizard.Window
    $all = [Windows.Automation.AutomationElement]::FromHandle($wizard.Window).FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    $browse = @($all | Where-Object { $_.Current.Name -match '^Browse' })
    Assert ($browse.Count -eq 1 -and -not $browse[0].Current.IsEnabled) 'Browse is not off on an upgrade'
    Send-Alt $wizard.Window 'r'
    Start-Sleep -Seconds 3
    Assert ([NavoE2E.Native]::FindDialog($wizard.Process.Id) -eq [IntPtr]::Zero) 'Alt+R opened Browse on an upgrade'
    Write-Evidence "locked Folder page text:`n$page"
    Step-Page $wizard.Window 'i' 'Navo is installed in' 300
    Finish-Wizard $wizard $false
    Assert ([NavoE2E.Native]::FileId($root) -eq $id) 'the root was replaced'
    Assert ((Read-SqVersion $root) -eq '0.0.2') 'sq.version is not 0.0.2'
    Assert ((Read-Marker $root).packId -eq 'NavoApp') 'no marker was written'
    # -Filter 'NavoApp.*' would also match NavoApp itself (the Win32 wildcard quirk), so -like decides.
    $siblings = @(Get-ChildItem -LiteralPath $env:LOCALAPPDATA -Directory | Where-Object { $_.Name -like 'NavoApp.*' })
    Assert ($siblings.Count -eq 0) "a sibling appeared: $(($siblings | ForEach-Object Name) -join ', ')"
    Assert ((Compare-Object $before (Get-HashListing $settings)) -eq $null) 'the settings changed'
}

# S8: a same or older wizard exits 8, starts nothing and leaves the root unchanged.
function Test-S8 {
    $root = Get-DefaultRoot
    $result = Invoke-Silent '0.0.2'
    Assert ($result.Code -eq 0) "install exit $($result.Code): $($result.Error)"
    $before = Get-HashListing $root
    foreach ($version in @('0.0.2', '0.0.1')) {
        $result = Invoke-Silent $version
        Assert ($result.Code -eq 8) "wizard $version exited $($result.Code), not 8: $($result.Error)"
        Assert ($result.Seen.Count -eq 0) "wizard $version started: $($result.Seen -join '; ')"
    }
    Assert ((Compare-Object $before (Get-HashListing $root)) -eq $null) 'the root changed'
    Write-Evidence "root unchanged across $($before.Count) files"
}

# S9a: an unticked Desktop link stays absent after a wizard upgrade.
function Test-S9a {
    $root = Get-DefaultRoot
    $links = Get-LinkPaths
    $result = Invoke-Silent '0.0.2' @('--no-desktop')
    Assert ($result.Code -eq 0) "install exit $($result.Code): $($result.Error)"
    Assert (-not (Test-Path -LiteralPath $links.Desktop)) 'the unticked Desktop link exists after install'
    Assert-LinkToRoot $links.Start $root
    $result = Invoke-Silent '0.0.3'
    Assert ($result.Code -eq 0) "upgrade exit $($result.Code): $($result.Error)"
    Assert ((Read-SqVersion $root) -eq '0.0.3') 'sq.version is not 0.0.3'
    Assert (-not (Test-Path -LiteralPath $links.Desktop)) 'the unticked Desktop link came back after apply'
    Assert-LinkToRoot $links.Start $root
}

# S9b: an unticked Start menu link stays absent after an update through Navo.
function Test-S9b {
    $root = Get-DefaultRoot
    $links = Get-LinkPaths
    $result = Invoke-Silent '0.0.2' @('--no-start-menu')
    Assert ($result.Code -eq 0) "install exit $($result.Code): $($result.Error)"
    Assert (-not (Test-Path -LiteralPath $links.Start)) 'the unticked Start menu link exists after install'
    Assert-LinkToRoot $links.Desktop $root
    $app = Start-Staged $root (New-Profile)
    Invoke-NavoRestart (Wait-MainWindow $app)
    Wait-Version $root '0.0.3'
    Wait-Until { @(Get-RootApps $root).Count -gt 0 } 60 'Navo did not restart'
    Stop-Root $root
    Assert (-not (Test-Path -LiteralPath $links.Start)) 'the unticked Start menu link came back after the update'
    Assert-LinkToRoot $links.Desktop $root
}
