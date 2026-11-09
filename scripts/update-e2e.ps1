# Exercises installed and portable updater paths in the isolated Windows VM.
# Builds tester packages and records case verdicts with application logs.
#requires -Version 5.1
#requires -RunAsAdministrator
param(
    [Parameter(Mandatory)][string]$Out,
    [switch]$SkipBuild,
    [switch]$ElevatedSession,
    [ValidateRange(1, 9)][int[]]$Cases = (1..9),
    [string]$WorkDir = (Join-Path $env:TEMP 'navo-update-e2e')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Get-Location).Path
$WorkDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($WorkDir)
$Out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out)
$installRoot = Join-Path $env:LOCALAPPDATA 'NavoApp'
$app = Join-Path $installRoot 'current\navo.exe'
$indexDir = Join-Path $env:ProgramFiles 'Navo\Index'
$packages = Join-Path $WorkDir 'packages'
$ownedInstall = $false
$ownedService = $false
$failed = $false

# Records checked observations in both the console log and the case report.
function Write-Evidence([string]$message) {
    $script:evidence += $message
    Write-Host "EVIDENCE $message"
}

function Assert($condition, [string]$reason) {
    if (-not $condition) { throw $reason }
}

function Quote-Arguments([string[]]$values) {
    ($values | ForEach-Object {
        '"' + [regex]::Replace([regex]::Replace($_, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1') + '"'
    }) -join ' '
}

# GUI-subsystem executables need redirected streams through a Process object.
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

function Run-Checked([string]$exe, [string[]]$arguments, [int]$seconds = 120, [hashtable]$environment = @{}) {
    $result = Run-Process $exe $arguments $seconds $environment
    $result.Text + $result.Error | Add-Content -LiteralPath (Join-Path $WorkDir 'commands.log')
    Assert ($result.Code -eq 0) "$exe exited $($result.Code): $($result.Error)"
    $result.Text.Trim()
}

function Get-IndexService {
    Get-CimInstance Win32_Service -Filter "Name='Navo Index'"
}

function Assert-ServiceOwned {
    $service = Get-IndexService
    if ($service) {
        Assert $script:ownedService 'refusing to touch a Navo Index service this run did not install'
        $expected = '^"?' + [regex]::Escape($indexDir + '\') + 'navo(-index)?\.exe"? '
        Assert ($service.PathName -match $expected) 'Navo Index command changed outside this run'
    }
}

function Observe-Consent {
    foreach ($process in @(Get-Process consent -ErrorAction SilentlyContinue)) {
        if ($process.StartTime -ge $script:caseStart) { [void]$script:consents.Add($process.Id) }
    }
}

function Wait-Until([scriptblock]$condition, [int]$seconds, [string]$reason) {
    $deadline = [DateTime]::UtcNow.AddSeconds($seconds)
    do {
        Observe-Consent
        if (& $condition) { return }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $reason
}

function Watch-For([int]$seconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        Observe-Consent
        Start-Sleep -Milliseconds 200
    }
}

function Read-Events {
    foreach ($profile in $script:profiles) {
        $logs = Join-Path $profile 'data\logs'
        if (-not (Test-Path -LiteralPath $logs)) { continue }
        foreach ($file in @(Get-ChildItem -LiteralPath $logs -Filter 'navo*.log' | Sort-Object LastWriteTime)) {
            foreach ($line in @(Get-Content -LiteralPath $file.FullName)) {
                # A live writer may leave its last JSON line unfinished.
                if (-not $line.EndsWith('}}')) { continue }
                $line | ConvertFrom-Json
            }
        }
    }
}

function Events([string]$message) {
    @(Read-Events | Where-Object { $_.msg -eq $message })
}

function Get-Status([string]$exe = $app) {
    $environment = @{ LOCALAPPDATA = $script:caseDir; APPDATA = $script:caseDir }
    $json = Run-Checked $exe @('--update-status') 20 $environment
    $json | ConvertFrom-Json
}

# Checks installed package metadata against the expected version and staged state.
function Assert-Version([string]$version, [bool]$staged = $false) {
    $status = Get-Status
    Assert ($status.kind -eq 'installed') "expected installed copy, got $($status.kind)"
    Assert ($status.current_version -eq $version) "expected $version, got $($status.current_version)"
    if (-not $staged) { Assert ($null -eq $status.staged_version) 'a package is still staged' }
    Write-Evidence ($status | ConvertTo-Json -Compress)
}

function Assert-NoPartial {
    $partials = @(Get-ChildItem -LiteralPath (Join-Path $installRoot 'packages') -Filter '*.partial')
    Assert ($partials.Count -eq 0) 'a partial download remains'
}

function New-Feed([string]$package) {
    $feed = Join-Path $script:caseDir ('feed-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $feed | Out-Null
    $script:feeds += $feed
    Copy-Item -Path (Join-Path $packages "$package\out\*.nupkg") -Destination $feed
    Copy-Item -LiteralPath (Join-Path $packages "$package\out\releases.win.json") -Destination $feed
    $feed
}

# Enlarges a case-owned package so a real copy can be interrupted; rewrites its feed checksums.
function New-InterruptedFeed {
    $feed = New-Feed 'next'
    $manifest = Join-Path $feed 'releases.win.json'
    $metadata = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
    $asset = @($metadata.Assets | Where-Object { $_.Type -eq 'Full' })[0]
    $package = Join-Path $feed $asset.FileName
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::Open($package, [IO.Compression.ZipArchiveMode]::Update)
    $random = [Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $entry = $archive.CreateEntry('ix8r-download.bin', [IO.Compression.CompressionLevel]::NoCompression)
        $stream = $entry.Open()
        try {
            $buffer = New-Object byte[] 1MB
            for ($chunk = 0; $chunk -lt 256; $chunk++) {
                $random.GetBytes($buffer)
                $stream.Write($buffer, 0, $buffer.Length)
            }
        } finally { $stream.Dispose() }
    } finally { $random.Dispose(); $archive.Dispose() }
    $asset.Size = (Get-Item -LiteralPath $package).Length
    foreach ($algorithm in @('SHA1', 'SHA256')) {
        $asset | Add-Member -NotePropertyName $algorithm -NotePropertyValue (Get-FileHash -LiteralPath $package -Algorithm $algorithm).Hash -Force
    }
    [IO.File]::WriteAllText($manifest, ($metadata | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
    $feed
}

# Reads a process's actual I/O bytes; Windows can preallocate a partial file at its full length.
function Get-WrittenBytes([Diagnostics.Process]$process) {
    if (-not ('NavoE2E.ProcessIo' -as [type])) {
        Add-Type @'
// Reads Windows process I/O counters for the interrupted-copy case.
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace NavoE2E {
    public static class ProcessIo {
        [StructLayout(LayoutKind.Sequential)]
        private struct Counters {
            public ulong Reads, Writes, Others, ReadBytes, WriteBytes, OtherBytes;
        }
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetProcessIoCounters(IntPtr process, out Counters counters);
        // Returns bytes written by the process; throws when its counters cannot be queried.
        public static ulong Written(IntPtr process) {
            Counters counters;
            if (!GetProcessIoCounters(process, out counters)) throw new Win32Exception();
            return counters.WriteBytes;
        }
    }
}
'@
    }
    [NavoE2E.ProcessIo]::Written($process.Handle)
}

function New-Profile([bool]$updates = $true) {
    $profile = Join-Path $script:caseDir ('profile-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path (Join-Path $profile 'config') | Out-Null
    $enabled = $updates.ToString().ToLowerInvariant()
    "[updates]`ncheck = $enabled`n[tray]`nkeep-running = false" |
        Set-Content -LiteralPath (Join-Path $profile 'config\navo.toml') -Encoding Ascii
    $script:profiles += $profile
    $profile
}

# Starts an interactive task for a profile and feed; elevated sessions leave UAC to the owner.
function Start-App([string]$profile, [string]$feed, [string]$exe = $app) {
    $launch = Join-Path $script:caseDir ([guid]::NewGuid().ToString('N'))
    $config = @{ exe = $exe; arguments = (Quote-Arguments @('--profile', $profile, '--left', $script:caseDir)); feed = $feed; elevated = [bool]$ElevatedSession }
    $config | ConvertTo-Json | Set-Content -LiteralPath "$launch.json" -Encoding UTF8
    @'
param($Config)
$ErrorActionPreference = 'Stop'
try {
    $config = Get-Content -LiteralPath $Config -Raw | ConvertFrom-Json
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    $elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if ($elevated -ne $config.elevated) { throw 'unexpected task elevation' }
    $env:NAVO_LOG = 'info,navo::updater=debug,navo::service_version_step=debug'
    Remove-Item Env:NAVO_UPDATE_FEED_DIR -ErrorAction SilentlyContinue
    if ($config.feed) { $env:NAVO_UPDATE_FEED_DIR = $config.feed }
    $psi = New-Object Diagnostics.ProcessStartInfo($config.exe, $config.arguments)
    $psi.UseShellExecute = $false
    $process = [Diagnostics.Process]::Start($psi)
    $process.Id | Set-Content -LiteralPath ($PSCommandPath + '.pid')
} catch { $_.ToString() | Set-Content -LiteralPath ($PSCommandPath + '.error') }
'@ | Set-Content -LiteralPath "$launch.ps1" -Encoding UTF8
    $task = 'NavoUpdateE2E-' + [guid]::NewGuid().ToString('N')
    $arguments = Quote-Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$launch.ps1", '-Config', "$launch.json")
    $action = New-ScheduledTaskAction -Execute $script:powershell -Argument $arguments
    $level = if ($ElevatedSession) { 'Highest' } else { 'Limited' }
    $principal = New-ScheduledTaskPrincipal -UserId $script:userSid -LogonType Interactive -RunLevel $level
    Register-ScheduledTask -TaskName $task -Action $action -Principal $principal | Out-Null
    try {
        Start-ScheduledTask -TaskName $task
        Wait-Until { (Test-Path -LiteralPath "$launch.ps1.pid") -or (Test-Path -LiteralPath "$launch.ps1.error") } 30 'interactive launch timed out'
        if (Test-Path -LiteralPath "$launch.ps1.error") { throw (Get-Content -LiteralPath "$launch.ps1.error" -Raw) }
        Get-Process -Id ([int](Get-Content -LiteralPath "$launch.ps1.pid")) -ErrorAction SilentlyContinue
    } finally { Unregister-ScheduledTask -TaskName $task -Confirm:$false }
}

function App-Processes {
    $roots = @($script:caseDir + '\')
    if ($script:ownedInstall) { $roots += $installRoot + '\' }
    @(Get-CimInstance Win32_Process -Filter "Name='navo.exe'" | Where-Object {
        $image = $_.ExecutablePath
        $image -and $_.CommandLine -notmatch '--index-service' -and
            @($roots | Where-Object { $image.StartsWith($_, [StringComparison]::OrdinalIgnoreCase) }).Count -gt 0
    })
}

function Stop-Apps([bool]$force = $false) {
    foreach ($record in @(App-Processes)) {
        $process = Get-Process -Id $record.ProcessId -ErrorAction SilentlyContinue
        if (-not $process) { continue }
        if (-not $force) {
            [void]$process.CloseMainWindow()
            if ($process.WaitForExit(10000)) { continue }
        }
        $process.Kill()
        Assert ($process.WaitForExit(10000)) 'Navo did not stop'
        if (-not $force) { throw 'Navo required termination instead of quitting' }
    }
}

function Install-Base {
    Assert (-not (Test-Path -LiteralPath $installRoot)) "existing install at $installRoot; use a clean VM"
    Assert (-not (Get-IndexService)) 'an unrelated Navo Index service appeared'
    $setup = @(Get-ChildItem -LiteralPath (Join-Path $packages 'base\out') -Filter '*Setup.exe')
    Assert ($setup.Count -eq 1) 'expected exactly one base Setup.exe'
    $script:ownedInstall = $true
    Run-Checked $setup[0].FullName @('--silent') | Out-Null
    Wait-Until { Test-Path -LiteralPath $app } 60 'Setup did not install current\navo.exe'
    Stop-Apps $true
    Assert-Version '0.0.2'
}

function Stage-Next([string]$feed) {
    $profile = New-Profile
    Start-App $profile $feed | Out-Null
    Wait-Until { (Get-Status).staged_version -eq '0.0.3' } 120 'N+1 was not staged'
    Assert-Version '0.0.2' $true
    Stop-Apps
    $profile
}

function Apply-Next([string]$profile, [string]$feed) {
    $launchProcess = Start-App $profile $feed
    if ($launchProcess) {
        Assert ($launchProcess.WaitForExit(30000)) 'staged update did not hand off at start'
    }
    Wait-Until {
        if (-not (Test-Path -LiteralPath $app)) { return $false }
        try { (Get-Status).current_version -eq '0.0.3' }
        catch { $false }
    } 120 'N+1 was not applied or its status remained unreadable'
    Wait-Until { @(App-Processes).Count -gt 0 } 30 'the updated app did not restart'
    Assert-Version '0.0.3'
    Assert-NoPartial
}

function Install-Service {
    Assert (-not (Get-IndexService)) 'refusing to replace an existing service'
    Assert (-not (Get-CimInstance Win32_Service -Filter "Name='Ferrite Index'")) 'refusing to migrate an unrelated Ferrite service'
    Assert (-not (Test-Path -LiteralPath $indexDir)) 'refusing to replace an existing Index folder'
    $script:ownedService = $true
    Run-Checked $app @('--index-service', 'install') 90 | Out-Null
    Wait-Until { $service = Get-IndexService; $service -and $service.State -eq 'Running' } 30 'service did not start'
    Assert-ServiceOwned
}

# Verifies the pipe revision, SCM process and service image; fails on any mismatch.
function Assert-ServiceAnswer([int]$revision) {
    $answer = @(Events 'index service step' | Where-Object {
        $_.fields.step -eq 'Nothing' -and $_.fields.version -match "revision: $revision\b"
    })
    Assert ($answer.Count -gt 0) "no pipe answer for service revision $revision"
    $service = Get-IndexService
    Assert ($service -and $service.State -eq 'Running') 'service is not running'
    $memory = (Run-Checked $app @('--index-memory', '--json') 20) | ConvertFrom-Json
    Assert ($memory.service_process_id -eq $service.ProcessId) 'pipe server and SCM process differ'
    $process = Get-Process -Id $service.ProcessId
    Assert ($process.Path -eq (Join-Path $indexDir 'navo-index.exe')) 'wrong running service image'
    Assert ($process.MainModule.FileVersionInfo.FileDescription -eq 'Navo Index') 'wrong service file description'
    Assert ($service.Description -eq 'Keeps the file index for Navo search up to date.') 'wrong service description'
    Write-Evidence ('revision={0}; pipe_pid={1}; scm_pid={2}; image={3}; description={4}' -f
        $revision, $memory.service_process_id, $service.ProcessId, $process.Path, $process.MainModule.FileVersionInfo.FileDescription)
}

function Wait-ServiceUpdate {
    do {
        Observe-Consent
        if (@(Events 'index service updated').Count -gt 0) { return }
        $errors = @(Events 'index service update declined or failed; not asking again for this revision')
        if ($errors.Count -gt 0) {
            if ($errors[-1].fields.reason -match 'elevation was cancelled') { throw 'MANUAL: UAC was declined' }
            throw "service update failed: $($errors[-1].fields.reason)"
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $script:promptDeadline)
    throw 'MANUAL: no accepted UAC update within 120 seconds'
}

# Exercises same and bumped revisions or legacy migration; elevated runs require an owner prompt check.
function Test-ServiceUpdate([bool]$legacy) {
    Install-Service
    if (-not $legacy) {
        $sameFeed = New-Feed 'next'
        $sameProfile = Stage-Next $sameFeed
        Apply-Next $sameProfile $sameFeed
        Watch-For 15
        Wait-Until { @(Events 'index service step' | Where-Object { $_.fields.step -eq 'Nothing' }).Count -gt 0 } 30 'no unchanged-revision check'
        Assert-ServiceAnswer $script:revision
        Assert (@(Events 'index service is older than Navo; asking to update it').Count -eq 0) 'unchanged revision requested a prompt'
        Assert ($script:consents.Count -eq 0) 'unchanged revision showed a UAC prompt'
        Stop-Apps
        Uninstall-Copy
        Assert (-not (Test-Path -LiteralPath $indexDir)) 'unchanged-revision uninstall left Index'
        $script:ownedService = $false
        Install-Base
        Install-Service
    }
    if ($legacy) {
        Stop-Service -Name 'Navo Index'
        Wait-Until { (Get-IndexService).State -eq 'Stopped' } 15 'base service did not stop'
        Wait-Until { -not (Get-Process -Name navo-index -ErrorAction SilentlyContinue) } 10 'base service process still running'
        Copy-Item -LiteralPath $app -Destination (Join-Path $indexDir 'navo.exe')
        Remove-Item -LiteralPath (Join-Path $indexDir 'navo-index.exe')
        $command = '"' + (Join-Path $indexDir 'navo.exe') + '" --index-service run'
        Run-Checked sc.exe @('config', 'Navo Index', 'binPath=', $command) | Out-Null
        Start-Service -Name 'Navo Index'
        Wait-Until { (Get-IndexService).State -eq 'Running' } 20 'legacy service did not start'
        Assert ((Get-IndexService).PathName -match 'navo\.exe" --index-service run$') 'legacy registration was not established'
    }
    $feed = New-Feed 'bumped'
    $profile = Stage-Next $feed
    Write-Host 'ACCEPT THE UAC PROMPT NOW'
    $script:promptDeadline = [DateTime]::UtcNow.AddSeconds(120)
    Apply-Next $profile $feed
    Wait-ServiceUpdate
    Assert (@(Events 'index service is older than Navo; asking to update it').Count -eq 1) 'expected one service prompt request'
    Stop-Apps
    $second = New-Profile $false
    Start-App $second $feed | Out-Null
    Wait-Until { @(Events 'index service step' | Where-Object {
        $_.fields.step -eq 'Nothing' -and $_.fields.version -match "revision: $($script:revision + 1)\b"
    }).Count -gt 0 } 30 'no same-revision check'
    Watch-For 10
    Assert-ServiceAnswer ($script:revision + 1)
    Assert (@(Events 'index service is older than Navo; asking to update it').Count -eq 1) 'same revision requested another prompt'
    Assert (-not (Test-Path -LiteralPath (Join-Path $indexDir 'navo.exe'))) 'legacy navo.exe remains in Index'
    Stop-Apps
    Uninstall-Copy
    Assert (-not (Get-IndexService)) 'Navo Index remains after uninstall'
    if (Test-Path -LiteralPath $indexDir) { throw 'MANUAL: Index remains; inspect deferred cleanup and verify after reboot' }
    Assert (-not (Test-Path -LiteralPath (Join-Path $env:ProgramFiles 'Navo'))) 'uninstall left the Navo folder'
    Write-Evidence 'legacy_navo.exe=False; service_after_uninstall=False; Index_after_uninstall=False; Navo_after_uninstall=False'
    if ($ElevatedSession) { throw 'MANUAL: revision, image, repeat start and clean uninstall verified; exactly one UAC prompt needs an interactive owner check' }
    if ($script:consents.Count -eq 0) { throw 'MANUAL: service updated, but no UAC prompt was observed; check VM UAC policy' }
    Assert ($script:consents.Count -eq 1) 'more than one UAC prompt was observed'
    'one accepted prompt, verified service revision and image; second start requested none'
}

function Uninstall-Copy {
    if (-not $script:ownedInstall) { return }
    Assert-ServiceOwned
    $updater = Join-Path $installRoot 'Update.exe'
    Assert (Test-Path -LiteralPath $updater) 'owned install has no Update.exe for uninstall'
    Run-Checked $updater @('uninstall', '--silent') 120 | Out-Null
    Wait-Until { -not (Test-Path -LiteralPath $app) } 30 'uninstall left current\navo.exe'
    Assert (-not (Get-IndexService)) 'uninstall left the owned service registered'
    Wait-Until { -not (Test-Path -LiteralPath $installRoot) } 30 'uninstall left its install folder'
    $script:ownedInstall = $false
}

function Cleanup-Case {
    $errors = @()
    try { Stop-Apps $true } catch { $errors += $_.Exception.Message }
    foreach ($rule in $script:rules) {
        try { Remove-NetFirewallRule -Name $rule } catch { $errors += $_.Exception.Message }
    }
    try { Uninstall-Copy } catch { $errors += $_.Exception.Message }
    if ($script:ownedService) {
        try {
            Assert-ServiceOwned
            if (Get-IndexService) {
                $exe = Join-Path $packages 'base\dist\navo.exe'
                $result = Run-Process $exe @('--index-service', 'remove') 30
                Assert ($result.Code -in @(0, 6)) 'owned service removal failed'
            }
        } catch { $errors += $_.Exception.Message }
    }
    foreach ($feed in $script:feeds) {
        try { Remove-Item -LiteralPath $feed -Recurse -Force } catch { $errors += $_.Exception.Message }
    }
    Assert ($errors.Count -eq 0) ($errors -join '; ')
}

# Runs one numbered updater scenario and fails when its observable behaviour differs.
function Test-Case([int]$number) {
    if ($number -eq 6) {
        $portable = @(Get-ChildItem -LiteralPath (Join-Path $packages 'base\out') -Filter '*Portable.zip')
        Assert ($portable.Count -eq 1) 'expected exactly one portable zip'
        $folder = Join-Path $script:caseDir 'portable'
        Expand-Archive -LiteralPath $portable[0].FullName -DestinationPath $folder
        $exe = Join-Path $folder 'current\navo.exe'
        Assert ((Get-Status $exe).kind -eq 'portable') 'zip is not portable'
        Start-App (New-Profile) (New-Feed 'next') $exe | Out-Null
        Wait-Until { @(Events 'update: no automatic checks for this copy').Count -gt 0 } 30 'missing portable no-check log'
        Watch-For 15
        Assert (@(Events 'update: check failed').Count -eq 0) 'portable copy attempted a check'
        $status = Get-Status $exe
        Assert ($status.state -eq 'unsupported') 'portable updater became active'
        Assert ($status.current_version -eq '0.0.2' -and $null -eq $status.staged_version) 'portable copy staged or applied a package'
        return 'portable metadata unsupported; explicit no-automatic-checks log; nothing staged'
    }
    Install-Base
    switch ($number) {
        1 {
            $feed = New-Feed 'next'
            $profile = Stage-Next $feed
            Apply-Next $profile $feed
            return '0.0.2 staged 0.0.3, quit/start applied it, restarted, and cleared staged metadata'
        }
        2 {
            $rule = 'NavoUpdateE2E-' + [guid]::NewGuid().ToString('N')
            New-NetFirewallRule -Name $rule -DisplayName $rule -Direction Outbound -Action Block -Program $app | Out-Null
            $script:rules += $rule
            Start-App (New-Profile) '' | Out-Null
            Wait-Until { @(Events 'update: check failed' | Where-Object { $_.fields.error -eq 'offline' }).Count -gt 0 } 90 'missing offline check failure'
            Assert-Version '0.0.2'
            Assert-NoPartial
            return 'real GitHub feed blocked by executable firewall rule; logged offline; no stage or partial'
        }
        3 {
            $feed = New-Feed 'next'
            '{broken json' | Set-Content -LiteralPath (Join-Path $feed 'releases.win.json') -Encoding UTF8
            Start-App (New-Profile) $feed | Out-Null
            Wait-Until { @(Events 'update: check failed' | Where-Object { $_.fields.error -eq 'bad feed' }).Count -gt 0 } 60 'missing bad-feed check failure'
            Assert-Version '0.0.2'
            Assert-NoPartial
            return 'broken releases.win.json logged bad feed; no stage or partial'
        }
        4 {
            $feed = New-InterruptedFeed
            $asset = @((Get-Content -LiteralPath (Join-Path $feed 'releases.win.json') -Raw | ConvertFrom-Json).Assets | Where-Object { $_.Type -eq 'Full' })[0]
            $partial = Join-Path $installRoot ('packages\' + [IO.Path]::ChangeExtension($asset.FileName, 'partial'))
            $profile = New-Profile
            $downloadProcess = Start-App $profile $feed
            $written = Get-WrittenBytes $downloadProcess
            $deadline = [DateTime]::UtcNow.AddSeconds(90)
            $percent = 0
            do {
                if (Test-Path -LiteralPath $partial) {
                    $percent = 100.0 * ((Get-WrittenBytes $downloadProcess) - $written) / $asset.Size
                    if ($percent -ge 40 -and $percent -le 65) {
                        $downloadProcess.Kill()
                        Assert ($downloadProcess.WaitForExit(10000)) 'download process did not stop'
                        Write-Evidence "interrupted at $([math]::Round($percent, 1))% written; partial_bytes=$((Get-Item -LiteralPath $partial).Length); package_bytes=$($asset.Size)"
                        break
                    }
                }
                if ((Test-Path -LiteralPath ([IO.Path]::ChangeExtension($partial, 'nupkg')))) { break }
                Start-Sleep -Milliseconds 1
            } while ([DateTime]::UtcNow -lt $deadline)
            if ($percent -lt 40 -or $percent -gt 65) { throw "folder copy missed the 40-65% interruption window (last $([math]::Round($percent, 1))% written); no interruption was verified" }
            Assert (-not (Test-Path -LiteralPath ([IO.Path]::ChangeExtension($partial, 'nupkg')))) 'interrupted copy was already staged'
            Assert-Version '0.0.2'
            $restart = New-Profile $false
            Start-App $restart $feed | Out-Null
            Watch-For 15
            Assert-Version '0.0.2'
            Stop-Apps
            Start-App $profile $feed | Out-Null
            Wait-Until { (Get-Status).staged_version -eq '0.0.3' } 120 'retry did not download N+1'
            Assert-NoPartial
            return "killed at $([math]::Round($percent))%; restart applied nothing; a fresh check staged 0.0.3"
        }
        5 {
            $feed = New-Feed 'previous'
            $profile = New-Profile
            Start-App $profile $feed | Out-Null
            Watch-For 25
            Assert (@(App-Processes).Count -gt 0) 'Navo exited before its startup check'
            Assert (@(Read-Events | Where-Object { $_.msg -match '^update: .*failed$' }).Count -eq 0) 'downgrade check failed'
            Assert-Version '0.0.2'
            Assert ((Get-Status).state -eq 'idle') 'downgrade feed did not leave idle metadata'
            Assert-NoPartial
            Stop-Apps
            Start-App (New-Profile $false) $feed | Out-Null
            Watch-For 5
            Assert-Version '0.0.2'
            return '0.0.1-only feed left 0.0.2 installed and idle, with nothing staged or applied on restart'
        }
        7 {
            $feed = New-Feed 'next'
            $second = Start-App (New-Profile $false) $feed
            Assert ($null -ne $second) 'second profile did not start'
            $profile = New-Profile
            $first = Start-App $profile $feed
            Wait-Until { (Get-Status).staged_version -eq '0.0.3' } 120 'N+1 was not staged'
            [void]$first.CloseMainWindow()
            Assert ($first.WaitForExit(10000)) 'first profile did not quit'
            Start-App $profile $feed | Out-Null
            Watch-For 15
            $second.Refresh()
            if ($second.HasExited -or $second.MainWindowHandle -eq 0) { throw 'D22d needs a change' }
            Assert-Version '0.0.2' $true
            $processes = @(App-Processes)
            $windows = @($processes | Where-Object { $_.CommandLine -notmatch '(?:^|\s)--pdf-helper(?:\s|$)' })
            Assert ($windows.Count -eq 2) "expected two profile processes, got $($windows.Count)"
            Write-Evidence "second_pid=$($second.Id); second_window=$($second.MainWindowHandle); profiles=$($windows.Count); pdf_helpers=$($processes.Count - $windows.Count); current=0.0.2; staged=0.0.3"
            Stop-Apps
            Apply-Next $profile $feed
            return 'second profile kept its window and blocked apply; after both quit, next start applied N+1'
        }
        8 { return Test-ServiceUpdate $false }
        9 { return Test-ServiceUpdate $true }
    }
}

# Builds and packs one tester version from a source tree; rejects failed commands or invalid feeds.
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

Assert ($env:OS -eq 'Windows_NT') 'run this script inside the Windows VM'
Assert ([Diagnostics.Process]::GetCurrentProcess().SessionId -ne 0) 'run the harness as an interactive scheduled task; SSH session 0 cannot inspect or close console windows'
Assert (Test-Path -LiteralPath (Join-Path $repo 'crates\navo-app\Cargo.toml')) 'run from the Navo repo root'
Assert ($WorkDir -ne $repo -and -not $WorkDir.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase)) 'WorkDir must be outside the checkout'
Assert (-not (Test-Path -LiteralPath $installRoot)) 'NavoApp is already installed; use a clean VM'
Assert (-not (Get-IndexService)) 'Navo Index already exists; this script will not touch it'
Assert (-not (Get-CimInstance Win32_Service -Filter "Name='Ferrite Index'")) 'Ferrite Index already exists; use a clean VM'
Assert (-not (Test-Path -LiteralPath $indexDir)) 'Index already exists; use a clean VM'
Assert (-not (Get-Process navo, navo-index -ErrorAction SilentlyContinue)) 'close existing Navo processes first'
New-Item -ItemType Directory -Path $WorkDir -Force | Out-Null
$powershell = Join-Path $PSHOME 'powershell.exe'
if (-not (Test-Path -LiteralPath $powershell)) { $powershell = (Get-Command powershell.exe).Source }
$userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$head = Run-Checked git @('rev-parse', 'HEAD')
$revisionFile = 'crates\navo-index\src\pipe\version.rs'
$revisionPattern = 'pub const SERVICE_REVISION: u32 = (\d+);'
$versionText = Get-Content -LiteralPath (Join-Path $repo $revisionFile) -Raw
Assert ([regex]::Matches($versionText, $revisionPattern).Count -eq 1) 'expected exactly one SERVICE_REVISION constant'
$revision = [int][regex]::Match($versionText, $revisionPattern).Groups[1].Value
$sourceHash = (Get-FileHash -LiteralPath (Join-Path $repo 'crates\navo-app\src\updater\service.rs')).Hash
$manifest = Join-Path $packages 'build.json'
if (-not $SkipBuild) {
    Assert (-not (Test-Path -LiteralPath $packages)) 'packages already exist; use -SkipBuild or a new -WorkDir'
    $vpk = (Get-Command vpk -ErrorAction Stop).Source
    Assert ((Run-Checked $vpk @('--help')) -match '\b1\.2\.161\b') 'vpk 1.2.161 is required, as in release.yml'
    $workflow = Get-Content -LiteralPath (Join-Path $repo '.github\workflows\release.yml') -Raw
    $runtimeEnv = @{ RUNNER_TEMP = (Join-Path $WorkDir ('fetch-' + [guid]::NewGuid().ToString('N'))) }
    foreach ($key in @('PDFIUM_TAG', 'PDFIUM_SHA256')) {
        $match = [regex]::Match($workflow, "(?m)^  ${key}: (.+)$")
        Assert $match.Success "missing $key in release.yml"
        $runtimeEnv[$key] = $match.Groups[1].Value.Trim()
    }
    Run-Checked $powershell @('-NoProfile', '-File', (Join-Path $repo '.github\scripts\fetch-pdfium.ps1'),
        '-Destination', (Join-Path $WorkDir 'runtime'), '-Licenses', (Join-Path $WorkDir 'runtime\licenses\pdfium')) 300 $runtimeEnv | Out-Null
    Build-Pack 'previous' '0.0.1' $repo
    Build-Pack 'base' '0.0.2' $repo
    Build-Pack 'next' '0.0.3' $repo
    $copy = Join-Path $WorkDir ('source-' + [guid]::NewGuid().ToString('N'))
    $archive = "$copy.zip"
    Run-Checked git @('archive', '--format=zip', '--output', $archive, 'HEAD') | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $copy
    $copiedFile = Join-Path $copy $revisionFile
    $copiedText = Get-Content -LiteralPath $copiedFile -Raw
    Assert ([regex]::Matches($copiedText, $revisionPattern).Count -eq 1) 'archive has no unique revision constant'
    Assert ([int][regex]::Match($copiedText, $revisionPattern).Groups[1].Value -eq $revision) 'HEAD and checkout service revisions differ'
    $bumped = [regex]::Replace($copiedText, $revisionPattern, "pub const SERVICE_REVISION: u32 = $($revision + 1);")
    [IO.File]::WriteAllText($copiedFile, $bumped, (New-Object Text.UTF8Encoding($false)))
    Build-Pack 'bumped' '0.0.3' $copy
    @{ head = $head; revision = $revision; source_hash = $sourceHash } | ConvertTo-Json | Set-Content -LiteralPath $manifest
}
$build = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
Assert ($build.head -eq $head -and $build.revision -eq $revision -and $build.source_hash -eq $sourceHash) 'cached packages do not match this checkout'
$run = Join-Path $WorkDir ('run-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run | Out-Null
if (Test-Path -LiteralPath $Out) { Copy-Item -LiteralPath $Out -Destination "$Out.$([guid]::NewGuid().ToString('N')).bak" }
New-Item -ItemType Directory -Path (Split-Path $Out -Parent) -Force | Out-Null
"# Navo update E2E`n`nHEAD: $head; revision: $revision; package versions: 0.0.1 / 0.0.2 / 0.0.3`nRun: $run`n" | Set-Content -LiteralPath $Out
$names = @('', 'update', 'offline', 'bad feed', 'interrupted download', 'downgrade refused', 'portable no request',
    'second process blocks apply', 'service revision bump', 'legacy service migration and remove')
foreach ($number in @($Cases | Select-Object -Unique)) {
    $caseDir = Join-Path $run "case$number"
    New-Item -ItemType Directory -Path $caseDir | Out-Null
    $profiles = @(); $feeds = @(); $rules = @(); $evidence = @()
    $consents = New-Object 'System.Collections.Generic.HashSet[int]'
    $caseStart = Get-Date
    $ownedService = $false
    $verdict = 'PASS'
    try { $reason = Test-Case $number }
    catch {
        $reason = $_.Exception.Message
        $verdict = 'FAIL'
        if ($reason.StartsWith('MANUAL: ')) { $verdict = 'OWNER-CHECK'; $reason = $reason.Substring(8) }
    } finally {
        try { Cleanup-Case }
        catch { $verdict = 'FAIL'; $reason += "; cleanup: $($_.Exception.Message)" }
    }
    $line = "$verdict case$number $($names[$number]): $reason"
    Write-Host $line
    Add-Content -LiteralPath $Out -Value $line
    foreach ($entry in $evidence) { Add-Content -LiteralPath $Out -Value "    EVIDENCE $entry" }
    $excerpt = @(Read-Events | Where-Object { $_.msg -match 'update|index service|app started' } | Select-Object -Last 10)
    foreach ($event in $excerpt) {
        $entry = $event | ConvertTo-Json -Compress -Depth 8
        Write-Host "  $entry"
        Add-Content -LiteralPath $Out -Value "    $entry"
    }
    Add-Content -LiteralPath $Out -Value ''
    if ($verdict -eq 'FAIL') { $failed = $true }
    if ($reason -match 'D22d needs a change') { throw 'D22d stop: second process closed; no further cases run' }
    if ($ownedInstall -or (Get-IndexService)) { throw 'cleanup incomplete; stopping before another case can touch this state' }
}
if ($failed) { exit 1 }
exit 0
