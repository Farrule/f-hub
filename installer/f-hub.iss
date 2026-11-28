#define MyAppName "F/HUB"
#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif
#define MyAppPublisher "Farrule"
#define MyAppExeName "f-hub-tray.exe"
#define MyDaemonExeName "f-hub.exe"

[Setup]
AppId={{8E9B3C4A-7F12-4D5E-9A21-6B4C8D0E1F3A}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
; 管理者権限不要のユーザー単位インストール（%LOCALAPPDATA%\Programs\F-HUB）
PrivilegesRequired=lowest
DefaultDirName={autopf}\F-HUB
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; 出力先とファイル名
OutputDir=..\dist-installer
OutputBaseFilename=F-HUB-Setup-{#MyAppVersion}
SetupIconFile=..\src-backend\assets\f-hub.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Windows 起動時にバックグラウンドで自動起動する"; GroupDescription: "スタートアップ:"; Flags: checkedonce

[Files]
; 実行バイナリ本体
Source: "..\src-backend\target\release\f-hub.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\src-backend\target\release\f-hub-tray.exe"; DestDir: "{app}"; Flags: ignoreversion
; 初回インストール時のみ %APPDATA%\F-HUB\config.toml を生成（既存設定は上書きしない・アンインストール時も保護）
Source: "config.default.toml"; DestDir: "{userappdata}\F-HUB"; DestName: "config.toml"; Flags: onlyifdoesntexist uninsneveruninstall

[Icons]
; スタートメニュー
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
; デスクトップ（選択時）
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon
; Windows スタートアップ（--background でウィンドウを出さずトレイ常駐のみ開始）
Name: "{userstartup}\F-HUB"; Filename: "{app}\{#MyAppExeName}"; Parameters: "--background"; Tasks: autostart

[Run]
; インストール完了時にチェックONで即起動（UIウィンドウが開きトレイ常駐開始）
Filename: "{app}\{#MyAppExeName}"; Description: "{#MyAppName} を今すぐ起動する"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; アンインストール時にログファイルのみ削除（config.toml は残す）
Type: files; Name: "{localappdata}\F-HUB\daemon.log"
Type: dirifempty; Name: "{localappdata}\F-HUB"

[Code]
// 上書きインストール・アンインストール前に稼働中の f-hub.exe を停止してファイルロックを防ぐ
procedure StopRunningDaemon();
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/F /IM {#MyDaemonExeName}', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopRunningDaemon();
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    StopRunningDaemon();
  end;
end;