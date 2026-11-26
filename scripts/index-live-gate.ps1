# Live-volume gate: attaches a scratch NTFS VHD, starts the index service against it,
# churns files while the first build is still running, then verifies the service caught up
# without rebuilds, errors or batch copies, and that xtask index-diff agrees with the disk.
# Requires an elevated session (diskpart, service install).
#requires -Version 5.1
param(
    [Parameter(Mandatory)][string]$ServiceExe,
    [string]$XtaskExe = '',
    [string]$RepoRoot = '',
    [int]$VhdSizeMB = 4096,
    [int]$Burst = 100000,
    [int]$HardLinks = 20000,
    [int]$Moves = 500,
    [int]$Waves = 3,
    [int]$WaveSize = 50000,
    [int]$CatchUpLimitSeconds = 30,
    [string]$ServiceName = 'Navo Index',
    [string]$LogPath = 'C:\ProgramData\Navo\index\logs\index-service.log'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not $XtaskExe -and -not $RepoRoot) {
    throw 'supply -XtaskExe or -RepoRoot so the script can locate cargo xtask'
}

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'index-live-gate requires an elevated session'
}

$work = Join-Path $env:TEMP ('index-live-gate-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$vhdPath = Join-Path $work 'scratch.vhdx'
$driveLetter = $null
$serviceInstalled = $false
$exitCode = 1
$runStart = [DateTimeOffset]::UtcNow

try {
    # --- VHD create and attach ---------------------------------------------------
    Write-Output "creating $VhdSizeMB MB VHD at $vhdPath"
    $dpScript = Join-Path $work 'diskpart.txt'
    @(
        "create vdisk file=`"$vhdPath`" maximum=$VhdSizeMB type=expandable",
        "select vdisk file=`"$vhdPath`"",
        'attach vdisk',
        'convert gpt',
        'create partition primary',
        'assign letter=V'
    ) | Set-Content -LiteralPath $dpScript -Encoding ascii
    $dpOut = & diskpart.exe /s $dpScript 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw "diskpart create failed ($LASTEXITCODE): $dpOut" }

    Start-Sleep -Seconds 2
    Format-Volume -DriveLetter V -FileSystem NTFS -NewFileSystemLabel 'navo-live-gate' -Confirm:$false | Out-Null
    $driveLetter = 'V'
    $vhdRoot = "${driveLetter}:\"
    Write-Output "VHD attached as $vhdRoot"

    # --- Service configuration ---------------------------------------------------
    $configDir = 'C:\ProgramData\Navo\index'
    New-Item -ItemType Directory -Force -Path (Join-Path $configDir 'logs') | Out-Null

    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $search = @{
        'drives' = @("${driveLetter}:")
        'exclude' = @('$Recycle.Bin', 'System Volume Information')
        'network' = $false
        'removable' = $false
        'remember' = $false
    }
    $serviceConfig = @{
        sid = $sid
        search = $search
    }
    $configJson = Join-Path $configDir 'service.json'
    $serviceConfig | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $configJson -Encoding utf8

    # Remove any stale log before the run.
    Remove-Item -LiteralPath $LogPath -Force -ErrorAction SilentlyContinue

    # --- Service install and start -----------------------------------------------
    Write-Output 'installing index service'
    $installResult = & $ServiceExe --index-service install 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { throw "service install failed ($LASTEXITCODE): $installResult" }
    $serviceInstalled = $true

    $svc = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
    if (-not $svc -or $svc.State -ne 'Running') {
        Start-Sleep -Seconds 3
        $svc = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
        if (-not $svc -or $svc.State -ne 'Running') { throw 'service did not reach Running state' }
    }
    Write-Output 'service is running'

    # --- Churn: start before the first full build finishes -----------------------
    $churnStart = [DateTimeOffset]::UtcNow

    # Wave 0: burst files immediately so the journal has changes during the first build.
    if ($Burst -gt 0) {
        Write-Output "burst: $Burst files"
        $burstDir = Join-Path $vhdRoot 'burst'
        $buckets = [math]::Min(100, [math]::Max(1, [math]::Ceiling($Burst / 1000)))
        $bucketDirs = [string[]]::new($buckets)
        for ($b = 0; $b -lt $buckets; $b++) {
            $d = Join-Path $burstDir "d$b"
            [System.IO.Directory]::CreateDirectory($d) | Out-Null
            $bucketDirs[$b] = $d
        }
        for ($i = 0; $i -lt $Burst; $i++) {
            $dir = $bucketDirs[$i % $buckets]
            [System.IO.File]::Create([System.IO.Path]::Combine($dir, "b$i.tmp")).Dispose()
        }
    }

    # Hard links.
    if ($HardLinks -gt 0) {
        Write-Output "hard links: $HardLinks"
        $hlDir = Join-Path $vhdRoot 'hardlinks'
        New-Item -ItemType Directory -Force -Path $hlDir | Out-Null
        $targets = [System.IO.Directory]::GetFiles($vhdRoot, '*', [System.IO.SearchOption]::AllDirectories)
        if ($targets.Count -gt 0) {
            for ($i = 0; $i -lt $HardLinks; $i++) {
                $target = $targets[$i % $targets.Count]
                New-Item -ItemType HardLink -Path (Join-Path $hlDir "hl-$i.tmp") -Target $target | Out-Null
            }
        }
    }

    # Folder moves.
    if ($Moves -gt 0) {
        Write-Output "folder moves: $Moves"
        $movesSrc = Join-Path $vhdRoot 'moves-src'
        $movesDst = Join-Path $vhdRoot 'moves-dst'
        New-Item -ItemType Directory -Force -Path $movesSrc, $movesDst | Out-Null
        for ($i = 0; $i -lt $Moves; $i++) {
            $itemDir = Join-Path $movesSrc "dir-$i"
            $subDir = Join-Path $itemDir 'sub'
            [System.IO.Directory]::CreateDirectory($subDir) | Out-Null
            [System.IO.File]::Create((Join-Path $itemDir 'a.tmp')).Dispose()
            [System.IO.File]::Create((Join-Path $subDir 'b.tmp')).Dispose()
        }
        for ($i = 0; $i -lt $Moves; $i++) {
            [System.IO.Directory]::Move(
                (Join-Path $movesSrc "dir-$i"),
                (Join-Path $movesDst "dir-$i")
            )
        }
    }

    # Additional churn waves.
    if ($Waves -gt 0) {
        $next = $Burst + $HardLinks
        foreach ($wave in 1..$Waves) {
            Write-Output "wave $wave`: $WaveSize files"
            $scratch = Join-Path $vhdRoot "scratch-$wave"
            New-Item -ItemType Directory -Force -Path $scratch | Out-Null
            $from = $next
            $to = $from + $WaveSize - 1
            foreach ($i in $from..$to) {
                [System.IO.File]::Create((Join-Path $scratch "f$i.tmp")).Dispose()
            }
            $next = $to + 1
        }
    }

    $lastChurn = [DateTimeOffset]::UtcNow
    $totalOps = $Burst + $HardLinks + $Moves + ($Waves * $WaveSize)
    Write-Output "churn complete: $totalOps operations in $([math]::Round(($lastChurn - $churnStart).TotalSeconds, 1)) s"

    # --- Wait for idle heartbeat after last churn --------------------------------
    Write-Output "waiting for idle heartbeat (limit ${CatchUpLimitSeconds}s)..."
    $caughtUp = $false
    $catchUpSeconds = 0.0
    $deadline = $lastChurn.AddSeconds($CatchUpLimitSeconds + 120)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $LogPath) {
            $logLines = [System.IO.File]::ReadAllLines($LogPath)
            foreach ($line in $logLines) {
                if ($line -match 'heartbeat\b' -and $line -match 'progress=idle') {
                    $catchUpSeconds = [math]::Round(([DateTimeOffset]::UtcNow - $lastChurn).TotalSeconds, 1)
                    $caughtUp = $true
                    break
                }
            }
            if ($caughtUp) { break }
        }
        Start-Sleep -Seconds 2
    }

    if (-not $caughtUp) {
        Write-Output "FAIL: no idle heartbeat within the deadline"
        $exitCode = 1
        return
    }
    Write-Output "catch-up: $catchUpSeconds s"

    # --- Stop the service (saves the snapshot) -----------------------------------
    Write-Output 'stopping service'
    $stopResult = & $ServiceExe --index-service remove 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        Write-Output "service remove returned $LASTEXITCODE`: $stopResult"
    }
    $serviceInstalled = $false

    # --- Run xtask index-diff ----------------------------------------------------
    $snapshotPath = "C:\ProgramData\Navo\index\${driveLetter}.index"
    if (-not (Test-Path -LiteralPath $snapshotPath)) {
        Write-Output "FAIL: snapshot not found at $snapshotPath"
        $exitCode = 1
        return
    }

    if ($XtaskExe) {
        $xtaskArgs = @('index-diff', '--snapshot', $snapshotPath, '--root', $vhdRoot)
        Write-Output "running: $XtaskExe $($xtaskArgs -join ' ')"
        $diffOut = & $XtaskExe @xtaskArgs 2>&1 | Out-String
        $diffExit = $LASTEXITCODE
    } else {
        $xtaskArgs = @('xtask', 'index-diff', '--snapshot', $snapshotPath, '--root', $vhdRoot)
        Write-Output "running: cargo $($xtaskArgs -join ' ')"
        Push-Location $RepoRoot
        try {
            $diffOut = & cargo @xtaskArgs 2>&1 | Out-String
            $diffExit = $LASTEXITCODE
        } finally { Pop-Location }
    }
    Write-Output "index-diff: $diffOut"
    if ($diffExit -ne 0) {
        Write-Output "FAIL: index-diff exited $diffExit"
        $exitCode = 1
        return
    }

    # --- Inspect the service log -------------------------------------------------
    $logText = ''
    if (Test-Path -LiteralPath $LogPath) {
        $logText = [System.IO.File]::ReadAllText($LogPath)
    }

    $fullBuildStarts = @($logText -split "`n" | Where-Object { $_ -match 'full build start' })
    if ($fullBuildStarts.Count -gt 1) {
        Write-Output "FAIL: $($fullBuildStarts.Count) 'full build start' lines in log (expected at most 1)"
        $exitCode = 1
        return
    }

    $updateErrors = @($logText -split "`n" | Where-Object { $_ -match 'update error' })
    if ($updateErrors.Count -gt 0) {
        Write-Output "FAIL: $($updateErrors.Count) 'update error' line(s) in log"
        $exitCode = 1
        return
    }

    $batchCopies = @($logText -split "`n" | Where-Object { $_ -match 'the batch copied it' })
    if ($batchCopies.Count -gt 0) {
        Write-Output "FAIL: $($batchCopies.Count) 'the batch copied it' line(s) in log"
        $exitCode = 1
        return
    }

    if ($catchUpSeconds -gt $CatchUpLimitSeconds) {
        Write-Output "FAIL: catch-up $catchUpSeconds s exceeded limit of ${CatchUpLimitSeconds}s"
        $exitCode = 1
        return
    }

    Write-Output 'PASS: live gate succeeded'
    $exitCode = 0
} catch {
    Write-Output "FAIL: $($_.Exception.Message)"
    $exitCode = 1
} finally {
    # --- Cleanup: always runs ----------------------------------------------------
    Write-Output ''
    Write-Output '=== cleanup ==='

    if ($serviceInstalled) {
        Write-Output 'removing service'
        & $ServiceExe --index-service remove 2>&1 | Out-Null
    }

    if ($driveLetter) {
        $detachScript = Join-Path $work 'detach.txt'
        @(
            "select vdisk file=`"$vhdPath`"",
            'detach vdisk'
        ) | Set-Content -LiteralPath $detachScript -Encoding ascii
        & diskpart.exe /s $detachScript 2>&1 | Out-Null
        Write-Output "VHD detached"
    }

    if (Test-Path -LiteralPath $vhdPath) {
        Remove-Item -LiteralPath $vhdPath -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $work) {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }

    if (Test-Path -LiteralPath $LogPath) {
        Write-Output ''
        Write-Output '=== index-service.log (last 40 lines) ==='
        $allLines = [System.IO.File]::ReadAllLines($LogPath)
        $tail = if ($allLines.Count -gt 40) { $allLines[-40..-1] } else { $allLines }
        foreach ($line in $tail) { Write-Output $line }
    }

    exit $exitCode
}
