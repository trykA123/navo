# Soak-tests the Navo index service under file creation, deletion, hard-link, move, and burst churn.
# Samples service memory per phase and verifies the service log stays free of rebuilds and update errors.

param(
    [Parameter(Mandatory)][string]$Folder,
    [Parameter(Mandatory)][string]$Exe,
    [int]$Waves = 6,
    [int]$Live = 50000,
    [int]$Churn = 250000,
    [int]$HoldSeconds = 60,
    [int]$SettleSeconds = 30,
    [string]$Service = 'Navo Index',
    [string]$Out = '',
    [int]$HardLinks = 0,
    [int]$Moves = 0,
    [int]$Burst = 100000,
    [string]$LogPath = 'C:\ProgramData\Navo\index\logs\index-service.log'
)
$ErrorActionPreference = 'Stop'
$runStart = [DateTimeOffset]::FromUnixTimeSeconds([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())
New-Item -ItemType Directory -Force -Path $Folder | Out-Null

# Returns the running service process or throws if the service is stopped.
function ServiceProcess {
    $svc = Get-CimInstance Win32_Service -Filter "Name='$Service' OR DisplayName='$Service'"
    if (-not $svc -or -not $svc.ProcessId) { throw "service '$Service' is not running" }
    Get-Process -Id $svc.ProcessId
}

# Queries index memory stats via the GUI-subsystem exe using redirected stdout.
function IndexMemory {
    for ($attempt = 0; $attempt -lt 15; $attempt++) {
        $psi = New-Object System.Diagnostics.ProcessStartInfo($Exe, '--index-memory --json')
        $psi.RedirectStandardOutput = $true
        $psi.UseShellExecute = $false
        $proc = [System.Diagnostics.Process]::Start($psi)
        $out = $proc.StandardOutput.ReadToEnd()
        $proc.WaitForExit()
        if ($proc.ExitCode -eq 0 -and $out.Trim()) {
            $parsed = $out | ConvertFrom-Json
            if ($null -ne $parsed.volumes) { return $parsed }
        }
        Start-Sleep -Seconds 1
    }
    throw 'index memory sample failed after retries'
}

# Captures a memory and handle snapshot of the service for the given wave and phase.
function Sample($wave, $phase) {
    $p = ServiceProcess
    $memory = IndexMemory
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

# Creates $count empty files numbered from $from in $dir.
function MakeFiles($dir, $from, $count) {
    if ($count -le 0) { return }
    $to = $from + $count - 1
    foreach ($i in $from..$to) { [System.IO.File]::Create((Join-Path $dir "f$i.tmp")).Dispose() }
}

# Creates $count hard links to existing files in $Folder, then deletes half of the original names.
function Invoke-HardLinks([string]$root, [string]$preferredDir, [int]$count) {
    if ($count -le 0) { return }
    $targets = @()
    if (Test-Path -LiteralPath $preferredDir) {
        $targets = [System.IO.Directory]::GetFiles($preferredDir)
    }
    if ($targets.Count -eq 0) {
        $targets = [System.IO.Directory]::GetFiles($root, '*', [System.IO.SearchOption]::AllDirectories)
    }
    if ($targets.Count -eq 0) { throw "no existing files in '$root' to hard-link" }

    $hlDir = Join-Path $root 'hardlinks'
    New-Item -ItemType Directory -Force -Path $hlDir | Out-Null
    $linkedOriginals = [System.Collections.Generic.List[string]]::new()
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

    for ($i = 0; $i -lt $count; $i++) {
        $target = $targets[$i % $targets.Count]
        $linkPath = Join-Path $hlDir "hl-$i.tmp"
        New-Item -ItemType HardLink -Path $linkPath -Target $target | Out-Null
        if ($seen.Add($target)) { $linkedOriginals.Add($target) }
    }

    $deleteCount = [math]::Floor($linkedOriginals.Count / 2)
    for ($i = 0; $i -lt $deleteCount; $i++) {
        [System.IO.File]::Delete($linkedOriginals[$i])
    }
}

# Creates $count folders with subfolders and files, then moves each folder to another parent.
function Invoke-FolderMoves([string]$root, [int]$count) {
    if ($count -le 0) { return }
    $movesRoot = Join-Path $root 'moves'
    $srcParent = Join-Path $movesRoot 'src'
    $dstParent = Join-Path $movesRoot 'dst'
    New-Item -ItemType Directory -Force -Path $srcParent, $dstParent | Out-Null

    for ($i = 0; $i -lt $count; $i++) {
        $itemDir = Join-Path $srcParent "dir-$i"
        $subDir = Join-Path $itemDir 'sub'
        [System.IO.Directory]::CreateDirectory($subDir) | Out-Null
        [System.IO.File]::Create((Join-Path $itemDir 'a.tmp')).Dispose()
        [System.IO.File]::Create((Join-Path $subDir 'b.tmp')).Dispose()
    }

    for ($i = 0; $i -lt $count; $i++) {
        $src = Join-Path $srcParent "dir-$i"
        $dst = Join-Path $dstParent "dir-$i"
        [System.IO.Directory]::Move($src, $dst)
    }
}

# Creates $count small files as fast as possible across sharded directories.
function Invoke-Burst([string]$root, [int]$count) {
    if ($count -le 0) { return }
    $burstDir = Join-Path $root 'burst'
    $buckets = [math]::Min(100, [math]::Max(1, [math]::Ceiling($count / 1000)))
    $bucketDirs = [string[]]::new($buckets)
    for ($b = 0; $b -lt $buckets; $b++) {
        $d = Join-Path $burstDir "d$b"
        [System.IO.Directory]::CreateDirectory($d) | Out-Null
        $bucketDirs[$b] = $d
    }
    for ($i = 0; $i -lt $count; $i++) {
        $dir = $bucketDirs[$i % $buckets]
        [System.IO.File]::Create([System.IO.Path]::Combine($dir, "b$i.tmp")).Dispose()
    }
}

# Reads service log message bodies whose timestamp is at or after $since (shared read, no locking).
function Read-LogMessagesAfter([string]$path, [DateTimeOffset]$since) {
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    $messages = [System.Collections.Generic.List[string]]::new()
    $fs = [System.IO.File]::Open($path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $sr = [System.IO.StreamReader]::new($fs)
        try {
            while ($null -ne ($line = $sr.ReadLine())) {
                if ($line -match '^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?)\s+(.*)$') {
                    $ts = [DateTimeOffset]::MinValue
                    if ([DateTimeOffset]::TryParse(
                            $Matches[1],
                            [System.Globalization.CultureInfo]::InvariantCulture,
                            [System.Globalization.DateTimeStyles]::AssumeUniversal,
                            [ref]$ts)) {
                        if ($ts -ge $since) { $messages.Add($Matches[2]) }
                    }
                }
            }
        }
        finally { $sr.Dispose() }
    }
    finally { $fs.Dispose() }
    return $messages
}

# Returns true when every volume in a heartbeat line reports progress=idle.
function Test-HeartbeatIdle([string]$message) {
    $progressMatches = [regex]::Matches($message, 'progress=(\S+)')
    if ($progressMatches.Count -eq 0) { return $false }
    foreach ($m in $progressMatches) {
        if ($m.Groups[1].Value -ne 'idle') { return $false }
    }
    return $true
}

# Polls the service log after $burstEnd until an idle heartbeat appears, then prints elapsed seconds.
function Wait-IdleHeartbeat([string]$path, [DateTimeOffset]$burstEnd, [int]$timeoutSeconds) {
    $since = [DateTimeOffset]::FromUnixTimeSeconds($burstEnd.ToUnixTimeSeconds())
    $deadline = $burstEnd.AddSeconds($timeoutSeconds)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        $messages = Read-LogMessagesAfter -path $path -since $since
        if ($null -ne $messages) {
            foreach ($msg in $messages) {
                if ($msg -match '^heartbeat\b' -and (Test-HeartbeatIdle $msg)) {
                    $elapsed = [math]::Round(([DateTimeOffset]::UtcNow - $burstEnd).TotalSeconds, 1)
                    Write-Output "Burst catch-up to idle heartbeat: $elapsed s"
                    return $true
                }
            }
        }
        Start-Sleep -Seconds 1
    }
    Write-Output "Burst catch-up timed out after $timeoutSeconds s without an idle heartbeat."
    return $false
}

# Fails with exit 1 if the service log shows a post-build full rebuild, an update error, or non-idle heartbeat.
function Assert-ServiceLogHealthy([string]$path, [DateTimeOffset]$since) {
    $messages = Read-LogMessagesAfter -path $path -since $since
    if ($null -eq $messages) {
        [Console]::Error.WriteLine("FAIL: service log not found at '$path'.")
        exit 1
    }

    $sawBuildDone = $false
    $rebuildAfterDone = $false
    $updateErrors = 0
    $lastHeartbeat = $null

    foreach ($msg in $messages) {
        if ($msg -match '^(?:full )?build done\b') {
            $sawBuildDone = $true
        }
        elseif ($sawBuildDone -and $msg -match '^full build start\b') {
            $rebuildAfterDone = $true
        }
        elseif ($msg -match '^update error\b') {
            $updateErrors++
        }
        elseif ($msg -match '^heartbeat\b') {
            $lastHeartbeat = $msg
        }
    }

    if ($rebuildAfterDone) {
        [Console]::Error.WriteLine("FAIL: 'full build start' appeared after 'build done' during this run.")
        exit 1
    }
    if ($updateErrors -gt 0) {
        [Console]::Error.WriteLine("FAIL: $updateErrors 'update error' line(s) appeared during this run.")
        exit 1
    }
    if ($null -ne $lastHeartbeat -and -not (Test-HeartbeatIdle $lastHeartbeat)) {
        [Console]::Error.WriteLine("FAIL: last heartbeat line is not idle.")
        exit 1
    }
}

$samples = @()
$liveDir = Join-Path $Folder 'live'
New-Item -ItemType Directory -Force -Path $liveDir | Out-Null
MakeFiles $liveDir 0 $Live
Start-Sleep -Seconds $SettleSeconds
$samples += Sample 0 'baseline'

$next = $Live
if ($Waves -gt 0) {
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
}

if ($HardLinks -gt 0) {
    Invoke-HardLinks -root $Folder -preferredDir $liveDir -count $HardLinks
    Start-Sleep -Seconds $SettleSeconds
    $samples += Sample ($Waves + 1) 'hardlinks'
}

if ($Moves -gt 0) {
    Invoke-FolderMoves -root $Folder -count $Moves
    Start-Sleep -Seconds $SettleSeconds
    $samples += Sample ($Waves + 2) 'moves'
}

if ($Burst -gt 0) {
    Invoke-Burst -root $Folder -count $Burst
    $burstEnd = [DateTimeOffset]::UtcNow
    $pollTimeout = [math]::Max($SettleSeconds + 180, 180)
    $null = Wait-IdleHeartbeat -path $LogPath -burstEnd $burstEnd -timeoutSeconds $pollTimeout
    $remainingSettle = [int][math]::Ceiling($SettleSeconds - ([DateTimeOffset]::UtcNow - $burstEnd).TotalSeconds)
    if ($remainingSettle -gt 0) { Start-Sleep -Seconds $remainingSettle }
    $samples += Sample ($Waves + 3) 'burst'
}

$samples | Format-Table -AutoSize
if ($Out) { $samples | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $Out }

Assert-ServiceLogHealthy -path $LogPath -since $runStart
