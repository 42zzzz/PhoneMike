$msbuild = 'C:\Program Files\Microsoft Visual Studio\18\Community\MSBuild\Current\Bin\MSBuild.exe'
$proj    = Join-Path $PSScriptRoot 'PhoneMike_driver.vcxproj'

# Step 1: compile only
Write-Host "=== Compiling ==="
& $msbuild $proj /p:Configuration=Debug /p:Platform=x64 /t:ClCompile /verbosity:minimal
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Step 2: link manually
Write-Host ""
Write-Host "=== Linking ==="
$wdk   = 'C:\Program Files (x86)\Windows Kits\10'
$vc    = 'C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.50.35717'
$link  = "$vc\bin\HostX64\x64\link.exe"
$objs  = Get-ChildItem (Join-Path $PSScriptRoot 'x64\Debug\obj') -Filter '*.obj' | ForEach-Object { $_.FullName }
$out   = Join-Path $PSScriptRoot 'x64\Debug\PhoneMikeDriver.sys'
$pdb   = Join-Path $PSScriptRoot 'x64\Debug\PhoneMikeDriver.pdb'
$libs  = @(
    "$wdk\Lib\10.0.26100.0\km\x64\portcls.lib",
    "$wdk\Lib\10.0.26100.0\km\x64\ks.lib",
    "$wdk\Lib\10.0.26100.0\km\x64\ksguid.lib",
    "$wdk\Lib\10.0.26100.0\km\x64\stdunk.lib",
    "$wdk\Lib\10.0.26100.0\km\x64\wdmguid.lib",
    "$wdk\Lib\10.0.26100.0\km\x64\ntoskrnl.lib",
    "$wdk\Lib\10.0.26100.0\km\x64\hal.lib",
    "$vc\lib\x64\libcmt.lib",
    "$vc\lib\x64\libvcruntime.lib"
)

& $link $objs $libs `
    /OUT:$out `
    /PDB:$pdb `
    /DEBUG `
    /DRIVER:WDM `
    /SUBSYSTEM:NATIVE `
    /ENTRY:DriverEntry `
    /NODEFAULTLIB `
    /MACHINE:X64 `
    /MERGE:_TEXT=.text `
    /MERGE:_PAGE=PAGE `
    /SECTION:INIT,d `
    /ALIGN:0x1000 `
    /RELEASE `
    /VERSION:10.0

Write-Host ""
if (Test-Path $out) {
    Write-Host "SUCCESS: $out ($([int](Get-Item $out).Length / 1024) KB)"
} else {
    Write-Host "FAILED: .sys not produced"
    exit 1
}