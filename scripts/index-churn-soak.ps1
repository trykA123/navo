param(
    [Parameter(Mandatory)][string]$Folder,
    [Parameter(Mandatory)][string]$Exe,
    [int]$Waves = 6,
    [int]$Live = 50000,
    [int]$Churn = 250000,
    [int]$HoldSeconds = 60,
    [int]$SettleSeconds = 30,
    [string]$Service = 'Navo Index',
    [string]$Out = ''
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Folder | Out-Null

function ServiceProcess {
    $svc = Get-CimInstance Win32_Service -Filter "Name='$Service' OR DisplayName='$Service'"
    if (-not $svc -or -not $svc.ProcessId) { throw "service '$Service' is not running" }
    Get-Process -Id $svc.ProcessId
}

function Sample($wave, $phase) {
    $p = ServiceProcess
    $json = & $Exe --index-memory --json
    if ($LASTEXITCODE -ne 0) { throw "index memory sample failed (exit $LASTEXITCODE)" }
    $memory = $json | ConvertFrom-Json
    if ($null -eq $script:firstPid) { $script:firstPid = $p.Id }
    $pidMismatch = $null -ne $memory.service_process_id -and $memory.service_process_id -ne $p.Id
    [pscustomobject]@{
        wave = $wave
        pid = $p.Id
        restartedSinceBaseline = ($p.Id -ne $script:firstPid) -or $pidMismatch
        phase = $phase
        privateMB = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
        workingSetMB = [math]::Round($p.WorkingSet64 / 1MB, 1)
        peakWorkingSetMB = [math]::Round($p.PeakWorkingSet64 / 1MB, 1)
        handles = $p.HandleCount
        indexMemory = $memory
        time = (Get-Date).ToString('o')
    }
}

function MakeFiles($dir, $from, $count) {
    $to = $from + $count - 1
    foreach ($i in $from..$to) { [System.IO.File]::Create((Join-Path $dir "f$i.tmp")).Dispose() }
}

$samples = @()
$liveDir = Join-Path $Folder 'live'
New-Item -ItemType Directory -Force -Path $liveDir | Out-Null
MakeFiles $liveDir 0 $Live
Start-Sleep -Seconds $SettleSeconds
$samples += Sample 0 'baseline'

$next = $Live
foreach ($wave in 1..$Waves) {
    $scratch = Join-Path $Folder "scratch-$wave"
    New-Item -ItemType Directory -Force -Path $scratch | Out-Null
    MakeFiles $scratch $next $Churn
    $next += $Churn
    Start-Sleep -Seconds $HoldSeconds
    $samples += Sample $wave 'created'
    Remove-Item -LiteralPath $scratch -Recurse -Force
    Start-Sleep -Seconds $SettleSeconds
    $samples += Sample $wave 'settled'
}

$samples | Format-Table -AutoSize
if ($Out) { $samples | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $Out }
