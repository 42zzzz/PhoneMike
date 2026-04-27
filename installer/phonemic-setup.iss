; PhoneMike Windows Installer â€” InnoSetup 6
; Installs PC client + kernel driver files

#define MyAppName "PhoneMike"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "PhoneMike"
#define MyAppURL "https://github.com/42zzzz/PhoneMike"
#define MyAppExeName "phonemike-client.exe"

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
WizardStyle=modern
CloseApplications=yes
CloseApplicationsFilter=phonemike-client.exe
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
; PC Client
Source: "..\pc-client\target\release\phonemike-client.exe"; DestDir: "{app}"; Flags: ignoreversion

; Driver files
Source: "..\driver\x64\Debug\PhoneMikeDriver.sys"; DestDir: "{app}\driver"; Flags: ignoreversion
Source: "..\driver\phonemic.inf"; DestDir: "{app}\driver"; Flags: ignoreversion
Source: "..\driver\install.ps1"; DestDir: "{app}\driver"; Flags: ignoreversion

[Dirs]
Name: "{commonappdata}\PhoneMike"; Permissions: everyone-full

[Icons]
Name: "{group}\PhoneMike Client"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Install Driver"; Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -File ""{app}\driver\install.ps1"""; WorkingDir: "{app}\driver"; IconFilename: "{sys}\shell32.dll"; IconIndex: 77; Comment: "Install/reinstall the PhoneMike virtual microphone driver"
Name: "{group}\Uninstall PhoneMike"; Filename: "{uninstallexe}"
Name: "{commondesktop}\PhoneMike"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"
Name: "installdriver"; Description: "Install virtual microphone driver now (requires test signing enabled)"; GroupDescription: "Driver:"

[Run]
; Post-install: optionally run driver install script
Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -File ""{app}\driver\install.ps1"""; WorkingDir: "{app}\driver"; StatusMsg: "Installing virtual microphone driver..."; Tasks: installdriver; Flags: runhidden waituntilterminated
; Launch app after install
Filename: "{app}\{#MyAppExeName}"; Description: "Launch PhoneMike Client"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Remove driver on uninstall
Filename: "powershell.exe"; Parameters: "-ExecutionPolicy Bypass -Command ""& {{ $DevCon = 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe'; if (Test-Path $DevCon) {{ & $DevCon remove ROOT\PhoneMikeDriver 2>&1 | Out-Null }}; Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {{ $_.InstanceId -like '*PhoneMikeDriver*' }} | ForEach-Object {{ pnputil /remove-device $_.InstanceId /subtree 2>&1 | Out-Null }}; sc.exe stop PhoneMikeDriver 2>&1 | Out-Null; sc.exe delete PhoneMikeDriver 2>&1 | Out-Null }}"""; Flags: runhidden waituntilterminated

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

// Called before files are copied â€” silently uninstall old version if present
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
    Log('Found existing PhoneMike v' + OldVer + ' â€” removing before upgrade.');

  ExePath := RemoveQuotes(UninstStr);

  // Run uninstaller silently; /SUPPRESSMSGBOXES prevents any dialog
  if not Exec(ExePath, '/SILENT /NORESTART /SUPPRESSMSGBOXES', '', SW_HIDE,
              ewWaitUntilTerminated, ResultCode) then
  begin
    Result := 'Failed to remove existing installation (exit ' + IntToStr(ResultCode) + '). Remove manually and retry.';
  end;
end;
