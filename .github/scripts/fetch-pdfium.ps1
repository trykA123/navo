param(
    [Parameter(Mandatory)] [string] $Destination,
    [string] $Licenses
)
$ErrorActionPreference = 'Stop'
$tag = $env:PDFIUM_TAG
$expected = $env:PDFIUM_SHA256
if (-not $tag -or -not $expected) { throw 'PDFIUM_TAG and PDFIUM_SHA256 must be set' }
$work = Join-Path $env:RUNNER_TEMP 'pdfium'
New-Item -ItemType Directory -Force -Path $work | Out-Null
$archive = Join-Path $work 'pdfium-win-x64.tgz'
$url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$([uri]::EscapeDataString($tag))/pdfium-win-x64.tgz"
Invoke-WebRequest -Uri $url -OutFile $archive
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "pdfium-win-x64.tgz sha256 is $actual, expected $expected" }
tar -xzf $archive -C $work
if ($LASTEXITCODE -ne 0) { throw "tar failed with $LASTEXITCODE" }
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
Copy-Item -LiteralPath (Join-Path $work 'bin/pdfium.dll') -Destination $Destination
if ($Licenses) {
    New-Item -ItemType Directory -Force -Path $Licenses | Out-Null
    Copy-Item -LiteralPath (Join-Path $work 'LICENSE') -Destination $Licenses
    Copy-Item -Path (Join-Path $work 'licenses/*') -Destination $Licenses
}
