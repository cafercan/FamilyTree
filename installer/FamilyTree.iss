; FamilyTree - Inno Setup kurulum betiği (C Launcher uyumlu)
; Kullanıcı başına kurulur (yönetici izni istemez), sessiz kipte (/VERYSILENT) çalışır,
; açık uygulamayı güvenle kapatır. AppId hiçbir sürümde değişmez: güncellemeler aynı yere kurulur.
;
; Derleme: scripts\build-installer.ps1 (sürümü tauri.conf.json'dan, exe'yi cargo'nun hedef
; klasöründen verir):
;   ISCC /DAppVersion=<sürüm> /DTargetDir=<cargo target_directory> installer\FamilyTree.iss

#ifndef AppVersion
  #error AppVersion verilmedi (/DAppVersion=<sürüm>) - scripts\build-installer.ps1 kullanın
#endif
#ifndef TargetDir
  #error TargetDir verilmedi (/DTargetDir=<cargo hedef klasörü>) - scripts\build-installer.ps1 kullanın
#endif

#define AppName "FamilyTree"
#define AppPublisher "cafercan"
#define AppExeName "FamilyTree.exe"
#define AppUrl "https://github.com/cafercan/FamilyTree"

[Setup]
AppId={{3971512A-372B-421E-9C98-B814BE459B7B}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}
DefaultDirName={autopf}\FamilyTree
DefaultGroupName=FamilyTree
DisableProgramGroupPage=yes
DisableDirPage=no
AllowNoIcons=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
CloseApplications=yes
RestartApplications=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=FamilyTree-{#AppVersion}-windows-x64-setup
SetupIconFile=..\src-tauri\icons\icon.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName=FamilyTree
WizardStyle=modern
Compression=lzma2
SolidCompression=yes

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; Tek exe (arayüz dosyaları içine gömülü)
Source: "{#TargetDir}\release\familytree.exe"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
Source: "..\src-tauri\icons\icon.ico"; DestDir: "{app}"; DestName: "familytree.ico"; Flags: ignoreversion

[Icons]
Name: "{group}\FamilyTree"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\familytree.ico"
Name: "{group}\{cm:UninstallProgram,FamilyTree}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\FamilyTree"; Filename: "{app}\{#AppExeName}"; IconFilename: "{app}\familytree.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,FamilyTree}"; Flags: nowait postinstall skipifsilent

; Kaldırma uygulama dosyalarını siler; kullanıcının soy ağaçları (%APPDATA%\FamilyTree) korunur.

[Code]
const
  WV2_GUID = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

// WebView2 Runtime kontrolü (Evergreen). 64-bit kipte sade HKLM 64-bit görünüme gider;
// WebView2 pv değerini 32-bit görünüme yazar, bu yüzden HKLM32 açıkça sorulur.
function WebView2Version(RootKey: Integer): String;
var
  pv: String;
begin
  Result := '';
  if RegQueryStringValue(RootKey, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\' + WV2_GUID, 'pv', pv) then
    if (pv <> '') and (pv <> '0.0.0.0') then
      Result := pv;
end;

function IsWebView2Installed(): Boolean;
var
  found: String;
begin
  found := WebView2Version(HKLM32);
  if found = '' then
    found := WebView2Version(HKCU);
  if (found = '') and IsWin64 then
    found := WebView2Version(HKLM64);
  Result := found <> '';
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  // Sessiz kipte (C Launcher) soru sorulmaz, kuruluma devam edilir
  if not IsWebView2Installed() then
    if SuppressibleMsgBox('FamilyTree için Microsoft Edge WebView2 Runtime gerekli ve şu an bulunamadı.'#13#10#13#10 +
              'https://developer.microsoft.com/microsoft-edge/webview2/ adresinden ücretsiz kurabilirsiniz.'#13#10#13#10 +
              'Yine de kuruluma devam edilsin mi?', mbConfirmation, MB_YESNO, IDYES) = IDNO then
      Result := False;
end;
