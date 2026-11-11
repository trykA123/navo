# Runs inside the Windows VM for the installer's end-to-end cases (plan W15a).
# -MakeBase builds the base once, -WaitLogon waits for u's desktop, -Launch starts a case in u's session, -Case runs it.
#requires -Version 5.1
param(
    [string]$Case,
    [switch]$MakeBase,
    [switch]$WaitLogon,
    [switch]$Launch
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot 'setup-e2e\common.ps1')
foreach ($file in @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'setup-e2e') -Filter 'cases-*.ps1')) { . $file.FullName }

# Stores the auto-logon password as the LSA secret, which Winlogon prefers over the registry value.
function Set-AutoLogonSecret([string]$password) {
    Add-Type @'
// Writes the LSA secret that Winlogon reads for auto-logon.
using System;
using System.Runtime.InteropServices;
namespace NavoE2E {
    public static class Lsa {
        [StructLayout(LayoutKind.Sequential)] struct Text { public ushort Length, Maximum; public IntPtr Buffer; }
        [StructLayout(LayoutKind.Sequential)] struct Attributes { public int Length; public IntPtr Root, Name; public uint Flags; public IntPtr Descriptor, Quality; }
        [DllImport("advapi32")] static extern uint LsaOpenPolicy(IntPtr system, ref Attributes attributes, uint access, out IntPtr policy);
        [DllImport("advapi32")] static extern uint LsaStorePrivateData(IntPtr policy, ref Text key, ref Text data);
        [DllImport("advapi32")] static extern uint LsaClose(IntPtr policy);
        static Text Make(string value) {
            return new Text { Length = (ushort)(value.Length * 2), Maximum = (ushort)(value.Length * 2 + 2), Buffer = Marshal.StringToHGlobalUni(value) };
        }
        // Stores the value under DefaultPassword; returns the NTSTATUS, 0 on success.
        public static uint Store(string value) {
            var attributes = new Attributes(); IntPtr policy;
            uint status = LsaOpenPolicy(IntPtr.Zero, ref attributes, 0x00000030, out policy);
            if (status != 0) return status;
            var key = Make("DefaultPassword"); var data = Make(value);
            status = LsaStorePrivateData(policy, ref key, ref data);
            LsaClose(policy);
            return status;
        }
    }
}
'@
    $status = [NavoE2E.Lsa]::Store($password)
    Assert ($status -eq 0) "LsaStorePrivateData failed with NTSTATUS $status"
}

# Creates the standard user u and the admin a, and makes u the auto-logon account.
function New-Accounts {
    $bytes = New-Object byte[] 18
    [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
    $password = 'Aa1!' + [Convert]::ToBase64String($bytes)
    $secure = ConvertTo-SecureString $password -AsPlainText -Force
    foreach ($name in @('u', 'a')) {
        New-LocalUser -Name $name -Password $secure -PasswordNeverExpires -AccountNeverExpires | Out-Null
    }
    Add-LocalGroupMember -Group Users -Member u
    Add-LocalGroupMember -Group Administrators -Member a
    $winlogon = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
    Set-ItemProperty -LiteralPath $winlogon -Name AutoAdminLogon -Value '1'
    Set-ItemProperty -LiteralPath $winlogon -Name DefaultUserName -Value 'u'
    Set-ItemProperty -LiteralPath $winlogon -Name DefaultDomainName -Value $env:COMPUTERNAME
    Remove-ItemProperty -LiteralPath $winlogon -Name DefaultPassword -ErrorAction SilentlyContinue
    Set-AutoLogonSecret $password
}

# Installs a private .NET SDK and vpk 1.2.161 under C:\e2e\tools; sets $script:vpk.
function Install-Vpk {
    $tools = Join-Path $e2e 'tools'
    $script:vpk = Join-Path $tools 'vpk\vpk.exe'
    $dotnetDir = Join-Path $tools 'dotnet'
    $env:DOTNET_ROOT = $dotnetDir
    $env:DOTNET_ROLL_FORWARD = 'Major'
    $env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
    $env:Path = "$dotnetDir;$env:Path"
    if (Test-Path -LiteralPath $script:vpk) { return }
    New-Item -ItemType Directory -Path $tools -Force | Out-Null
    $installer = Join-Path $tools 'dotnet-install.ps1'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing -Uri 'https://dot.net/v1/dotnet-install.ps1' -OutFile $installer
    & $installer -Channel 9.0 -InstallDir $dotnetDir -NoPath
    Run-Checked (Join-Path $dotnetDir 'dotnet.exe') @('tool', 'install', 'vpk', '--version', '1.2.161', '--tool-path', (Join-Path $tools 'vpk')) 600 | Out-Null
    Assert (Test-Path -LiteralPath $script:vpk) 'vpk.exe was not installed'
}

# Fetches pdfium as release.yml pins it into $WorkDir\runtime. Source: scripts/update-e2e.ps1:628-636.
function Get-Runtime([string]$source) {
    $workflow = Get-Content -LiteralPath (Join-Path $source '.github\workflows\release.yml') -Raw
    $runtimeEnv = @{ RUNNER_TEMP = (Join-Path $WorkDir ('fetch-' + [guid]::NewGuid().ToString('N'))) }
    foreach ($key in @('PDFIUM_TAG', 'PDFIUM_SHA256')) {
        $match = [regex]::Match($workflow, "(?m)^  ${key}: (.+)$")
        Assert $match.Success "missing $key in release.yml"
        $runtimeEnv[$key] = $match.Groups[1].Value.Trim()
    }
    Run-Checked $script:powershell @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $source '.github\scripts\fetch-pdfium.ps1'),
        '-Destination', (Join-Path $WorkDir 'runtime'), '-Licenses', (Join-Path $WorkDir 'runtime\licenses\pdfium')) 300 $runtimeEnv | Out-Null
}

# Builds one wizard into C:\e2e\wizard\<name>, from the given extra environment and cargo arguments.
function Build-Wizard([string]$name, [string]$source, [hashtable]$extra, [string[]]$arguments) {
    $target = Join-Path $WorkDir 'target'
    $environment = @{ CARGO_TARGET_DIR = $target; CARGO_PROFILE_RELEASE_LTO = 'thin' } + $extra
    $script:repo = $source
    Run-Checked cargo (@('build', '--locked', '--release', '-p', 'navo-setup') + $arguments) 1800 $environment | Out-Null
    $folder = Join-Path $wizards $name
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $target 'release\navo-setup.exe') -Destination (Join-Path $folder 'Navo-Setup.exe')
}

# Sets navo-setup's own version in the build copy: an embedded payload must match CARGO_PKG_VERSION.
function Set-SetupVersion([string]$source, [string]$version) {
    $utf8 = New-Object Text.UTF8Encoding($false)
    $manifest = Join-Path $source 'crates\navo-setup\Cargo.toml'
    $text = [regex]::Replace([IO.File]::ReadAllText($manifest), '(?m)^version(\.workspace = true| = "[^"]+")', "version = `"$version`"")
    [IO.File]::WriteAllText($manifest, $text, $utf8)
    $lock = Join-Path $source 'Cargo.lock'
    $pattern = '(name = "navo-setup"\r?\nversion = ")[^"]+'
    $text = [IO.File]::ReadAllText($lock)
    Assert ([regex]::Matches($text, $pattern).Count -eq 1) 'expected one navo-setup entry in Cargo.lock'
    [IO.File]::WriteAllText($lock, [regex]::Replace($text, $pattern, "`${1}$version"), $utf8)
}

# Builds a wizard with each pack embedded and one tester wizard; throws when a version line is wrong.
function Build-Wizards([string]$source) {
    foreach ($version in @('0.0.1', '0.0.2', '0.0.3')) {
        Set-SetupVersion $source $version
        $payload = @{ NAVO_SETUP_PAYLOAD = (Get-PackFile $version '*Setup.exe'); NAVO_SETUP_PACKAGE = (Get-PackFile $version '*-full.nupkg'); NAVO_SETUP_REQUIRE_PAYLOAD = '1' }
        Build-Wizard $version $source $payload @()
        $line = Run-Checked (Join-Path $wizards "$version\Navo-Setup.exe") @('--version')
        Assert ($line -eq "navo-setup $version") "wizard $version says: $line"
    }
    Build-Wizard 'tester' $source @{} @('--features', 'tester')
}

# Builds the S10 fake payloads; each sleeps 5 s and exits 1, and fail-partial first writes partial.txt.
function New-Fakes {
    $folder = Join-Path $e2e 'fakes'
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    $body = 'public static class P { public static int Main(string[] a) { %WRITE% System.Threading.Thread.Sleep(5000); return 1; } }'
    $write = 'for (int i = 0; i + 1 < a.Length; i++) if (a[i] == "--installto") { System.IO.Directory.CreateDirectory(a[i + 1]); System.IO.File.WriteAllText(System.IO.Path.Combine(a[i + 1], "partial.txt"), "x"); }'
    Add-Type -TypeDefinition $body.Replace('%WRITE%', '') -OutputType ConsoleApplication -OutputAssembly (Join-Path $folder 'fail-empty.exe')
    Add-Type -TypeDefinition $body.Replace('%WRITE%', $write) -OutputType ConsoleApplication -OutputAssembly (Join-Path $folder 'fail-partial.exe')
}

# Makes the base's content: accounts, packs 0.0.1 to 0.0.3, wizards and fakes. Nothing is installed.
# Each finished step is skipped on a rerun, so a failed build resumes without a new clone.
function Invoke-MakeBase {
    New-Item -ItemType Directory -Path $WorkDir -Force | Out-Null
    $source = Join-Path $e2e 'src'
    if (-not (Test-Path -LiteralPath $source)) { Expand-Archive -LiteralPath (Join-Path $e2e 'source.zip') -DestinationPath $source }
    if (-not (Get-LocalUser -Name u -ErrorAction SilentlyContinue)) { New-Accounts }
    Install-Vpk
    if (-not (Test-Path -LiteralPath (Join-Path $WorkDir 'runtime'))) { Get-Runtime $source }
    foreach ($pack in @(@('previous', '0.0.1'), @('base', '0.0.2'), @('next', '0.0.3'))) {
        if (-not (Test-Path -LiteralPath (Join-Path $packages "$($pack[0])\out\releases.win.json"))) { Build-Pack $pack[0] $pack[1] $source }
    }
    Build-Wizards $source
    New-Fakes
    $out = Join-Path $e2e 'out'
    New-Item -ItemType Directory -Path $out -Force | Out-Null
    Run-Checked icacls.exe @($out, '/grant', 'u:(OI)(CI)M', 'a:(OI)(CI)M') | Out-Null
    Remove-Item -LiteralPath (Join-Path $WorkDir 'target'), $source, (Join-Path $e2e 'source.zip') -Recurse -Force
}

# Waits until u has an active console session with a running explorer.exe, at most 300 s.
function Invoke-WaitLogon {
    Wait-Until {
        $shell = @(Get-CimInstance Win32_Process -Filter "Name='explorer.exe'" | Where-Object {
            (Invoke-CimMethod -InputObject $_ -MethodName GetOwner).User -eq 'u'
        })
        $shell.Count -gt 0
    } 300 'u did not log on'
    Start-Sleep -Seconds 10
}

# Registers and starts the case as u in the interactive session; the task outlives this SSH call.
function Invoke-Launch([string]$name) {
    $folder = Join-Path $e2e "out\$name"
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    # vpk moves its output from admin's TEMP, so those files keep a private ACL that u cannot read.
    Run-Checked icacls.exe @($packages, '/reset', '/T', '/Q') | Out-Null
    $arguments = Quote-Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $e2e 'scripts\setup-e2e.ps1'), '-Case', $name)
    $action = New-ScheduledTaskAction -Execute $script:powershell -Argument $arguments -WorkingDirectory $e2e
    $principal = New-ScheduledTaskPrincipal -UserId 'u' -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 25) -AllowStartIfOnBatteries
    Register-ScheduledTask -TaskName "NavoSetupE2E-$name" -Action $action -Principal $principal -Settings $settings | Out-Null
    Start-ScheduledTask -TaskName "NavoSetupE2E-$name"
}

# Runs Test-<case>, writes PASS or FAIL into result.txt, then the done file the host waits for.
function Invoke-Case([string]$name) {
    $script:caseDir = Join-Path $e2e "out\$name"
    New-Item -ItemType Directory -Path $script:caseDir -Force | Out-Null
    $script:WorkDir = $script:caseDir
    $line = "PASS $name"
    try {
        Assert (Get-Command "Test-$name" -ErrorAction SilentlyContinue) "no case function Test-$name"
        & "Test-$name"
    } catch {
        $line = "FAIL ${name}: $($_.Exception.Message)"
        Write-Evidence ($_ | Out-String)
    }
    try {
        Restore-Registry
        Save-WizardLogs
    } catch {
        if ($line.StartsWith('PASS')) { $line = "FAIL ${name}: cleanup: $($_.Exception.Message)" }
    }
    [IO.File]::WriteAllText((Join-Path $script:caseDir 'result.txt'), "$line`r`n", (New-Object Text.UTF8Encoding($false)))
    [IO.File]::WriteAllText((Join-Path $script:caseDir 'done'), '', (New-Object Text.UTF8Encoding($false)))
    Write-Host $line
}

if ($MakeBase) { Invoke-MakeBase }
elseif ($WaitLogon) { Invoke-WaitLogon }
elseif ($Launch) { Invoke-Launch $Case }
elseif ($Case) { Invoke-Case $Case }
else { throw 'give -MakeBase, -WaitLogon, -Launch -Case <case> or -Case <case>' }
exit 0
