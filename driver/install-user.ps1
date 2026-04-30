#Requires -RunAsAdministrator
# End-user driver installer for PhoneMike.
# Driver is pre-signed — no WDK required on the user's machine.
# Run from an elevated PowerShell in the folder containing phonemic.inf.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$DriverDir = $PSScriptRoot
$Inf    = Join-Path $DriverDir 'phonemic.inf'
$DevCon = Join-Path $DriverDir 'devcon.exe'

foreach ($f in @($Inf)) {
    if (-not (Test-Path $f)) { Write-Error "Required file not found: $f" }
}

# ---------------------------------------------------------------------------
# 1. Enable test signing (required — driver is test-signed, not WHQL)
# ---------------------------------------------------------------------------
$bcdOut = & bcdedit /enum 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error "bcdedit failed. Run this script from a fully-elevated admin prompt."
}
if (-not ($bcdOut | Select-String 'testsigning\s+Yes')) {
    Write-Host '[1] Enabling test signing (reboot required after this step)...'
    & bcdedit /set testsigning on | Out-Null
    & bcdedit /set nointegritychecks on | Out-Null
    Write-Host ''
    Write-Host '*** TEST SIGNING ENABLED. YOU MUST REBOOT NOW. ***'
    Write-Host '    After reboot, run this installer again to complete driver installation.'
    exit 3010
} else {
    Write-Host '[1] Test signing already enabled.'
}

# ---------------------------------------------------------------------------
# 2. Tear down existing driver
# ---------------------------------------------------------------------------
Write-Host '[2] Removing existing driver (if present)...'

if (Test-Path $DevCon) {
    & $DevCon remove 'ROOT\PhoneMikeDriver' 2>&1 | Out-Null
}
Get-PnpDevice -ErrorAction SilentlyContinue |
    Where-Object { $_.InstanceId -like '*PhoneMikeDriver*' } |
    ForEach-Object { pnputil /remove-device $_.InstanceId /subtree 2>&1 | Out-Null }
sc.exe stop   PhoneMikeDriver 2>&1 | Out-Null
sc.exe delete PhoneMikeDriver 2>&1 | Out-Null
Start-Sleep -Milliseconds 500

# ---------------------------------------------------------------------------
# 3. Remove old DriverStore entry
# ---------------------------------------------------------------------------
Write-Host '[3] Cleaning DriverStore...'
$lines = (pnputil /enum-drivers 2>&1) -split "`n"
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match 'phonemic\.inf') {
        for ($j = $i; $j -ge [Math]::Max(0, $i - 5); $j--) {
            if ($lines[$j] -match '(oem\d+\.inf)') {
                Write-Host "    Deleting $($Matches[1]) from DriverStore..."
                pnputil /delete-driver $Matches[1] /uninstall /force 2>&1 | Out-Null
                break
            }
        }
    }
}

# ---------------------------------------------------------------------------
# 4. Install driver package into DriverStore
# ---------------------------------------------------------------------------
Write-Host '[4] Installing driver package...'
pnputil /add-driver $Inf /install
if ($LASTEXITCODE -notin @(0, 259, 3010)) {
    Write-Error "pnputil /add-driver failed: exit $LASTEXITCODE"
}

# ---------------------------------------------------------------------------
# 5. Create device node
# ---------------------------------------------------------------------------
Write-Host '[5] Creating device node...'
if (Test-Path $DevCon) {
    & $DevCon install $Inf 'ROOT\PhoneMikeDriver'
    if ($LASTEXITCODE -notin @(0, 1)) {
        Write-Warning "devcon install returned $LASTEXITCODE"
    }
} else {
    Write-Warning 'devcon.exe not found — skipping device node creation.'
    Write-Warning 'The driver package was staged but no virtual microphone device was created.'
}

# ---------------------------------------------------------------------------
# 6. Verify
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[6] Checking device status...'
Start-Sleep -Milliseconds 1000
$dev = Get-PnpDevice -ErrorAction SilentlyContinue |
    Where-Object { $_.FriendlyName -like '*PhoneMike*' -or $_.InstanceId -like '*PhoneMikeDriver*' }
if ($dev) {
    Write-Host "    Found: $($dev.FriendlyName) - Status: $($dev.Status)"
    if ($dev.Status -eq 'OK') {
        Write-Host ''
        Write-Host 'SUCCESS. PhoneMike Virtual Microphone is ready.'
        Write-Host 'Open Sound Settings and check the Input devices list.'
    } else {
        $code = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId `
            -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction SilentlyContinue).Data
        Write-Warning "Device has a problem (code $code). Try rebooting and running this script again."
    }
} else {
    Write-Warning 'Device node not found. Try rebooting and running this script again.'
}
