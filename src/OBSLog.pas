{
  OBSLog — log centralizado em arquivo (UTF-8) ao inves de stdout.

  Caminho: %LOCALAPPDATA%\NoOBS\logs\NoOBS_yyyy-mm-dd.log — um arquivo por
  dia. Reaberturas no mesmo dia appendam no mesmo arquivo (cada sessao
  delimitada pelo cabecalho SESSION START); retencao dos 3 dias mais recentes
  (PruneOldDailyLogs apaga o excedente no startup).

  Cada linha: HH:MM:SS.zzz<2 espacos>texto.
  Cabecalho de sessao no startup, footer no finalization.

  Uso:
    Log('mensagem simples');
    Log('formatada %d %s', [N, Texto]);
    Log;                    // linha em branco

  Thread-safe via TCriticalSection — embora hoje todas as chamadas
  ocorram na main thread (WebView2 + WindowProc), defensivo pra futuro.
}
unit OBSLog;

interface

procedure Log; overload;
procedure Log(const AMsg: string); overload;
procedure Log(const AFmt: string; const AArgs: array of const); overload;

function LogFilePath: string;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.IOUtils,
  System.Classes,
  System.SyncObjs;

var
  LogStream: TFileStream = nil;
  LogLock: TCriticalSection = nil;
  LogPath: string = '';

procedure WriteLineRaw(const ALine: string);
const
  CRLF: array[0..1] of Byte = (13, 10);
var
  Buf: TBytes;
begin
  if LogStream = nil then Exit;
  Buf := TEncoding.UTF8.GetBytes(ALine);
  if Length(Buf) > 0 then
    LogStream.WriteBuffer(Buf[0], Length(Buf));
  LogStream.WriteBuffer(CRLF[0], 2);
  // FlushFileBuffers forca grava em disco IMEDIATAMENTE. Sem isso o
  // OS cacheia e em caso de crash perdemos as ultimas linhas (que
  // tipicamente sao as mais importantes pra debug). Custo: ~50us
  // por log, aceitavel pra arquivo de log small/infrequente.
  FlushFileBuffers(LogStream.Handle);
end;

procedure DoLog(const AMsg: string);
begin
  if (LogLock = nil) or (LogStream = nil) then Exit;
  LogLock.Enter;
  try
    // Recheck sob o lock: DoneLog pode ter fechado o stream entre o
    // nil-check acima e a entrada no lock (workers que ainda logam
    // durante o teardown — ex.: thread WASAPI vazada no timeout de enum).
    if LogStream = nil then Exit;
    if AMsg = '' then
      WriteLineRaw('')
    else
      WriteLineRaw(FormatDateTime('hh:nn:ss.zzz', Now) + '  ' + AMsg);
  finally
    LogLock.Leave;
  end;
end;

procedure Log;
begin
  DoLog('');
end;

procedure Log(const AMsg: string);
begin
  DoLog(AMsg);
end;

procedure Log(const AFmt: string; const AArgs: array of const);
begin
  try
    DoLog(Format(AFmt, AArgs));
  except
    DoLog('[falha ao formatar log: ' + AFmt + ']');
  end;
end;

function LogFilePath: string;
begin
  Result := LogPath;
end;

// Mantem apenas os KEEP_DAYS arquivos de log mais recentes (um por dia,
// NoOBS_yyyy-mm-dd.log). O nome com data zero-padded ordena
// cronologicamente por string, entao basta ordenar e apagar o excedente
// do inicio da lista. Tambem remove o log legado NoOBS.log da versao
// anterior (um nivel acima, na raiz de NoOBS\), que nao segue o padrao novo.
procedure PruneOldDailyLogs(const ADir: string);
const
  KEEP_DAYS = 3;
var
  Files: TStringList;
  SR: TSearchRec;
  i: Integer;
  LegacyPath: string;
begin
  // Log unico da versao antiga (%LOCALAPPDATA%\NoOBS\NoOBS.log) — ADir e
  // ...\NoOBS\logs, entao subimos um nivel pra achar o arquivo legado.
  LegacyPath := IncludeTrailingPathDelimiter(
    ExtractFileDir(ExcludeTrailingPathDelimiter(ADir))) + 'NoOBS.log';
  if FileExists(LegacyPath) then
    try DeleteFile(LegacyPath); except end;

  Files := TStringList.Create;
  try
    if FindFirst(IncludeTrailingPathDelimiter(ADir) + 'NoOBS_*.log',
      faAnyFile, SR) = 0 then
    try
      repeat
        if (SR.Attr and faDirectory) = 0 then
          Files.Add(SR.Name);
      until FindNext(SR) <> 0;
    finally
      FindClose(SR);
    end;

    // Ordena por nome (= por data, gracas ao yyyy-mm-dd zero-padded) e apaga
    // tudo exceto os KEEP_DAYS mais recentes (que ficam no fim da lista).
    // Se Count <= KEEP_DAYS o limite superior fica negativo e o loop nao roda.
    Files.Sort;
    for i := 0 to Files.Count - 1 - KEEP_DAYS do
      try
        DeleteFile(IncludeTrailingPathDelimiter(ADir) + Files[i]);
      except
      end;
  finally
    Files.Free;
  end;
end;

procedure InitLog;
var
  AppData, Dir: string;
begin
  AppData := GetEnvironmentVariable('LOCALAPPDATA');
  if AppData = '' then AppData := GetEnvironmentVariable('APPDATA');
  if AppData = '' then Exit;

  // Logs em %LOCALAPPDATA%\NoOBS\logs\ — um arquivo por dia
  // (NoOBS_yyyy-mm-dd.log). Subpasta propria facilita o glob da retencao.
  Dir := IncludeTrailingPathDelimiter(AppData) + 'NoOBS' + PathDelim + 'logs';
  try
    ForceDirectories(Dir);
  except
    Exit;
  end;
  LogPath := IncludeTrailingPathDelimiter(Dir) +
    'NoOBS_' + FormatDateTime('yyyy-mm-dd', Now) + '.log';

  LogLock := TCriticalSection.Create;

  // Abre em APPEND: se o arquivo do dia ja existe (reabertura no mesmo dia),
  // continua no fim; senao cria. Assim varias sessoes do mesmo dia convivem
  // no mesmo arquivo em vez de sobrescrever.
  try
    if FileExists(LogPath) then
    begin
      LogStream := TFileStream.Create(LogPath, fmOpenReadWrite or fmShareDenyWrite);
      LogStream.Seek(Int64(0), soEnd);
    end
    else
      LogStream := TFileStream.Create(LogPath, fmCreate or fmShareDenyWrite);
  except
    FreeAndNil(LogStream);
  end;

  if LogStream <> nil then
  begin
    WriteLineRaw('');
    WriteLineRaw('=========================================================');
    WriteLineRaw('=== ' + FormatDateTime('yyyy-mm-dd hh:nn:ss', Now) +
      '  SESSION START ===');
    WriteLineRaw('=========================================================');
  end;

  // Retencao DEPOIS de criar/abrir o arquivo de hoje — assim o dia de hoje
  // conta e ficam no total 3 dias (hoje + 2 anteriores). O arquivo de hoje e o
  // nome lexicograficamente maior, entao nunca entra na lista de exclusao (sem
  // risco de apagar o log aberto). Tambem remove o log legado NoOBS.log.
  try PruneOldDailyLogs(Dir); except end;
end;

procedure DoneLog;
begin
  if LogLock = nil then Exit;
  // Fecha o stream SOB o lock e o nila antes de liberar — assim qualquer
  // worker que ja esteja dentro de DoLog ve LogStream=nil no recheck e sai
  // sem escrever em handle liberado.
  LogLock.Enter;
  try
    if LogStream <> nil then
    begin
      try
        WriteLineRaw('=== ' + FormatDateTime('yyyy-mm-dd hh:nn:ss', Now) +
          '  SESSION END ===');
      except end;
      try LogStream.Free except end;
      LogStream := nil;
    end;
  finally
    LogLock.Leave;
  end;
  // NAO liberamos LogLock de proposito: no shutdown ainda pode haver
  // worker thread viva (pegadinha #30: enum WASAPI vazada no timeout) que
  // chama Log(). Liberar o lock aqui criaria janela de use-after-free no
  // LogLock.Enter desses workers. Vazar uma TCriticalSection ate o
  // ExitProcess e inofensivo — o OS recupera tudo no fim do processo.
end;

// Vectored exception handler — captura crashes nativos (access
// violations em codigo de DLLs externas como WASAPI/COM) que escapam
// dos try/except do Delphi por nao serem EAccessViolation Pascal.
// Quando o processo ia morrer silenciosamente, agora pelo menos
// deixa rastro no log antes da morte.
type
  PExceptionPointers = ^TExceptionPointers;
  TExceptionPointers = record
    ExceptionRecord: Pointer;
    ContextRecord: Pointer;
  end;

  TExceptionRecord = record
    ExceptionCode: DWORD;
    ExceptionFlags: DWORD;
    ExceptionRecord: Pointer;
    ExceptionAddress: Pointer;
  end;
  PExceptionRecord = ^TExceptionRecord;

function VectoredExceptionHandler(ExceptionInfo: PExceptionPointers): LONG; stdcall;
const
  EXCEPTION_CONTINUE_SEARCH       = 0;
  // Codigos inequivocos de crash. Outros (Delphi managed, breakpoints,
  // C++ EH) sao tratados pelos handlers normais — nao logamos pra
  // nao poluir o log com excecoes "normais".
  STATUS_ACCESS_VIOLATION   = DWORD($C0000005);
  STATUS_ILLEGAL_INSTRUCTION= DWORD($C000001D);
  STATUS_PRIV_INSTRUCTION   = DWORD($C0000096);
  STATUS_STACK_OVERFLOW     = DWORD($C00000FD);
  STATUS_INT_DIVIDE_BY_ZERO = DWORD($C0000094);
  STATUS_HEAP_CORRUPTION    = DWORD($C0000374);
  STATUS_INVALID_HANDLE     = DWORD($C0000008);
var
  Rec: PExceptionRecord;
  Code: DWORD;
begin
  Result := EXCEPTION_CONTINUE_SEARCH;
  if ExceptionInfo = nil then Exit;
  Rec := PExceptionRecord(ExceptionInfo.ExceptionRecord);
  if Rec = nil then Exit;
  Code := Rec.ExceptionCode;
  if (Code <> STATUS_ACCESS_VIOLATION) and
     (Code <> STATUS_ILLEGAL_INSTRUCTION) and
     (Code <> STATUS_PRIV_INSTRUCTION) and
     (Code <> STATUS_STACK_OVERFLOW) and
     (Code <> STATUS_INT_DIVIDE_BY_ZERO) and
     (Code <> STATUS_HEAP_CORRUPTION) and
     (Code <> STATUS_INVALID_HANDLE) then Exit;
  try
    Log('NATIVE EXCEPTION: code=$%.8x addr=$%p (first-chance — pode ser pego por try/except)',
      [Code, Rec.ExceptionAddress]);
  except end;
end;

function AddVectoredExceptionHandler(First: ULONG; Handler: Pointer): Pointer;
  stdcall; external 'kernel32.dll';

function SetUnhandledExceptionFilter(lpTopLevelExceptionFilter: Pointer): Pointer;
  stdcall; external 'kernel32.dll';

// GetModuleHandleExW nao esta declarado em todas as versoes do
// Winapi.Windows.pas — declaramos manualmente. Usado pra resolver
// endereco de excecao -> modulo (DLL/EXE) sem incrementar refcount.
function GetModuleHandleExW(dwFlags: DWORD; lpModuleName: PWideChar;
  out phModule: HMODULE): BOOL; stdcall; external 'kernel32.dll';

// Handler de ULTIMA chance — chamado pelo Windows logo antes de matar
// o processo, depois que TODOS os outros handlers (Delphi RTL, SEH,
// VectoredExceptionHandler) ja passaram. Aqui logamos a excecao fatal
// pra ter o ultimo rastro antes do "processo sumiu sem aviso".
// Resolve qual modulo (.dll / .exe) contem o endereco — usado pra dar
// pista do que crashou (ex.: obs.dll, avcodec-61.dll, WebView2 etc.)
// sem precisar de symbols. Retorna "modname.dll+0xNNNN" ou string vazia.
function ResolveAddressToModule(Addr: Pointer): string;
const
  GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS = $00000004;
  GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT = $00000002;
var
  HMod: HMODULE;
  Buf: array[0..MAX_PATH - 1] of WideChar;
  Offset: NativeUInt;
  Path: string;
begin
  Result := '';
  HMod := 0;
  // Pega o HMODULE do modulo que contem esse endereco. Com a flag
  // FROM_ADDRESS, o ponteiro e tratado como endereco dentro do modulo
  // (nao como nome) — cast pra PWideChar e' so pra casar o tipo da
  // assinatura, o valor nao e interpretado como string.
  if not GetModuleHandleExW(
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS or
    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    PWideChar(Addr), HMod) then Exit;
  if HMod = 0 then Exit;
  if GetModuleFileNameW(HMod, @Buf[0], MAX_PATH) = 0 then Exit;
  Path := Buf;
  // So o nome do arquivo (sem path) — mais legivel no log.
  Path := ExtractFileName(Path);
  Offset := NativeUInt(Addr) - NativeUInt(HMod);
  Result := Format('%s+0x%x', [Path, Offset]);
end;

function UnhandledExceptionFilter_NoOBS(ExceptionInfo: PExceptionPointers): LONG; stdcall;
const
  EXCEPTION_EXECUTE_HANDLER = 1;
var
  Rec: PExceptionRecord;
  ModInfo: string;
begin
  // EXECUTE_HANDLER = "permite o processo morrer com codigo dessa
  // excecao". Mesmo se voltassemos CONTINUE_SEARCH, nao tem mais
  // ninguem pra capturar — Windows vai matar de qualquer jeito.
  Result := EXCEPTION_EXECUTE_HANDLER;
  if ExceptionInfo = nil then Exit;
  Rec := PExceptionRecord(ExceptionInfo.ExceptionRecord);
  if Rec = nil then Exit;
  try
    Log('===== FATAL UNHANDLED EXCEPTION =====');
    Log('FATAL: code=$%.8x addr=$%p — processo sera terminado pelo Windows.',
      [Rec.ExceptionCode, Rec.ExceptionAddress]);
    // Resolve endereco -> modulo+offset (sem symbols). Isso da pista
    // de em qual DLL/EXE a excecao aconteceu — obs.dll, avcodec-61.dll,
    // user32.dll, etc.
    try
      ModInfo := ResolveAddressToModule(Rec.ExceptionAddress);
      if ModInfo <> '' then
        Log('FATAL: module=%s', [ModInfo]);
    except end;
    Log('FATAL: TID=%d', [GetCurrentThreadId]);
    // Forca flush — FlushFileBuffers ja roda dentro de WriteLineRaw,
    // mas garantido aqui antes do processo morrer. Snapshot em local
    // pra evitar leitura rasgada se DoneLog nilar LogStream em paralelo.
    var LS: TFileStream := LogStream;
    if LS <> nil then
      try FlushFileBuffers(LS.Handle); except end;
  except end;
end;

initialization
  InitLog;
  // Registra handler nativo — first-chance, primeira posicao.
  AddVectoredExceptionHandler(1, @VectoredExceptionHandler);
  // Handler de ultima chance — captura a excecao fatal antes do
  // Windows matar o processo. Sem isso, em crashes silenciosos
  // (sem dialog de "stopped working"), o log nao mostra a causa.
  SetUnhandledExceptionFilter(@UnhandledExceptionFilter_NoOBS);

finalization
  DoneLog;

end.
