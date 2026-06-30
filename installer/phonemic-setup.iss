; PhoneMike Windows Installer — InnoSetup 6
; Installs PC client only (driver replaced with VB-Cable relay)

#define MyAppName "PhoneMike"
#define MyAppVersion "1.4.0"
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

[Dirs]
Name: "{commonappdata}\PhoneMike"; Permissions: everyone-full

[Icons]
Name: "{group}\PhoneMike Client"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall PhoneMike"; Filename: "{uninstallexe}"
Name: "{commondesktop}\PhoneMike"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Run]
; Launch app after install
Filename: "{app}\{#MyAppExeName}"; Description: "Launch PhoneMike Client"; Flags: nowait postinstall skipifsilent

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
end;
