# Installer DLL guard check (IW24): fails when Navo-Setup.exe imports a DLL at load time
# that is not a KnownDLL. Such a DLL could be planted beside the exe in Downloads.
param(
    [string] $Exe = 'target/release/navo-setup.exe',
    [string] $BuildRs = 'crates/navo-setup/build.rs',
    [string] $Dumpbin = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\dumpbin.exe'
)

$ErrorActionPreference = 'Stop'

# Returns the lower-case DLL names in one dumpbin imports section ("imports" or "delay load imports").
function Get-ImportSection([string[]] $Lines, [string] $Header) {
    $names = [Collections.Generic.List[string]]::new()
    $inside = $false
    foreach ($line in $Lines) {
        if ($line -match '^\s*Section contains the following (.+)$') {
            $inside = ($Matches[1].Trim().TrimEnd(':') -eq $Header)
            continue
        }
        # Any name at the 4-space indent is a module, whatever its extension (.dll, .drv, .exe, .ocx).
        if ($inside -and $line -match '^\s{4}(\S+)\s*$') {
            $names.Add($Matches[1].ToLowerInvariant())
        }
        if ($inside -and $line -match '^\s*Summary\s*$') { $inside = $false }
    }
    return $names
}

if (-not (Test-Path -LiteralPath $Exe)) { throw "missing $Exe" }
if (-not (Test-Path -LiteralPath $Dumpbin)) { throw "missing $Dumpbin" }

$lines = & $Dumpbin /nologo /imports $Exe
if ($LASTEXITCODE -ne 0) { throw "dumpbin exited $LASTEXITCODE" }

$known = @{}
$key = Get-Item -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs'
foreach ($name in $key.GetValueNames()) {
    $value = [string] $key.GetValue($name)
    if ($value -like '*.dll') { $known[$value.ToLowerInvariant()] = $true }
}
# The kernel maps ntdll into every process before any DLL search, so it cannot be planted.
$known['ntdll.dll'] = $true

$direct = Get-ImportSection $lines 'imports'
$delayed = Get-ImportSection $lines 'delay load imports'
Write-Host "Load-time imports: $($direct -join ', ')"
Write-Host "Delay-loaded imports: $($delayed -join ', ')"
# Every Windows exe imports kernel32; an empty list means the parse failed, not a clean exe.
if ($direct.Count -eq 0) { Write-Host 'FAIL: no load-time imports parsed'; exit 1 }

# Without a payload the linker drops the runner and its imports, so such a build proves nothing.
$listed = [regex]::Matches((Get-Content -Raw -LiteralPath $BuildRs), '"([A-Za-z0-9_.-]+\.dll)"') |
    ForEach-Object { $_.Groups[1].Value.ToLowerInvariant() }
$missing = @($listed | Where-Object { $delayed -notcontains $_ })
if ($missing.Count -gt 0) {
    Write-Host "FAIL: DELAY_LOAD entries not delay-loaded (build without a payload?): $($missing -join ', ')"
    exit 1
}

# API-set names resolve through the loader's schema, never through the search path.
$unsafe = @($direct | Where-Object { -not $known.ContainsKey($_) -and $_ -notmatch '^(api|ext)-ms-' })
if ($unsafe.Count -gt 0) {
    Write-Host "FAIL: load-time imports outside KnownDLLs: $($unsafe -join ', ')"
    exit 1
}
Write-Host 'PASS: every load-time import is a KnownDLL'
exit 0
