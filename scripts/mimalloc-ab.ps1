param(
    [Parameter(Mandatory)][string]$ExeA,
    [Parameter(Mandatory)][string]$ExeB,
    [Parameter(Mandatory)][string]$Work,
    [int]$MetricsRuns = 7,
    [int]$BenchRuns = 3,
    [int]$BenchFiles = 1000000
)
$ErrorActionPreference = 'Stop'
$exes = [ordered]@{ A = (Resolve-Path -LiteralPath $ExeA).Path; B = (Resolve-Path -LiteralPath $ExeB).Path }
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$fixture = Join-Path $Work 'fixture'
if (-not (Test-Path -LiteralPath $fixture)) {
    $flat = Join-Path $fixture 'flat'
    New-Item -ItemType Directory -Force -Path $flat | Out-Null
    1..5000 | ForEach-Object { New-Item -ItemType File -Path (Join-Path $flat "file-$_.txt") | Out-Null }
    $nested = Join-Path $fixture 'nested'
    1..200 | ForEach-Object {
        $dir = Join-Path $nested "dir-$_"
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        1..25 | ForEach-Object { New-Item -ItemType File -Path (Join-Path $dir "file-$_.txt") | Out-Null }
    }
}

function Median($values) {
    $sorted = @($values | Sort-Object)
    $sorted[[int][math]::Floor(($sorted.Count - 1) / 2)]
}

$metrics = @{ A = @(); B = @() }
foreach ($round in 0..$MetricsRuns) {
    foreach ($name in $exes.Keys) {
        $profile = Join-Path $Work "profile-$name"
        $out = Join-Path $Work "metrics-$name-$round.json"
        Remove-Item -LiteralPath $out -ErrorAction SilentlyContinue
        $argList = @('--metrics', "`"$out`"", '--profile', "`"$profile`"", '--left', "`"$fixture\flat`"", '--right', "`"$fixture\nested`"")
        $proc = Start-Process -FilePath $exes[$name] -ArgumentList $argList -PassThru -Wait -NoNewWindow
        if ($proc.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $out)) { throw "metrics run $name/$round failed ($($proc.ExitCode))" }
        if ($round -gt 0) { $metrics[$name] += , (Get-Content -LiteralPath $out -Raw | ConvertFrom-Json) }
    }
}

$bench = @{ A = @(); B = @() }
foreach ($round in 1..$BenchRuns) {
    foreach ($name in $exes.Keys) {
        $root = Join-Path $Work "bench-$name"
        $out = Join-Path $Work "bench-$name-$round.json"
        Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
        $proc = Start-Process -FilePath $exes[$name] -ArgumentList @('bench', '--root', "`"$root`"", '--files', $BenchFiles, '--out', "`"$out`"") -PassThru -Wait -NoNewWindow
        if ($proc.ExitCode -ne 0) { throw "bench run $name/$round failed ($($proc.ExitCode))" }
        $bench[$name] += , (Get-Content -LiteralPath $out -Raw | ConvertFrom-Json)
    }
}

$rows = @()
foreach ($name in $exes.Keys) {
    $m = $metrics[$name]
    $b = $bench[$name]
    $huge = { param($r) $r.listing | Sort-Object entries -Descending | Select-Object -First 1 }
    $rows += [pscustomobject]@{
        exe                    = $name
        first_usable_ms        = Median ($m | ForEach-Object { $_.first_usable_ms })
        first_frame_ms         = Median ($m | ForEach-Object { $_.first_frame_ms })
        idle_private_mb        = [math]::Round((Median ($m | ForEach-Object { $_.idle_private_bytes })) / 1e6, 1)
        peak_working_set_mb    = [math]::Round((Median ($m | ForEach-Object { $_.peak_working_set_bytes })) / 1e6, 1)
        list_500k_ms           = Median ($b | ForEach-Object { (& $huge $_).list_ms })
        index_ms               = Median ($b | ForEach-Object { $_.index.ms })
        index_peak_private_mb  = [math]::Round((Median ($b | ForEach-Object { $_.index.peak.private_bytes })) / 1e6, 1)
        bench_peak_ws_mb       = [math]::Round((Median ($b | ForEach-Object { $_.index.process_peak_working_set_bytes })) / 1e6, 1)
        search_p95_ms          = Median ($b | ForEach-Object { Median ($_.search | ForEach-Object { $_.p95_ms }) })
    }
}
$rows | Format-Table -AutoSize | Out-String | Write-Output
$rows | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Work 'ab-summary.json')
