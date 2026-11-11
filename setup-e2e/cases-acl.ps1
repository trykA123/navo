# DACL cases S16a and S16b of the installer's VM run: every folder Setup creates is protected,
# and an existing parent that other users can change is refused.

# Replaces the Folder field text with a full install folder path, then waits until it is shown.
function Set-FolderPath($wizard, [string]$path) {
    $field = @(Get-UiElements $wizard.Window '^Install folder' | Where-Object {
            $pattern = $null
            $_.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)
        })
    Assert ($field.Count -ge 1) 'the Folder page has no editable Install folder field'
    Invoke-Element ($field[0])
    Start-Sleep -Milliseconds 300
    [NavoE2E.Native]::Press([uint16][char]'A', $script:VK_CONTROL)
    [NavoE2E.Native]::Type($path)
    Wait-UiText $wizard.Window ([regex]::Escape($path)) 15
}

# S16a: typing a missing chain makes Setup create both folders with a protected DACL.
function Test-S16a {
    $parent = 'C:\NavoS16a'
    $root = Join-Path $parent 'Navo'
    if (Test-Path -LiteralPath $root) { Uninstall-Root $root }
    if (Test-Path -LiteralPath $parent) { Remove-Item -LiteralPath $parent -Recurse -Force }
    $wizard = Start-Wizard '0.0.2'
    Step-ToFolder $wizard 'Install Navo 0\.0\.2'
    Set-FolderPath $wizard $root
    Assert (-not (Test-Path -LiteralPath $parent)) "$parent exists before Install"
    Step-FreshInstall $wizard
    Finish-Wizard $wizard $false
    Assert ((Read-SqVersion $root) -eq '0.0.2') 'the install is not 0.0.2'
    $user = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    foreach ($folder in @($parent, $root)) {
        $acl = Get-Acl -LiteralPath $folder
        Assert ($acl.AreAccessRulesProtected) "$folder DACL is not protected"
        $sddl = $acl.Sddl
        foreach ($name in @('AU', 'BU', 'WD')) {
            Assert ($sddl -notmatch ";;;$name\)") "$folder DACL names $name"
        }
        Assert ($sddl -notmatch 'S-1-5-11') "$folder DACL names S-1-5-11"
        Assert ($sddl -match [regex]::Escape(";;;$user)")) "$folder DACL lacks the current user SID"
        Write-Evidence "icacls $folder`n$((icacls.exe $folder | Out-String).Trim())"
    }
    Uninstall-Root $root
    Remove-Item -LiteralPath $parent -Recurse -Force
}

# S16b: a user-made C:\NavoS16b inherits Authenticated Users Modify from C:\ and is refused.
function Test-S16b {
    $parent = 'C:\NavoS16b'
    $root = Join-Path $parent 'Navo'
    if (Test-Path -LiteralPath $parent) { Remove-Item -LiteralPath $parent -Recurse -Force }
    New-Item -ItemType Directory -Path $parent | Out-Null
    $wizard = Start-Wizard '0.0.2'
    Step-ToFolder $wizard 'Install Navo 0\.0\.2'
    Select-Folder $wizard $parent
    Wait-UiText $wizard.Window ([regex]::Escape('Other people on this PC can change a folder above this one.')) 30
    Assert (-not (Test-Path -LiteralPath $root)) "$root exists after the refusal"
    Write-Evidence "parent SDDL: $((Get-Acl -LiteralPath $parent).Sddl)"
    Close-Wizard $wizard
    Remove-Item -LiteralPath $parent -Recurse -Force
}