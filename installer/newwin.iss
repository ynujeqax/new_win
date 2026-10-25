; NewWin installer (M5 spec §2). Build with scripts\build-installer.ps1, which passes AppVersion.

#ifndef AppVersion
  #error Pass the version: ISCC /DAppVersion=1.0.0 installer\newwin.iss
#endif

[Setup]
AppId={{8F3C2A71-5B4E-4D2A-9C1E-6A7B3D0E4F12}
AppName=NewWin
AppVersion={#AppVersion}
AppVerName=NewWin {#AppVersion}
AppPublisher=NewWin
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\NewWin
PrivilegesRequired=lowest
DisableDirPage=auto
DisableWelcomePage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
CloseApplications=no
RestartApplications=no
ShowLanguageDialog=no
LanguageDetectionMethod=uilanguage
WizardStyle=modern dark
WizardImageFile=
WizardSmallImageFile=
SetupIconFile=..\assets\newwin.ico
UninstallDisplayIcon={app}\NewWin.exe
UninstallDisplayName=NewWin
OutputDir=..\target\installer
OutputBaseFilename=NewWin-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"

[CustomMessages]
english.CannotCloseNewWin=Could not close NewWin. Close it and run Setup again.
ukrainian.CannotCloseNewWin=Не вдалося закрити NewWin. Закрийте його й запустіть інсталятор ще раз.

[Files]
Source: "..\target\release\NewWin.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\NewWinSettings.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
; The new NewWin.exe, unpacked to {tmp} before anything is copied, closes whatever NewWin runs.
Source: "..\target\release\NewWin.exe"; Flags: dontcopy

[Icons]
Name: "{userprograms}\NewWin Settings"; Filename: "{app}\NewWinSettings.exe"

[Run]
Filename: "{app}\NewWin.exe"; Flags: nowait

[UninstallRun]
Filename: "{app}\NewWin.exe"; Parameters: "--quit"; Flags: runhidden waituntilterminated; RunOnceId: "QuitNewWin"

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\NewWin"
Type: filesandordirs; Name: "{localappdata}\NewWin"

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
begin
  Result := '';
  ExtractTemporaryFile('NewWin.exe');
  if not Exec(ExpandConstant('{tmp}\NewWin.exe'), '--quit', '', SW_HIDE, ewWaitUntilTerminated, Code) or (Code <> 0) then
    Result := CustomMessage('CannotCloseNewWin');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'NewWin');
end;
