# Exercises IX8v service migration, rollback, removal and contention in the Windows VM.
# Uses the IX8r process, profile and evidence conventions; UAC stays an owner check.
#requires -Version 5.1
param(
    [string]$Out,
    [string]$WorkDir,
    [string]$LegacyExe,
    [string]$LegacySetup,
    [string]$Setup,
    [string]$Revision2Exe,
    [ValidateSet('Run', 'AfterReboot')][string]$Phase = 'Run',
    [ValidateSet('1','2','2b','3','4a','4b','4c','5','6','7')][string[]]$Checks = @('1','2','2b','3','4a','4b','4c','5','7','6'),
    [ValidateSet('', 'Lock', 'Mapped', 'Refused', 'Global')][string]$Hold = '',
    [string]$Target,
    [string]$Signal
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Holds a Windows resource in a separate process until released or its safety deadline expires.
function Hold-Resource {
    $resource = $null
    try {
        switch ($Hold) {
            'Global' { $resource = New-Object Threading.Mutex($true, 'Global\NavoIndexServiceChange') }
            'Lock' { $resource = [IO.File]::Open($Target, 'OpenOrCreate', 'ReadWrite', 'None') }
            'Refused' { $resource = [IO.File]::Open($Target, 'Open', 'Read', 'ReadWrite') }
            'Mapped' {
                Add-Type @'
// Keeps an executable mapped as an image, allowing rename while blocking deletion.
using System;
using System.Runtime.InteropServices;
public static class ImageHold {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    // Maps the image; a null handle reports a Win32 failure.
    public static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll")]
    // Releases the mapped image handle.
    public static extern bool FreeLibrary(IntPtr image);
}
'@
                $resource = [ImageHold]::LoadLibraryEx($Target, [IntPtr]::Zero, 0x20)
                if ($resource -eq [IntPtr]::Zero) { throw "image mapping failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
            }
        }
        $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
        "pid=$PID; mode=$Hold; elevated=$($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))" | Set-Content "$Signal.ready"
        $deadline = [DateTime]::UtcNow.AddSeconds(240)
        while (-not (Test-Path "$Signal.release") -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    } finally {
        if ($resource -is [IntPtr]) { [void][ImageHold]::FreeLibrary($resource) }
        elseif ($null -ne $resource) { $resource.Dispose() }
    }
}
if ($Hold) {
    try { Hold-Resource }
    catch {
        $_ | Out-String | Set-Content (Join-Path $env:TEMP ([IO.Path]::GetFileName($Signal) + '.error'))
        throw
    }
    exit 0
}
$repo = (Get-Location).Path
$installRoot = Join-Path $env:LOCALAPPDATA 'NavoApp'
$app = Join-Path $installRoot 'current\navo.exe'
$parent = Join-Path $env:ProgramFiles 'Navo'
$indexDir = Join-Path $parent 'Index'
$powerShell = Join-Path $PSHOME 'powershell.exe'
$failed = $false
$evidence = @()

# Records an observation in the transcript and the current check report.
function Write-Evidence([string]$message) {
    $script:evidence += $message
    Write-Host "EVIDENCE $message"
}

# Fails a check when its observed condition is false.
function Assert($condition, [string]$reason) {
    if (-not $condition) { throw $reason }
}

# Quotes Windows process arguments, including embedded quotes and trailing slashes.
function Quote-Arguments([string[]]$values) {
    ($values | ForEach-Object {
        '"' + [regex]::Replace([regex]::Replace($_, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
    }) -join ' '
}

# Runs a console or GUI executable with captured output and a bounded timeout.
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

# Reads the Navo Index SCM registration, or returns no service.
function Get-IndexService {
    Get-CimInstance Win32_Service -Filter "Name='Navo Index'"
}

# Waits for an observable condition and fails after the supplied seconds.
function Wait-Until([scriptblock]$condition, [int]$seconds, [string]$reason) {
    $deadline = [DateTime]::UtcNow.AddSeconds($seconds)
    do {
        if (& $condition) { return }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $reason
}

# Runs a checked fixture command and preserves its output and elapsed seconds.
function Invoke-CommandChecked([string]$exe, [string[]]$arguments, [int]$seconds = 120) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $result = Run-Process $exe $arguments $seconds
    Write-Evidence "command=$([IO.Path]::GetFileName($exe)) $($arguments -join ' '); exit=$($result.Code); seconds=$([math]::Round($watch.Elapsed.TotalSeconds, 2)); $($result.Text.Trim()) $($result.Error.Trim())"
    Assert ($result.Code -eq 0) "command failed with exit $($result.Code)"
}

# Runs an elevated service action and records exit, output and total elapsed seconds.
function Invoke-Service([string]$action) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $result = Run-Process $app @('--index-service', $action) 100
    Write-Evidence "$action exit=$($result.Code); seconds=$([math]::Round($watch.Elapsed.TotalSeconds, 2)); $($result.Text.Trim()) $($result.Error.Trim())"
    [pscustomobject]@{ Code = $result.Code; Seconds = $watch.Elapsed.TotalSeconds; Error = $result.Error }
}

# Stops only GUI processes in this run's install, including its PDF helper children.
function Stop-Apps {
    foreach ($record in @(Get-CimInstance Win32_Process -Filter "Name='navo.exe'")) {
        if ($record.ExecutablePath -ne $app) { continue }
        $process = Get-Process -Id $record.ProcessId -ErrorAction SilentlyContinue
        if ($process) {
            [void]$process.CloseMainWindow()
            if (-not $process.WaitForExit(10000)) { $process.Kill(); [void]$process.WaitForExit(10000) }
        }
    }
}

# Creates an isolated profile with application updates disabled.
function New-Profile {
    $profile = Join-Path $caseDir ('profile-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory (Join-Path $profile 'config') -Force | Out-Null
    "[updates]`ncheck = false`n[tray]`nkeep-running = false" | Set-Content (Join-Path $profile 'config\navo.toml') -Encoding Ascii
    $script:profiles += $profile
    $profile
}

# Starts the installed application with debug service events in an isolated profile.
function Start-App([string]$profile) {
    $psi = New-Object Diagnostics.ProcessStartInfo($app, (Quote-Arguments @('--profile', $profile, '--left', $caseDir)))
    $psi.UseShellExecute = $false
    $psi.EnvironmentVariables['NAVO_LOG'] = 'info,navo::service_version_step=debug'
    [Diagnostics.Process]::Start($psi)
}

# Reads complete JSON log lines from this check's profiles.
function Read-Events {
    foreach ($profile in $profiles) {
        $logs = Join-Path $profile 'data\logs'
        if (-not (Test-Path $logs)) { continue }
        foreach ($file in @(Get-ChildItem $logs -Filter 'navo*.log')) {
            foreach ($line in @(Get-Content $file.FullName)) {
                if ($line.EndsWith('}}')) { $line | ConvertFrom-Json }
            }
        }
    }
}

# Returns this check's service events with the requested message.
function Events([string]$message) { @(Read-Events | Where-Object msg -eq $message) }

# Captures command, description, recovery and delayed start for rollback comparison.
function Read-Configuration {
    $service = Get-IndexService
    Assert ($null -ne $service) 'service is missing'
    [pscustomobject]@{
        Path = $service.PathName
        Description = $service.Description
        Recovery = (Run-Process sc.exe @('qfailure', 'Navo Index')).Text
        FailureFlag = (Run-Process sc.exe @('qfailureflag', 'Navo Index')).Text
        Delayed = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\Navo Index').DelayedAutoStart
    } | ConvertTo-Json -Compress
}

# Recreates the real revision-2 navo.exe service, with distinctive rollback configuration.
function Reset-Legacy {
    Stop-Apps
    $result = Invoke-Service 'remove'
    Assert ($result.Code -in @(0, 6)) 'fixture removal failed'
    Assert (-not (Get-IndexService)) 'fixture service still registered'
    Invoke-CommandChecked $LegacyExe @('--index-service', 'install')
    Invoke-CommandChecked sc.exe @('description', 'Navo Index', 'IX8v legacy rollback description')
    Invoke-CommandChecked sc.exe @('failure', 'Navo Index', 'reset=', '86400', 'actions=', 'restart/7000/restart/11000/none/0')
    Invoke-CommandChecked sc.exe @('failureflag', 'Navo Index', '1')
    Assert ((Get-IndexService).PathName -match 'navo\.exe" --index-service run$') 'legacy fixture command differs'
    Assert (-not (Test-Path (Join-Path $indexDir 'navo-index.exe'))) 'legacy fixture contains new image'
}

# Checks the service's new image, process description, SCM description and pipe process identity.
function Assert-Migrated {
    $service = Get-IndexService
    Assert ($service -and $service.State -eq 'Running') 'migrated service is not running'
    Assert ($service.PathName -eq ('"' + (Join-Path $indexDir 'navo-index.exe') + '" run')) 'wrong migrated command'
    Assert ($service.Description -eq 'Keeps the file index for Navo search up to date.') 'wrong migrated description'
    $process = Get-Process -Id $service.ProcessId
    Assert ($process.MainModule.FileVersionInfo.FileDescription -eq 'Navo Index') 'wrong process description'
    $memory = Run-Process $app @('--index-memory', '--json') 20
    Assert ($memory.Code -eq 0) 'pipe memory query failed'
    $answer = $memory.Text | ConvertFrom-Json
    Assert ($answer.service_process_id -eq $service.ProcessId) 'pipe and SCM process differ'
    Assert (-not (Test-Path (Join-Path $indexDir 'navo.exe'))) 'legacy image remains'
    Write-Evidence "state=$($service.State); command=$($service.PathName); description=$($service.Description); pipe_pid=$($answer.service_process_id); scm_pid=$($service.ProcessId); files=$((Get-ChildItem $indexDir).Name -join ',')"
}

# Starts a separate file or mutex holder and waits for its ready marker.
function Start-Holder([string]$mode, [string]$path, [switch]$Limited) {
    $signal = Join-Path $caseDir ('hold-' + [guid]::NewGuid().ToString('N'))
    $arguments = Quote-Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath, '-Hold', $mode, '-Target', $path, '-Signal', $signal)
    $task = $null
    if ($Limited) {
        $task = 'NavoIndexE2E-' + [guid]::NewGuid().ToString('N')
        $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $principal = New-ScheduledTaskPrincipal -UserId $sid -LogonType Interactive -RunLevel Limited
        @{ script = $PSCommandPath; target = $path; signal = $signal } | ConvertTo-Json | Set-Content "$signal.json"
        @'
# Launches the mutex fixture under the scheduled task's limited token.
param([string]$Config)
$ErrorActionPreference = 'Stop'
$settings = Get-Content $Config -Raw | ConvertFrom-Json
try { & $settings.script -Hold Global -Target $settings.target -Signal $settings.signal }
catch { $_ | Out-String | Set-Content ($settings.signal + '.error'); exit 1 }
'@ | Set-Content "$signal.ps1"
        $taskArguments = '-NoProfile -ExecutionPolicy Bypass -File ' + (Quote-Arguments @("$signal.ps1")) + ' -Config ' + (Quote-Arguments @("$signal.json"))
        $action = New-ScheduledTaskAction -Execute $powerShell -Argument $taskArguments
        Register-ScheduledTask -TaskName $task -Action $action -Principal $principal | Out-Null
        Write-Evidence "holder_task=$task; execute=$($action.Execute); arguments=$($action.Arguments)"
        Start-ScheduledTask -TaskName $task
    } else {
        $process = Start-Process $powerShell -ArgumentList $arguments -PassThru -RedirectStandardError "$signal.error"
    }
    try {
        Wait-Until { Test-Path "$signal.ready" } 20 "holder $mode did not become ready"
        $ready = Get-Content "$signal.ready"
        Write-Evidence $ready
        if ($Limited) { Assert ($ready -match 'elevated=False') 'global mutex holder is elevated' }
        $holderId = [int]([regex]::Match($ready, 'pid=(\d+)').Groups[1].Value)
        [pscustomobject]@{ Signal = $signal; Process = (Get-Process -Id $holderId) }
    } finally { if ($task) { Unregister-ScheduledTask -TaskName $task -Confirm:$false } }
}

# Releases a holder and fails if its process does not exit in ten seconds.
function Stop-Holder($holder) {
    New-Item -ItemType File "$($holder.Signal).release" -Force | Out-Null
    if (-not $holder.Process.WaitForExit(10000)) { $holder.Process.Kill(); throw 'holder required termination' }
}

# Reads Windows pending rename/delete pairs, returning an empty array when none exist.
function Read-PendingDeletes {
    $key = Get-Item 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager'
    try { $key.GetValue('PendingFileRenameOperations', @()) }
    finally { $key.Dispose() }
}

# Tests failure before commit and compares every captured SCM field and restored image hash.
function Test-Rollback([string]$fixture, [bool]$migrated, [bool]$mismatch) {
    Reset-Legacy
    if ($migrated) { Assert ((Invoke-Service 'install').Code -eq 0) 'migration prerequisite failed' }
    $before = Read-Configuration
    $image = if ($migrated) { 'navo-index.exe' } else { 'navo.exe' }
    $hash = (Get-FileHash (Join-Path $indexDir $image)).Hash
    Write-Evidence "before=$before; image_sha256=$hash"
    Copy-Item $fixture $sibling -Force
    try { $result = Invoke-Service 'install' }
    finally { Copy-Item $savedSibling $sibling -Force }
    $after = Read-Configuration
    Write-Evidence "after=$after; state=$((Get-IndexService).State); files=$((Get-ChildItem $indexDir).Name -join ',')"
    Assert ($result.Code -ne 0) 'broken replacement unexpectedly succeeded'
    if ($mismatch) { Assert ($result.Code -eq 3) 'revision-2 replacement did not return mismatch exit 3' }
    Assert ($after -eq $before) 'rollback did not restore SCM configuration'
    Assert ((Get-IndexService).State -eq 'Running') 'old service did not restart'
    Assert ((Get-FileHash (Join-Path $indexDir $image)).Hash -eq $hash) 'restored image hash differs'
    if (-not $migrated) { Assert (-not (Test-Path (Join-Path $indexDir 'navo-index.exe'))) 'replacement image survived rollback' }
    else { Assert-Migrated }
}
# Exercises the requested numbered scenario; human-only portions raise an explicit owner marker.
function Test-Check([string]$number) {
    switch ($number) {
        '1' {
            Reset-Legacy
            $profile = New-Profile
            Start-App $profile | Out-Null
            Wait-Until { @(Events 'index service updated').Count -eq 1 } 100 'automatic migration did not complete'
            Assert-Migrated
            Stop-Apps
            Start-App $profile | Out-Null
            Wait-Until { @(Events 'index service step' | Where-Object { $_.fields.version -match 'revision: 3\b' -and $_.fields.step -eq 'Nothing' }).Count -gt 0 } 30 'no revision-3 answer after restart'
            Assert (@(Events 'index service is older than Navo; asking to update it').Count -eq 1) 'repeat start requested another update'
            throw 'OWNER: normal unelevated session: install v0.2.2 with Setup, install IX8v with Setup, accept exactly one UAC prompt; verify Task Manager Navo Index and Settings Running; restart and verify no prompt.'
        }
        '2' {
            Reset-Legacy
            Copy-Item $LegacyExe $sibling -Force
            $profile = New-Profile
            try {
                Start-App $profile | Out-Null
                Wait-Until { Test-Path (Join-Path $profile 'data\index-service-declined') } 100 'failed automatic install did not persist declined revision'
                Stop-Apps
            } finally { Copy-Item $savedSibling $sibling -Force }
            $marker = Get-Content (Join-Path $profile 'data\index-service-declined') -Raw
            Write-Evidence "automatic failure marker=$($marker.Trim()); this tests the shared failed/declined path, not a UAC cancellation"
            Assert ($marker.Trim() -eq '3') 'declined revision marker differs'
            Start-App $profile | Out-Null
            Wait-Until { @(Events 'index service step' | Where-Object { $_.fields.step -eq 'Outdated' }).Count -gt 0 } 30 'repeat start did not retain Outdated'
            Assert (@(Events 'index service is older than Navo; asking to update it').Count -eq 1) 'failed/declined revision requested again'
            Stop-Apps
            Assert ((Invoke-Service 'install').Code -eq 0) 'manual install equivalent failed'
            Assert-Migrated
            throw 'OWNER: normal unelevated session with revision-2 service: decline the first prompt, restart the same profile, verify no second prompt; Settings > Search must show Older than Navo and Update; click Update, accept exactly one prompt, verify Running.'
        }
        '2b' {
            Reset-Legacy
            $first = New-Profile
            $second = New-Profile
            Start-App $first | Out-Null
            Start-App $second | Out-Null
            Wait-Until { @(Events 'index service updated').Count -eq 1 } 100 'two-profile migration did not finish'
            Wait-Until { @(Events 'index service step' | Where-Object { $_.fields.step -eq 'Nothing' }).Count -gt 0 -or @(Events 'index service step: another Navo is changing the service').Count -gt 0 } 30 'second profile did not observe migration or lock'
            Assert (@(Events 'index service is older than Navo; asking to update it').Count -eq 1) 'two profiles requested multiple updates'
            Assert-Migrated
            throw 'OWNER: normal unelevated session with revision-2 service: launch two installed profiles simultaneously; count exactly one UAC prompt in total; accept it and verify both profiles show the migrated service.'
        }
        '3' {
            Reset-Legacy
            $oldId = (Get-IndexService).ProcessId
            Stop-Service 'Navo Index'
            Wait-Until { (Get-IndexService).State -eq 'Stopped' -and -not (Get-Process -Id $oldId -ErrorAction SilentlyContinue) } 20 'old service process did not stop'
            Write-Evidence "old_pid=$oldId; state=$((Get-IndexService).State); command=$((Get-IndexService).PathName)"
            Start-App (New-Profile) | Out-Null
            Wait-Until { @(Events 'index service step').Count -gt 0 } 30 'no startup service query'
            Assert (@(Events 'index service is older than Navo; asking to update it').Count -eq 0) 'stopped service requested automatic elevation'
            Assert ((Get-IndexService).State -eq 'Stopped') 'startup changed stopped service'
            Stop-Apps
            Assert ((Invoke-Service 'install').Code -eq 0) 'stopped legacy service did not migrate'
            Assert-Migrated
            throw 'OWNER: stop the revision-2 navo.exe service, start the installed IX8v app unelevated, verify no prompt; Settings > Search must offer Update; click it, accept one prompt and verify Running.'
        }
        '4a' { Test-Rollback $LegacyExe $false $false; return 'refused run restored legacy command, description, recovery, delayed start, hash and Running; new image absent' }
        '4b' { Test-Rollback $Revision2Exe $false $true; return 'revision-2 answer returned exit 3 and restored complete legacy state' }
        '4c' { Test-Rollback $LegacyExe $true $false; return 'failed reinstall restored previous navo-index.exe hash, complete SCM configuration and Running' }
        '5' {
            Reset-Legacy
            Assert ((Invoke-Service 'install').Code -eq 0) 'migration failed'
            $result = Invoke-Service 'remove'
            $serviceLeft = [bool](Get-IndexService)
            $folderLeft = Test-Path $parent
            Write-Evidence "service_after_remove=$serviceLeft; Index_after_remove=$(Test-Path $indexDir); Navo_after_remove=$folderLeft"
            Invoke-CommandChecked (Join-Path $installRoot 'Update.exe') @('uninstall', '--silent')
            Wait-Until { -not (Test-Path $installRoot) } 30 'uninstall left application install'
            Assert ($result.Code -eq 0 -and -not $serviceLeft) 'ordinary removal failed'
            Assert (-not $folderLeft) 'remove left Program Files\Navo; no workaround applied'
            throw 'OWNER: Settings > Search > Remove, then uninstall; verify uninstall log and Settings action. CLI removal and silent uninstall were exercised.'
        }
        '7' {
            Reset-Legacy
            $holder = Start-Holder 'Global' '-' -Limited
            try { Assert ((Invoke-Service 'install').Code -eq 0) 'old global mutex blocked elevated install' }
            finally { Stop-Holder $holder }
            Assert-Migrated
            $before = Read-Configuration
            $holder = Start-Holder 'Lock' (Join-Path $parent 'change.lock')
            try {
                $install = Invoke-Service 'install'
                $remove = Invoke-Service 'remove'
                Assert ($install.Code -eq 4 -and $install.Seconds -ge 59 -and $install.Seconds -le 65) 'install contention did not respect 60-second limit'
                Assert ($remove.Code -eq 4 -and $remove.Seconds -ge 19 -and $remove.Seconds -le 24) 'remove contention did not respect shared 20-second limit'
                Assert ((Read-Configuration) -eq $before) 'contending action mutated the service'
            } finally { Stop-Holder $holder }
            Assert ((Invoke-Service 'install').Code -eq 0) 'install did not acquire released lock'
            return 'unelevated global mutex ignored; exclusive file contention timed out at 60/20 seconds with exit 4; released lock acquired'
        }
        '6' {
            Reset-Legacy
            Assert ((Invoke-Service 'install').Code -eq 0) 'migration failed'
            $holder = Start-Holder 'Refused' (Join-Path $indexDir 'navo-index.exe')
            try { $refused = Invoke-Service 'remove' }
            finally { Stop-Holder $holder }
            $pending = (Read-PendingDeletes)
            Write-Evidence "refused_delete_and_rename_exit=$($refused.Code); image_exists=$(Test-Path (Join-Path $indexDir 'navo-index.exe')); pending=$($pending -join ';')"
            $imageScheduled = @($pending | Where-Object { $_ -match '\\Navo\\Index\\navo-index\.exe$' }).Count -gt 0
            $script:refusedMatches = $refused.Code -eq 1 -and -not $imageScheduled
            Assert (-not $imageScheduled) 'refused image path was scheduled'
            Assert ((Invoke-Service 'remove').Code -in @(0, 6)) 'retry after refused holder failed'
            Assert ((Invoke-Service 'install').Code -eq 0) 'fresh install failed'
            $holder = Start-Holder 'Mapped' (Join-Path $indexDir 'navo-index.exe')
            try { $result = Invoke-Service 'remove' }
            finally { Stop-Holder $holder }
            $left = @(Get-ChildItem $parent -Recurse | ForEach-Object FullName)
            Write-Evidence "deferred_exit=$($result.Code); service_left=$([bool](Get-IndexService)); leftovers=$($left -join ';')"
            Assert ($result.Code -eq 6) 'mapped-image removal did not report deferred exit 6'
            Assert (-not (Get-IndexService)) 'deferred remove left SCM service'
            $pending = (Read-PendingDeletes)
            Write-Evidence "pending=$($pending -join ';')"
            $fileEntry = @($pending | Where-Object { $_ -match '\\Navo\\Index\\navo-index\.exe\.\d+\.old$' })
            Assert ($fileEntry.Count -eq 1) 'renamed image was not scheduled exactly once'
            $folderEntry = @($pending | Where-Object { $_.EndsWith($indexDir, [StringComparison]::OrdinalIgnoreCase) })
            Assert ($folderEntry.Count -eq 1) 'Index folder was not scheduled exactly once'
            Assert ($pending.IndexOf($fileEntry[0]) -lt $pending.IndexOf($folderEntry[0])) 'folder scheduled before file'
            @{ Boot = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o'); RefusedMatches = $script:refusedMatches; RefusedCode = $refused.Code } | ConvertTo-Json | Set-Content (Join-Path $WorkDir 'reboot.json')
            throw 'REBOOT: exit 6 and renamed-file-before-folder scheduling observed. Stop/start the VM once, wait for SSH, then rerun this script with -Phase AfterReboot and the same Out/WorkDir; verify no Program Files\Navo folder.'
        }
    }
}

# Appends a verdict and its captured evidence, preserving explicit owner and reboot limits.
function Write-Verdict([string]$number, [string]$verdict, [string]$reason) {
    $line = "$verdict check$number`: $reason"
    Write-Host $line
    Add-Content $Out "`n$line`n"
    foreach ($entry in $evidence) { Add-Content $Out "    EVIDENCE $($entry -replace '\r?\n', ' | ')" }
    if ($verdict -eq 'FAIL') { $script:failed = $true }
}

Assert ($env:OS -eq 'Windows_NT') 'run inside Windows'
Assert ($Out -and $WorkDir) 'Out and WorkDir are required'
New-Item -ItemType Directory $WorkDir -Force | Out-Null
if ($Phase -eq 'AfterReboot') {
    $state = Get-Content (Join-Path $WorkDir 'reboot.json') -Raw | ConvertFrom-Json
    $boot = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
    Write-Evidence "boot_before=$($state.Boot); boot_after=$boot; service_left=$([bool](Get-IndexService)); Index_left=$(Test-Path $indexDir); Navo_left=$(Test-Path $parent); refused_exit=$($state.RefusedCode)"
    if (Test-Path $parent) { Write-Evidence "leftovers=$((Get-ChildItem $parent -Recurse | ForEach-Object FullName) -join ';')" }
    $verdict = 'PASS'; $reason = 'reboot removed renamed image and both folders; refused image returned exit 1 without scheduling'
    if ($boot -eq $state.Boot -or (Get-IndexService) -or (Test-Path $parent) -or -not $state.RefusedMatches) {
        $verdict = 'FAIL'; $reason = 'reboot cleanup or refused-image exit differs from the VM plan; see evidence; Settings deferred notice still OWNER-CHECK'
    }
    $previous = [IO.File]::ReadAllText($Out)
    Copy-Item $Out "$Out.before-reboot.bak"
    $line = "$verdict check6`: $reason"
    Assert ($previous -match '(?m)^REBOOT-CHECK check6:') 'report has no pending reboot check'
    $updated = [regex]::Replace($previous, '(?m)^REBOOT-CHECK check6:[^\r\n]*', $line)
    [IO.File]::WriteAllText($Out, $updated, (New-Object Text.UTF8Encoding($false)))
    Write-Host $line
    foreach ($entry in $evidence) { Add-Content $Out "    EVIDENCE $entry" }
    if ($verdict -eq 'FAIL') { $failed = $true }
    if ($failed) { exit 1 }; exit 0
}
Assert ([Diagnostics.Process]::GetCurrentProcess().SessionId -ne 0) 'run as an elevated interactive scheduled task; the service install needs an interactive user'
Assert (-not (Test-Path $installRoot) -and -not (Get-IndexService) -and -not (Test-Path $parent)) 'start with a clean VM'
Assert (-not (Get-CimInstance Win32_Service -Filter "Name='Ferrite Index'")) 'unrelated Ferrite service exists'
foreach ($path in @($LegacyExe, $LegacySetup, $Setup, $Revision2Exe)) { Assert (Test-Path $path -PathType Leaf) "fixture missing: $path" }
if (Test-Path $Out) { Copy-Item $Out "$Out.$([guid]::NewGuid().ToString('N')).bak" }
"<!-- Records IX8v Windows service VM observations and human follow-up. -->`n# IX8v index-service VM report`n`nHEAD: $((Run-Process git @('rev-parse', 'HEAD')).Text.Trim()); session=$([Diagnostics.Process]::GetCurrentProcess().SessionId); elevated driver; UAC cannot be counted.`n" | Set-Content $Out
foreach ($path in @($LegacyExe, $LegacySetup, $Setup, $Revision2Exe)) { Add-Content $Out "Fixture $([IO.Path]::GetFileName($path)): SHA256=$((Get-FileHash $path).Hash)`n" }
Invoke-CommandChecked $LegacySetup @('--silent')
Wait-Until { Test-Path $app } 60 'v0.2.2 Setup did not install'
Stop-Apps
Invoke-CommandChecked $app @('--index-service', 'install')
Write-Evidence "real v0.2.2 Setup service=$((Get-IndexService).PathName)"
Invoke-CommandChecked $Setup @('--silent')
Wait-Until { Test-Path (Join-Path $installRoot 'current\navo-index.exe') } 60 'IX8v Setup did not install sibling'
Stop-Apps
$sibling = Join-Path $installRoot 'current\navo-index.exe'
$savedSibling = Join-Path $WorkDir 'current-service.exe'
Copy-Item $sibling $savedSibling -Force
Add-Content $Out "Current service SHA256=$((Get-FileHash $savedSibling).Hash); v0.2.2 Setup followed by IX8v Setup.`n"
foreach ($number in $Checks) {
    if (-not (Test-Path $app)) {
        Invoke-CommandChecked $Setup @('--silent')
        Wait-Until { Test-Path $app } 60 'fixture reinstall failed'
        Stop-Apps
    }
    $caseDir = Join-Path $WorkDir ('check' + $number + '-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $caseDir | Out-Null
    $profiles = @(); $evidence = @()
    $verdict = 'PASS'; $reason = ''
    try { $reason = Test-Check $number }
    catch {
        $verdict = 'FAIL'; $reason = $_.Exception.Message
        if ($reason.StartsWith('OWNER: ')) { $verdict = 'OWNER-CHECK'; $reason = $reason.Substring(7) }
        if ($reason.StartsWith('REBOOT: ')) { $verdict = 'REBOOT-CHECK'; $reason = $reason.Substring(8) }
    } finally { Stop-Apps; if (Test-Path $sibling) { Copy-Item $savedSibling $sibling -Force } }
    foreach ($event in @(Read-Events | Where-Object { $_.msg -match 'index service' } | Select-Object -Last 12)) { Write-Evidence ($event | ConvertTo-Json -Compress -Depth 8) }
    Write-Verdict $number $verdict $reason
}
if (Get-IndexService) { $cleanup = Invoke-Service 'remove'; Assert ($cleanup.Code -in @(0,6)) 'final service removal failed' }
if (Test-Path $installRoot) {
    Invoke-CommandChecked (Join-Path $installRoot 'Update.exe') @('uninstall', '--silent')
    Wait-Until { -not (Test-Path $installRoot) } 30 'final uninstall left app folder'
}
Add-Content $Out "`nFinal automated cleanup: service=$([bool](Get-IndexService)); app_install=$(Test-Path $installRoot); Navo_folder=$(Test-Path $parent). Leftovers stay intact for reboot observation.`n"
if ($failed) { exit 1 }; exit 0
