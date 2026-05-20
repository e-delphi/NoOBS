// Eduardo/Claude - 07/05/2026
{
  NoOBS — Gravador de tela com OBS embarcado via libobs.
  Aplicativo Delphi com interface web (ui.html via WebView2) que grava
  a tela usando libobs diretamente (sem processo separado, sem websocket).
  Composicao:
    OBSUI          — host WebView2 (janela, mensagens com JS, tema).
    OBSBridge      — ponte UI <-> motor de gravacao (dispatch, push de estado).
    LibOBS         — bindings Delphi para obs.dll (tipos, funcoes cdecl).
    OBSEngine      — motor de gravacao (init, scene, encoders, output).
    OBSEncoder     — selecao de codec de video (AV1/HEVC/H264/x264).
    OBSAudioTracks — atribuicao de faixas de audio + enum de devices.
    NoOBSTypes     — tipos compartilhados (TEncoderCaps, etc).
    FFmpegOps      — wrappers altos sobre libav* (Remux, ExtractFrame, ...).
    FFmpegLib      — bindings raw das DLLs libav*.
    OBSScene       — tipos de monitor/audio e calculo de canvas.
}
program NoOBS;

{$APPTYPE GUI}

{$R *.res}

{$R *.dres}

uses
  NoOBSTypes in 'src\NoOBSTypes.pas',
  FFmpegLib in 'src\FFmpegLib.pas',
  FFmpegOps in 'src\FFmpegOps.pas',
  LibOBS in 'src\LibOBS.pas',
  OBSEngine in 'src\OBSEngine.pas',
  OBSAudioTracks in 'src\OBSAudioTracks.pas',
  OBSAudioWatch in 'src\OBSAudioWatch.pas',
  OBSAutostart in 'src\OBSAutostart.pas',
  OBSBridge in 'src\OBSBridge.pas',
  OBSConfig in 'src\OBSConfig.pas',
  OBSEncoder in 'src\OBSEncoder.pas',
  OBSHotkey in 'src\OBSHotkey.pas',
  OBSLog in 'src\OBSLog.pas',
  OBSPlayer in 'src\OBSPlayer.pas',
  OBSProbe in 'src\OBSProbe.pas',
  OBSRecordWatch in 'src\OBSRecordWatch.pas',
  OBSScene in 'src\OBSScene.pas',
  OBSStartupCheck in 'src\OBSStartupCheck.pas',
  OBSTray in 'src\OBSTray.pas',
  OBSUI in 'src\OBSUI.pas',
  WinAudioMeter in 'src\WinAudioMeter.pas',
  WinPreview in 'src\WinPreview.pas',
  WinWebcam in 'src\WinWebcam.pas';

begin
  OBSUI.Run;
end.
