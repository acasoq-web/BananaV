; BananaV's Windows installer (Inno Setup 6). For one user, no administrator
; needed: it goes to %LOCALAPPDATA%\Programs\BananaV, with a shortcut in the
; Start menu, one on the desktop if wanted, and BananaV's project files opening
; in it. Uninstalling leaves the user's settings and projects where they are.
;
; Built by .github/workflows/package.yml:
;   ISCC /DAppVersion=0.2.0 /DArch=x64 /DFiles=<folder of the zip's files>
;        /O<output folder> /F<file name> packaging\windows\bananav.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef Files
  #error Say where the files are: /DFiles=<folder>
#endif

[Setup]
; The installation's identity: updates find it by this, so it never changes.
AppId={{6DEA8E75-09B5-4CBD-8BDF-E7AA3C85FFD5}
AppName=BananaV
AppVersion={#AppVersion}
AppVerName=BananaV {#AppVersion}
AppPublisher=BananaV
AppPublisherURL=https://bananav-editor.github.io/
AppSupportURL=https://discord.gg/vtC7Yasz98
AppUpdatesURL=https://github.com/acasoq-web/BananaV/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\BananaV
DefaultGroupName=BananaV
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
MinVersion=10.0
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
SetupIconFile=..\icons\bananav.ico
UninstallDisplayIcon={app}\bananav.ico
UninstallDisplayName=BananaV
WizardStyle=modern
WizardImageFile=wizard-large.bmp
WizardSmallImageFile=wizard-small.bmp
Compression=lzma2/max
SolidCompression=yes
ChangesAssociations=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[CustomMessages]
english.OpenProjects=Open BananaV projects (.bvproj) with BananaV
brazilianportuguese.OpenProjects=Abrir projetos do BananaV (.bvproj) com o BananaV

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "projects"; Description: "{cm:OpenProjects}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Files}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs
Source: "..\icons\bananav.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\BananaV"; Filename: "{app}\BananaV.exe"; IconFilename: "{app}\bananav.ico"
Name: "{autodesktop}\BananaV"; Filename: "{app}\BananaV.exe"; IconFilename: "{app}\bananav.ico"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.bvproj"; ValueType: string; ValueName: ""; ValueData: "BananaV.Project"; Flags: uninsdeletevalue; Tasks: projects
Root: HKA; Subkey: "Software\Classes\BananaV.Project"; ValueType: string; ValueName: ""; ValueData: "BananaV Project"; Flags: uninsdeletekey; Tasks: projects
Root: HKA; Subkey: "Software\Classes\BananaV.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\bananav.ico"; Tasks: projects
Root: HKA; Subkey: "Software\Classes\BananaV.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\BananaV.exe"" ""%1"""; Tasks: projects

[Run]
Filename: "{app}\BananaV.exe"; Description: "{cm:LaunchProgram,BananaV}"; Flags: nowait postinstall skipifsilent
