; PhoneMike Windows Installer â€” InnoSetup 6
; Installs PC client + kernel driver files

#define MyAppName "PhoneMike"
#define MyAppVersion "1.2.1"
#define MyAppPublisher "42zzzz"
#define MyAppURL "https://github.com/42zzzz/PhoneMike"
#define MyAppExeName "PhoneMike.exe"

[Setup]
AppId={{B8F3A1D2-7E4C-4A9B-8D6F-1C2E3F4A5B6C}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
OutputDir=Output
OutputBaseFilename=PhoneMike-v{#MyAppVersion}-windows-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
LicenseFile=..\LICENSE
SetupIconFile=..\assets\icons\windows\logo.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
WizardStyle=modern
CloseApplications=yes
CloseApplicationsFilter=PhoneMike.exe
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
; PC Client
Source: "..\pc-client\target\release\PhoneMike.exe"; DestDir: "{app}"; Flags: ignoreversion

; Driver files
Source: "..\driver\x64\Debug\PhoneMikeDriver.sys"; DestDir: "{app}\driver"; Flags: ignoreversion
Source: "..\driver\phonemic.inf"; DestDir: "{app}\driver"; Flags: ignoreversion
Source: "..\driver\PhoneMike.cat"; DestDir: "{app}\driver"; Flags: ignoreversion
Source: "..\driver\install-user.ps1"; DestDir: "{app}\driver"; Flags: ignoreversion
Source: "devcon.exe"; DestDir: "{app}\driver"; Flags: ignoreversion

[Dirs]
Name: "{commonappdata}\PhoneMike"; Permissions: everyone-full

[Icons]
Name: "{group}\PhoneMike Client"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Install Driver"; Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -File ""{app}\driver\install-user.ps1"""; WorkingDir: "{app}\driver"; IconFilename: "{sys}\shell32.dll"; IconIndex: 77; Comment: "Install/reinstall the PhoneMike virtual microphone driver"
Name: "{group}\Uninstall PhoneMike"; Filename: "{uninstallexe}"
Name: "{commondesktop}\PhoneMike"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"
Name: "installdriver"; Description: "Install virtual microphone driver now (requires test signing enabled)"; GroupDescription: "Driver:"

[Run]
; Delete stale ring.dat before driver install so indices start clean
Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -Command ""Remove-Item 'C:\ProgramData\PhoneMike\ring.dat' -Force -ErrorAction SilentlyContinue"""; StatusMsg: "Cleaning up previous session data..."; Tasks: installdriver; Flags: runhidden waituntilterminated
; Post-install: optionally run driver install script
Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -File ""{app}\driver\install-user.ps1"""; WorkingDir: "{app}\driver"; StatusMsg: "Installing virtual microphone driver..."; Tasks: installdriver; Flags: runhidden waituntilterminated
; Launch app after install
Filename: "{app}\{#MyAppExeName}"; Description: "Launch PhoneMike Client"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Delete ring.dat on uninstall
Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -Command ""Remove-Item 'C:\ProgramData\PhoneMike\ring.dat' -Force -ErrorAction SilentlyContinue"""; RunOnceId: "DeleteRingDat"; Flags: runhidden waituntilterminated
; Remove driver on uninstall
Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -Command ""& {{ $DevCon = 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe'; if (Test-Path $DevCon) {{ & $DevCon remove ROOT\PhoneMikeDriver 2>&1 | Out-Null }}; Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {{ $_.InstanceId -like '*PhoneMikeDriver*' }} | ForEach-Object {{ pnputil /remove-device $_.InstanceId /subtree 2>&1 | Out-Null }}; sc.exe stop PhoneMikeDriver 2>&1 | Out-Null; sc.exe delete PhoneMikeDriver 2>&1 | Out-Null }}"""; RunOnceId: "RemoveDriver"; Flags: runhidden waituntilterminated

[Code]
const
  // Registry key where InnoSetup stores uninstall info for this AppId
  UninstRegKey = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{B8F3A1D2-7E4C-4A9B-8D6F-1C2E3F4A5B6C}_is1';

function GetExistingUninstallString(): String;
var
  s: String;
begin
  s := '';
  if not RegQueryStringValue(HKLM64, UninstRegKey, 'UninstallString', s) then
    RegQueryStringValue(HKLM, UninstRegKey, 'UninstallString', s);
  Result := s;
end;

function GetExistingVersion(): String;
var
  s: String;
begin
  s := '';
  if not RegQueryStringValue(HKLM64, UninstRegKey, 'DisplayVersion', s) then
    RegQueryStringValue(HKLM, UninstRegKey, 'DisplayVersion', s);
  Result := s;
end;

// Returns True if the PhoneMikeDriver SCM service entry still exists.
// A service “marked for deletion” remains in SCM until reboot — installing
// over it causes the new devcon install to silently fail.
function DriverServicePendingDelete(): Boolean;
var
  ResultCode: Integer;
begin
  // sc.exe query exits 0 if service exists (running or stopped), 1060 if not found.
  Exec(ExpandConstant('{sys}\sc.exe'), 'query PhoneMikeDriver', '', SW_HIDE,
       ewWaitUntilTerminated, ResultCode);
  Result := (ResultCode = 0);
end;

// Called before files are copied — silently uninstall old version if present.
// If the old driver service is still pending deletion after uninstall, signal
// InnoSetup to reboot first so the SCM entry is flushed before we reinstall.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  UninstStr: String;
  OldVer: String;
  ExePath: String;
  ResultCode: Integer;
begin
  Result := '';
  UninstStr := GetExistingUninstallString();
  if UninstStr = '' then
    Exit;

  OldVer := GetExistingVersion();
  if OldVer <> '' then
    Log('Found existing PhoneMike v' + OldVer + ' - removing before upgrade.');

  ExePath := RemoveQuotes(UninstStr);

  // Run uninstaller silently; /SUPPRESSMSGBOXES prevents any dialog
  if not Exec(ExePath, '/SILENT /NORESTART /SUPPRESSMSGBOXES', '', SW_HIDE,
              ewWaitUntilTerminated, ResultCode) then
  begin
    Result := 'Failed to remove existing installation (exit ' + IntToStr(ResultCode) + '). Remove manually and retry.';
    Exit;
  end;

  // After uninstall, check if the driver service is still registered in SCM.
  // If yes, it is “marked for deletion” and a reboot is required before the
  // new driver can be installed cleanly via devcon.
  if DriverServicePendingDelete() then
  begin
    Log('PhoneMikeDriver service still pending deletion - requesting reboot before install.');
    NeedsRestart := True;
  end;
end;
