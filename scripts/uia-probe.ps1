param(
    [string] $Exe = 'target/release/navo.exe',
    [string] $DumpPath = 'uia-dump.txt'
)

$ErrorActionPreference = 'Stop'
$DumpPath = [IO.Path]::GetFullPath($DumpPath)
Set-Content -LiteralPath $DumpPath -Value 'Navo UI Automation probe' -Encoding utf8
$failures = [Collections.Generic.List[string]]::new()
$app = $null
$work = $null
$exitCode = 2

function Write-ProbeLog([string] $Message) {
    Write-Host $Message
    Add-Content -LiteralPath $DumpPath -Value $Message -Encoding utf8
}

function Assert-Probe([bool] $Condition, [string] $Message) {
    if ($Condition) {
        Write-ProbeLog "PASS: $Message"
    } else {
        $failures.Add($Message)
        Write-ProbeLog "FAIL: $Message"
    }
}

function Get-Tree($Element, [int] $Depth = 0) {
    if ($Depth -gt 40) { throw 'UIA tree exceeds the probe depth limit' }
    $current = $Element.Current
    [pscustomobject]@{
        Element = $Element
        Depth = $Depth
        Type = $current.ControlType.ProgrammaticName
        Name = $current.Name
        Id = $current.AutomationId
        Focused = $current.HasKeyboardFocus
    }
    $walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
    $child = $walker.GetFirstChild($Element)
    while ($null -ne $child) {
        Get-Tree $child ($Depth + 1)
        $child = $walker.GetNextSibling($child)
    }
}

function Wait-Tree([scriptblock] $Ready) {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $app.Refresh()
        if ($app.HasExited) { throw "Navo exited during the probe: $($app.ExitCode)" }
        $tree = @(Get-Tree $window)
        if (& $Ready $tree) { return $tree }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    return $tree
}

function Write-TreeDump([string] $Phase, [object[]] $Tree) {
    Write-ProbeLog "=== $Phase ==="
    foreach ($node in $Tree) {
        $name = ConvertTo-Json -InputObject $node.Name -Compress
        $id = ConvertTo-Json -InputObject $node.Id -Compress
        Write-ProbeLog "$('  ' * $node.Depth)$($node.Type) Name=$name Id=$id Focused=$($node.Focused)"
    }
}

function Get-Items($Element) {
    @(Get-Tree $Element | Where-Object { $_.Type -in 'ControlType.DataItem', 'ControlType.ListItem' })
}

function Assert-NamedItems([object[]] $Items, [string] $Label) {
    Assert-Probe ($Items.Count -gt 0) "$Label exposes DataItem or ListItem children"
    $unnamed = @($Items | Where-Object { [string]::IsNullOrWhiteSpace($_.Name) })
    Assert-Probe ($unnamed.Count -eq 0) "$Label has no unnamed children ($($unnamed.Count) unnamed)"
}

function Send-ProbeKeys([string] $Keys) {
    if (-not [NavoUiaWindow]::SetForegroundWindow($windowHandle)) {
        throw 'Cannot focus Navo on the Windows desktop'
    }
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
}

try {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class NavoUiaWindow {
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr window);
}
'@
    $exePath = (Resolve-Path -LiteralPath $Exe).Path
    $work = Join-Path ([IO.Path]::GetTempPath()) ('navo-uia-' + [guid]::NewGuid())
    $folder = Join-Path $work 'files'
    $profilePath = Join-Path $work 'profile'
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $folder 'uia-alpha.txt') -Value 'alpha'
    Set-Content -LiteralPath (Join-Path $folder 'uia-beta.txt') -Value 'beta'
    $arguments = @('--profile', "`"$profilePath`"", '--left', "`"$folder`"", '--right', "`"$folder`"")
    $app = Start-Process -FilePath $exePath -ArgumentList $arguments -PassThru
    $processCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $app.Id)
    $classCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Zed::Window')
    $windowCondition = [System.Windows.Automation.AndCondition]::new($processCondition, $classCondition)
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    $window = $null
    while ($null -eq $window) {
        $app.Refresh()
        if ($app.HasExited) { throw "Navo exited before opening its window: $($app.ExitCode)" }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Navo did not open a window within 30 seconds' }
        $window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst(
            [System.Windows.Automation.TreeScope]::Children, $windowCondition)
        if ($null -eq $window) { Start-Sleep -Milliseconds 250 }
    }
    $windowHandle = [IntPtr]$window.Current.NativeWindowHandle
    $tree = @(Wait-Tree { param($nodes)
        @($nodes | Where-Object { $_.Name -like '*uia-alpha.txt*' }).Count -gt 0
    })
    Write-TreeDump 'File list' $tree
    $lists = @($tree | Where-Object {
        $_.Type -in 'ControlType.List', 'ControlType.DataGrid', 'ControlType.Table' -and
        $_.Name -like "*$folder*"
    })
    Assert-Probe ($lists.Count -gt 0) 'The fixture file list is exposed'
    foreach ($list in $lists) {
        $items = @(Get-Items $list.Element)
        Assert-NamedItems $items 'File list'
        foreach ($fixture in 'uia-alpha.txt', 'uia-beta.txt') {
            Assert-Probe (@($items | Where-Object { $_.Name -like "*$fixture*" }).Count -gt 0) "File list names $fixture"
        }
    }

    Send-ProbeKeys '{HOME}{DOWN}{F8}'
    $tree = @(Wait-Tree { param($nodes)
        @($nodes | Where-Object { $_.Depth -gt 0 -and $_.Type -eq 'ControlType.Window' }).Count -gt 0
    })
    Write-TreeDump 'F8 confirmation' $tree
    $dialogs = @($tree | Where-Object { $_.Depth -gt 0 -and $_.Type -eq 'ControlType.Window' })
    Assert-Probe ($dialogs.Count -gt 0) 'F8 opens a dialog'
    foreach ($dialog in $dialogs) {
        Assert-Probe (-not [string]::IsNullOrWhiteSpace($dialog.Name)) 'F8 dialog has a name'
        $buttons = @(Get-Tree $dialog.Element | Where-Object { $_.Type -eq 'ControlType.Button' })
        Assert-Probe ($buttons.Count -ge 2) 'F8 dialog exposes confirm and cancel buttons'
        $unnamed = @($buttons | Where-Object { [string]::IsNullOrWhiteSpace($_.Name) })
        Assert-Probe ($unnamed.Count -eq 0) 'F8 dialog buttons have names'
    }

    Send-ProbeKeys '{ESC}'
    $tree = @(Wait-Tree { param($nodes)
        @($nodes | Where-Object { $_.Depth -gt 0 -and $_.Type -eq 'ControlType.Window' }).Count -eq 0
    })
    if (@($tree | Where-Object { $_.Depth -gt 0 -and $_.Type -eq 'ControlType.Window' }).Count -gt 0) {
        throw 'F8 dialog did not close with Escape'
    }
    Send-ProbeKeys '^k'
    $tree = @(Wait-Tree { param($nodes)
        @($nodes | Where-Object { $_.Type -eq 'ControlType.Edit' -and $_.Focused }).Count -gt 0
    })
    Send-ProbeKeys '>copy'
    $tree = @(Wait-Tree { param($nodes)
        @($nodes | Where-Object { $_.Depth -gt 0 -and $_.Type -eq 'ControlType.Window' }).Count -gt 0 -and
        @($nodes | Where-Object { $_.Type -eq 'ControlType.ListItem' -and $_.Name -match 'copy' }).Count -gt 0
    })
    Write-TreeDump 'Ctrl+K results' $tree
    $dialogs = @($tree | Where-Object { $_.Depth -gt 0 -and $_.Type -eq 'ControlType.Window' })
    Assert-Probe ($dialogs.Count -gt 0) 'Ctrl+K opens its dialog'
    $results = @($dialogs | ForEach-Object { Get-Items $_.Element })
    Assert-NamedItems $results 'Ctrl+K results'
    Assert-Probe (@($results | Where-Object { $_.Name -match 'copy' }).Count -gt 0) 'Ctrl+K returns named copy commands'
    $app.Refresh()
    if ($app.HasExited) { throw "Navo exited during the probe: $($app.ExitCode)" }
    $exitCode = [int]($failures.Count -gt 0)
    Write-ProbeLog "Probe finished: $($failures.Count) accessibility assertion(s) failed; Navo is still running."
} catch {
    Write-ProbeLog "PROBE ERROR: $($_.Exception.Message)"
} finally {
    try {
        if ($null -ne $app -and -not $app.HasExited) {
            Stop-Process -Id $app.Id -Force
            if (-not $app.WaitForExit(5000)) { throw 'Navo did not stop within five seconds' }
        }
        if ($null -ne $work -and (Test-Path -LiteralPath $work)) {
            Remove-Item -LiteralPath $work -Recurse -Force
        }
    } catch {
        Write-ProbeLog "CLEANUP ERROR: $($_.Exception.Message)"
        $exitCode = 2
    }
}

exit $exitCode
