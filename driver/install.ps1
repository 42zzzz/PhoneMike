#Requires -RunAsAdministrator
# install.ps1 - Sign and install PhoneMikeDriver on this machine (test signing)
# Run once from an elevated PowerShell in the driver\ folder.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$DriverDir  = $PSScriptRoot
$SysDebug   = Join-Path $DriverDir 'x64\Debug\PhoneMikeDriver.sys'
$Inf        = Join-Path $DriverDir 'phonemic.inf'
$Pfx        = Join-Path $DriverDir 'PhoneMike_test.pfx'
$PfxPass    = 'PhoneMiketest'
$WdkBin     = 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64'
$WdkTools   = 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64'
$SignTool    = Join-Path $WdkBin 'signtool.exe'
$MakeCat    = Join-Path $WdkBin 'makecat.exe'
$DevCon     = Join-Path $WdkTools 'devcon.exe'

# ---------------------------------------------------------------------------
# 0. Sanity checks
# ---------------------------------------------------------------------------
foreach ($f in @($SysDebug, $Inf, $SignTool)) {
    if (-not (Test-Path $f)) {
        Write-Error "Required file not found: $f"
    }
}

# ---------------------------------------------------------------------------
# 1. Enable test signing (requires reboot - skip if already set)
# ---------------------------------------------------------------------------
$bcdOut = & bcdedit /enum 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error "[1] bcdedit failed (exit $LASTEXITCODE). Run this script from a fully-elevated admin prompt (right-click -> Run as Administrator).`n$bcdOut"
}
$tsOn = ($bcdOut | Select-String 'testsigning\s+Yes')
if (-not $tsOn) {
    Write-Host '[1] Enabling test signing (reboot required after this script)...'
    & bcdedit /set testsigning on
    if ($LASTEXITCODE -ne 0) { Write-Error 'bcdedit /set testsigning on failed' }
    & bcdedit /set nointegritychecks on
    $needReboot = $true
} else {
    Write-Host '[1] Test signing already enabled.'
    $needReboot = $false
}

# ---------------------------------------------------------------------------
# 2. Create / reuse self-signed cert
# ---------------------------------------------------------------------------
Write-Host '[2] Creating self-signed code-signing certificate...'
$certSubject = 'CN=PhoneMikeTestCert'
$existing = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $certSubject }
if ($existing) {
    $cert = $existing | Select-Object -First 1
    Write-Host "    Reusing existing cert: $($cert.Thumbprint)"
} else {
    $cert = New-SelfSignedCertificate `
        -Subject $certSubject `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -Type CodeSigningCert `
        -KeyUsage DigitalSignature `
        -HashAlgorithm SHA256
    Write-Host "    Created cert: $($cert.Thumbprint)"
}

# Trust in Root + TrustedPublisher (both required for kernel driver)
foreach ($storeName in @('Root', 'TrustedPublisher')) {
    $store = [System.Security.Cryptography.X509Certificates.X509Store]::new(
        $storeName, 'LocalMachine')
    $store.Open('ReadWrite')
    if (-not ($store.Certificates | Where-Object { $_.Thumbprint -eq $cert.Thumbprint })) {
        $store.Add($cert)
        Write-Host "    Added to LocalMachine\$storeName"
    }
    $store.Close()
}

# Export PFX for signtool
$pwd = ConvertTo-SecureString $PfxPass -AsPlainText -Force
Export-PfxCertificate -Cert $cert -FilePath $Pfx -Password $pwd -Force | Out-Null

# ---------------------------------------------------------------------------
# 3. Fully tear down existing driver (unload + unlock .sys)
# ---------------------------------------------------------------------------
Write-Host '[3] Tearing down existing driver...'

# 3a. Remove ALL PhoneMike device nodes (forces driver unload)
$existingDevs = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {
    $_.InstanceId -like '*PhoneMikeDriver*' -or $_.FriendlyName -like '*PhoneMike*'
}
if ($existingDevs) {
    # Use devcon to cleanly remove (triggers IRP_MN_REMOVE_DEVICE)
    if (Test-Path $DevCon) {
        Write-Host '    devcon remove...'
        & $DevCon remove 'ROOT\PhoneMikeDriver' 2>&1 | Write-Host
    }
    # Also pnputil remove each instance
    foreach ($d in $existingDevs) {
        Write-Host "    Removing: $($d.InstanceId)"
        pnputil /remove-device $d.InstanceId /subtree 2>&1 | Out-Null
    }
    Start-Sleep -Milliseconds 1000
}

# 3b. Stop and delete service
$existingSvc = Get-Service -Name 'PhoneMikeDriver' -ErrorAction SilentlyContinue
if ($existingSvc) {
    Write-Host '    Stopping service...'
    sc.exe stop PhoneMikeDriver 2>&1 | Out-Null
    Start-Sleep -Milliseconds 500
    Write-Host '    Deleting service...'
    sc.exe delete PhoneMikeDriver 2>&1 | Out-Null
    Start-Sleep -Milliseconds 500
}

# 3b-extra. Tear down legacy service name (pre-rename: PhoneMicDriver)
$legacySvc = Get-Service -Name 'PhoneMicDriver' -ErrorAction SilentlyContinue
if ($legacySvc) {
    Write-Host '    Stopping legacy PhoneMicDriver service...'
    sc.exe stop PhoneMicDriver 2>&1 | Out-Null
    Start-Sleep -Milliseconds 500
    sc.exe delete PhoneMicDriver 2>&1 | Out-Null
    Start-Sleep -Milliseconds 500
}

# 3b-extra2. Delete stale ring.dat so ReadIndex/WriteIndex start clean
$ringDat = 'C:\ProgramData\PhoneMike\ring.dat'
if (Test-Path $ringDat) {
    Write-Host '    Removing stale ring.dat...'
    Remove-Item $ringDat -Force -ErrorAction SilentlyContinue
}

# 3c. Wait for file to unlock, then verify
$Sys32File = "$env:SystemRoot\System32\drivers\PhoneMikeDriver.sys"
if (Test-Path $Sys32File) {
    $unlocked = $false
    for ($retry = 1; $retry -le 10; $retry++) {
        try {
            [IO.File]::Open($Sys32File, 'Open', 'ReadWrite', 'None').Close()
            $unlocked = $true
            break
        } catch {
            Write-Host "    Waiting for .sys unlock... (attempt $retry/10)"
            Start-Sleep -Seconds 1
        }
    }
    if (-not $unlocked) {
        Write-Warning "File still locked. Scheduling replacement on next reboot..."
        # MoveFileEx with MOVEFILE_DELAY_UNTIL_REBOOT via .NET
        Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MoveFileHelper {
    [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
    public static extern bool MoveFileEx(string src, string dst, int flags);
}
"@
        # Flag 5 = MOVEFILE_REPLACE_EXISTING | MOVEFILE_DELAY_UNTIL_REBOOT
        [MoveFileHelper]::MoveFileEx($SysDebug, $Sys32File, 5) | Out-Null
        Write-Host "    Scheduled file replacement on reboot."
        $needReboot = $true
    }
}

# ---------------------------------------------------------------------------
# 4. Sign the .sys + generate catalog
# ---------------------------------------------------------------------------
Write-Host '[4] Signing PhoneMikeDriver.sys...'
& $SignTool sign /fd sha256 /a /ph /f $Pfx /p $PfxPass $SysDebug
if ($LASTEXITCODE -ne 0) { Write-Error 'signtool failed on .sys' }

# Stage .sys next to .inf (pnputil resolves SourceDisksFiles relative to .inf)
Write-Host '    Staging .sys next to .inf...'
Copy-Item $SysDebug (Join-Path $DriverDir 'PhoneMikeDriver.sys') -Force

Write-Host '    Generating catalog (makecat)...'
$Cdf = Join-Path $DriverDir 'PhoneMike.cdf'
$Cat = Join-Path $DriverDir 'PhoneMike.cat'
@"
[CatalogHeader]
Name=PhoneMike.cat
ResultDir=$DriverDir
PublicVersion=0x0000001
EncodingType=0x00010001
CATATTR1=0x10010001:OSAttr:2:6.0

[CatalogFiles]
<HASH>phonemic.inf=$Inf
<HASH>PhoneMikeDriver.sys=$(Join-Path $DriverDir 'PhoneMikeDriver.sys')
"@ | Set-Content $Cdf -Encoding ASCII

& $MakeCat $Cdf
if ($LASTEXITCODE -ne 0) { Write-Error "makecat failed" }

Write-Host '    Signing catalog...'
& $SignTool sign /fd sha256 /a /ph /f $Pfx /p $PfxPass $Cat
if ($LASTEXITCODE -ne 0) { Write-Error 'signtool failed on .cat' }

# ---------------------------------------------------------------------------
# 5. Remove old driver package from DriverStore, then install fresh
# ---------------------------------------------------------------------------
Write-Host '[5a] Removing old PhoneMike driver from DriverStore...'
$oemList = pnputil /enum-drivers 2>&1
$lines = $oemList -split "`n"
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match 'phonemic\.inf') {
        for ($j = $i; $j -ge [Math]::Max(0, $i - 5); $j--) {
            if ($lines[$j] -match '(oem\d+\.inf)') {
                $oemInf = $Matches[1]
                Write-Host "    Deleting DriverStore entry: $oemInf"
                pnputil /delete-driver $oemInf /uninstall /force 2>&1 | Write-Host
                break
            }
        }
    }
}

Write-Host '[5b] Installing driver package (pnputil)...'
pnputil /add-driver $Inf /install
# pnputil returns 0 (success) or 3010 (success, reboot needed)
if ($LASTEXITCODE -notin @(0, 259, 3010)) { Write-Error "pnputil failed: $LASTEXITCODE" }

# ---------------------------------------------------------------------------
# 6. Create device node (we removed all nodes in step 3, so always create)
# ---------------------------------------------------------------------------
if (Test-Path $DevCon) {
    Write-Host '[6] Creating device node (devcon)...'
    & $DevCon install $Inf 'ROOT\PhoneMikeDriver'
    if ($LASTEXITCODE -notin @(0, 1)) { Write-Warning "devcon install returned $LASTEXITCODE" }
} else {
    Write-Warning "devcon not found at $DevCon - use Device Manager -> Add Legacy Hardware to create device node manually."
}

# ---------------------------------------------------------------------------
# 7. Check device PnP status and get problem code
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[7] Checking PnP device status...'
Start-Sleep -Milliseconds 1000
$dev = Get-PnpDevice | Where-Object { $_.FriendlyName -like '*PhoneMike*' -or $_.InstanceId -like '*PhoneMikeDriver*' }
if ($dev) {
    # Handle multiple devices (shouldn't happen, but clean up if it does)
    $devList = @($dev)
    if ($devList.Count -gt 1) {
        Write-Host "    WARNING: Found $($devList.Count) device nodes â€” removing extras..."
        # Keep first, remove rest
        for ($i = 1; $i -lt $devList.Count; $i++) {
            Write-Host "    Removing duplicate: $($devList[$i].InstanceId)"
            pnputil /remove-device $devList[$i].InstanceId /subtree 2>&1 | Out-Null
        }
        $dev = $devList[0]
    }
    Write-Host "    Found: $($dev.FriendlyName) - Status: $($dev.Status) - InstanceId: $($dev.InstanceId)"
    if ($dev.Status -ne 'OK') {
        # Get the problem code (CM_PROB_*)
        $code = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_ProblemCode' -ErrorAction SilentlyContinue).Data
        $status = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName 'DEVPKEY_Device_ProblemStatus' -ErrorAction SilentlyContinue).Data
        Write-Host "    ProblemCode: $code  ProblemStatus: 0x$($status.ToString('X8'))"
        if ($status -eq 0xC000028C) {
            Write-Warning @"

*** KASPERSKY IS BLOCKING THE DRIVER (STATUS_DRIVER_BLOCKED 0xC000028C) ***

Fix: Add PhoneMikeDriver.sys to Kaspersky trusted zone BEFORE running install.ps1:
  Kaspersky -> Settings -> Threats -> Exclusions -> Add
    File: $($SysDebug)
    Also add: $(Join-Path $DriverDir 'x64\Debug\PhoneMikeDriver.sys')
  -or- Temporarily disable Kaspersky Self-Defense + Application Control

Then re-run install.ps1 from this elevated prompt.
"@
        }
    }
} else {
    Write-Warning 'Device node not found. devcon may have failed.'
}

# ---------------------------------------------------------------------------
# 8. Verify audio endpoint
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[8] Checking audio endpoint...'
Write-Host "    Open Sound Settings -> Input devices and look for 'PhoneMike Virtual Microphone'."
Write-Host "    If not listed: Kaspersky blocked PnP load â€” driver did not call AddDevice."
Write-Host "    Service SCM status:"
$svcFinal = Get-Service -Name 'PhoneMikeDriver' -ErrorAction SilentlyContinue
if ($svcFinal) {
    Write-Host "      PhoneMikeDriver service: $($svcFinal.Status)"
} else {
    Write-Host "      PhoneMikeDriver service: not found"
}

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
Write-Host ''
if ($needReboot) {
    Write-Host '*** TEST SIGNING WAS JUST ENABLED. REBOOT REQUIRED. ***'
    Write-Host '    After reboot, re-run this script to complete installation.'
} else {
    Write-Host 'Done. Check Sound Settings -> Recording for "PhoneMike Virtual Microphone".'
    Write-Host 'Then run:  phonemike-client.exe --driver'
}