# Copy offload probe (R12a): sets up a loopback SMB share and a Dev Drive on the Windows VM, then
# runs the ignored copy_offload tests, which append one JSON line per case to C:\winvm\offload.json.
# Needs an elevated session (the VM's admin ssh session is); safe to run again.
$ErrorActionPreference = 'Stop'
$work = 'C:\navo-offload'
$shareDir = Join-Path $work 'share'
$vhdx = Join-Path $work 'devdrive.vhdx'
$out = 'C:\winvm\offload.json'

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Output 'offload-probe: needs an elevated session'
    exit 1
}
$version = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
Write-Output ("windows build: {0}.{1} ({2})" -f $version.CurrentBuildNumber, $version.UBR, $version.DisplayVersion)

New-Item -ItemType Directory -Force -Path $shareDir | Out-Null
if (-not (Get-SmbShare -Name 'navo-offload' -ErrorAction SilentlyContinue)) {
    New-SmbShare -Name 'navo-offload' -Path $shareDir -FullAccess 'admin' | Out-Null
}
Write-Output 'smb: \\localhost\navo-offload'

# Runs one diskpart script; throws with diskpart's output when it fails.
function Invoke-Diskpart([string[]]$lines) {
    $script = Join-Path $work 'diskpart.txt'
    Set-Content -Path $script -Value $lines -Encoding ascii
    $text = & diskpart.exe /s $script 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw "diskpart failed ($LASTEXITCODE): $text" }
}

# Returns $true when R: is a Dev Drive (ReFS) after attaching or creating the VHDX.
function Initialize-DevDrive {
    $volume = Get-Volume -DriveLetter R -ErrorAction SilentlyContinue
    if (-not $volume -and (Test-Path -LiteralPath $vhdx)) {
        # A reboot detaches the VHDX; its partition keeps the R: letter.
        Invoke-Diskpart @("select vdisk file=`"$vhdx`"", 'attach vdisk')
        Start-Sleep -Seconds 2
        $volume = Get-Volume -DriveLetter R -ErrorAction SilentlyContinue
    }
    if (-not $volume) {
        if (Test-Path -LiteralPath $vhdx) { throw "R: missing after attaching $vhdx" }
        # Dev Drive needs at least 50 GB; the expandable VHDX only grows by what is written.
        Invoke-Diskpart @(
            "create vdisk file=`"$vhdx`" maximum=51200 type=expandable",
            "select vdisk file=`"$vhdx`"",
            'attach vdisk',
            'convert gpt',
            'create partition primary',
            'assign letter=R'
        )
        Format-Volume -DriveLetter R -DevDrive -NewFileSystemLabel 'navo-offload' -Confirm:$false | Out-Null
        $volume = Get-Volume -DriveLetter R
    }
    return $volume.FileSystem -eq 'ReFS'
}

$devDrive = $false
try {
    $devDrive = Initialize-DevDrive
    if (-not $devDrive) { Write-Output 'devdrive: unavailable (R: is not ReFS)' }
} catch {
    Write-Output "devdrive: unavailable ($($_.Exception.Message))"
}
if ($devDrive) { Write-Output 'devdrive: R:\' }

$env:NAVO_TEST_SMB_DIR = '\\localhost\navo-offload'
$env:NAVO_TEST_DEVDRIVE_DIR = if ($devDrive) { 'R:\' } else { '' }
$env:NAVO_TEST_OFFLOAD_OUT = $out
Remove-Item -LiteralPath $out -Force -ErrorAction SilentlyContinue
# The same profile settings as the VM CI, so its test build is reused.
$env:CARGO_PROFILE_DEV_DEBUG = 'line-tables-only'
$env:CARGO_PROFILE_TEST_DEBUG = 'line-tables-only'
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_TERM_COLOR = 'never'
Set-Location (Split-Path -Parent $PSScriptRoot)
# One test at a time, so the copies do not share the disk and the ms numbers stay comparable.
cmd /c "cargo nextest run -p navo-fs --run-ignored only -E ""test(copy_offload)"" --no-fail-fast --test-threads 1 2>&1"
$code = $LASTEXITCODE
if (Test-Path -LiteralPath $out) { Get-Content -LiteralPath $out }
exit $code
