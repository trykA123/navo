; Inno Setup 7 script of the per-user Sidewise installer (plan P4, NC170-NC176).
; vm-build.ps1's installer stage compiles it after the release build; names and version come from the exe.
#if Ver < EncodeVer(7, 1, 1)
  #error Inno Setup 7.1.1 or newer is required (NC175)
#endif
#define Root AddBackslash(SourcePath) + ".."
#define Exe Root + "\target\release\sidewise.exe"
#if !FileExists(Exe)
  #error sidewise.exe is missing: run the release stage first
#endif
#define AppName GetStringFileInfo(Exe, "ProductName")
#define Slug GetStringFileInfo(Exe, "InternalName")
#define AppVersion GetStringFileInfo(Exe, "ProductVersion")
#if AppName == "" || Slug == "" || AppVersion == ""
  #error sidewise.exe lacks ProductName, InternalName or ProductVersion
#endif

[Setup]
AppId={{77DE4BF2-91BD-459E-B2DA-C9BC1FEB476E}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Sidewise
AppPublisherURL=https://github.com/trykA123/sidewise
AppSupportURL=https://github.com/trykA123/sidewise/issues
AppUpdatesURL=https://github.com/trykA123/sidewise/releases
VersionInfoVersion={#GetVersionNumbersString(Exe)}
VersionInfoProductTextVersion={#AppVersion}
SetupArchitecture=x64
MinVersion=10.0.19045
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DisableDirPage=yes
AlwaysShowDirOnReadyPage=yes
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#Slug}.exe
SetupIconFile={#Root}\assets\brand\sidewise.ico
WizardStyle=modern dynamic
Compression=lzma2/max
SolidCompression=yes
CloseApplications=yes
; The app holds this named mutex (crates/navo-compare-app/src/app_mutex.rs, never renamed), so Setup
; and the uninstaller ask the user to close Sidewise first (IN7, F2). No Global\ prefix: per-user.
AppMutex=Sidewise-7f3c2a91-5d4e-4b8a-9c61-0e2f7a3b8d54
RestartApplications=no
OutputBaseFilename={#Slug}-{#AppVersion}-windows-x64-setup
#ifdef Sign
SignTool=sidewise
SignedUninstaller=yes
#endif

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Exe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\crates\sidewise-kit\fonts\Inter-OFL.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\crates\sidewise-kit\fonts\JetBrainsMono-OFL.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\crates\sidewise-kit\fonts\SpaceGrotesk-OFL.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\assets\icons\material-LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#Slug}.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#Slug}.exe"; Tasks: desktopicon
