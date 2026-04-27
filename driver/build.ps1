# build.ps1 — Build PhoneMike WDM driver
# Run after install_build_env.ps1 (admin, once)

$msbuild = 'C:\Program Files\Microsoft Visual Studio\18\Community\MSBuild\Current\Bin\MSBuild.exe'
$proj    = Join-Path $PSScriptRoot 'phonemic_driver.vcxproj'

& $msbuild $proj /p:Configuration=Debug /p:Platform=x64 /t:Build /verbosity:normal