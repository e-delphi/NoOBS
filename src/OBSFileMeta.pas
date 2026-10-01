(*
  OBSFileMeta - dados do NoOBS GRAVADOS NO PROPRIO ARQUIVO DE VIDEO.

  O cache (%LOCALAPPDATA%\NoOBS\cache\<hash>.json/.txt/...) continua sendo a copia de
  trabalho: rapido, local, nunca puxa da nuvem. O que nao da pra refazer
  — layout dos monitores, transcricao, nomes de falante, como foi gravado —
  ganha uma copia DENTRO do arquivo, que viaja com ele pra outra maquina.

  Onde: um bloco acrescentado ao FIM do arquivo, sem tocar em nenhum byte
  do video. Disfarcado de elemento que o formato manda ignorar, entao o
  arquivo continua VALIDO:
    MKV: elemento EBML Void (ID $EC, tamanho em vint de 8 bytes)
    MP4: caixa 'free' (tamanho u32 big-endian + 'free')
  Conteudo (igual nos dois):
    'NOOBSMETA' + zlib(JSON) + rodape de 24 bytes, SEMPRE no fim do arquivo:
    'NOOBSEND' | versao u16 | reservado u16 | tamanho do envelope u64 |
    crc32 do zlib u32   (little-endian)
  Ler = seek pros ultimos 24 bytes. O video nunca e lido.

  Validado num prototipo com as DLLs do app (pacotes, bytes, duracao, seek,
  remux pro MP4 do player e o <video> do Chromium identicos com e sem o
  bloco), com UMA excecao: MKV interrompido no MEIO de um cluster. O
  cluster declara um tamanho que passa do fim do arquivo e o leitor engole
  o comeco do bloco como pacote de video. Por isso so escrevemos quando a
  estrutura termina num limite de elemento (MkvDataEnd/Mp4DataEnd).

  Sincronia cache <-> arquivo, por uma marca de revisao (UTC ISO):
    - MarkChanged: algo mudou no cache (gravacao nova, transcricao, nome de
      falante) -> carimba 'tailRev' no <hash>.json e agenda a escrita.
    - ImportIfNewer: na varredura de metadados (EnsureRecordingMeta), le o
      rodape. Bloco mais novo que o cache (arquivo veio de outra maquina)
      -> popula o cache. Cache mais novo -> agenda a escrita.
      'tailSig' (tamanho + data do arquivo) evita reler o rodape de quem
      nao mudou.
  A escrita roda numa worker, com lista de pendentes PERSISTIDA: arquivo
  aberto por outro processo, so na nuvem ou com erro fica pra depois.
*)
unit OBSFileMeta;

interface

uses
  System.SysUtils;

type
  TFileMetaImported = reference to procedure(const APath: string);

// Sobe a worker e restaura os pendentes da sessao anterior.
procedure Start;
// Para a worker (o que estava pendente fica no arquivo da lista).
procedure Shutdown;

// O cache de APath mudou: carimba a revisao e agenda a escrita no arquivo.
procedure MarkChanged(const APath: string);

// Worker thread. Le o bloco do fim do arquivo e, se for mais novo que o
// cache, popula o cache (meta + transcricao). True se importou.
function ImportIfNewer(const APath: string): Boolean;

// Arquivo (ou pasta, por prefixo) renomeado/movido: os pendentes seguem.
procedure RenamePath(const AOld, ANew: string);

// Chamado (na worker) quando um arquivo teve o cache populado pelo bloco.
procedure SetOnImported(ACallback: TFileMetaImported);

// Baixo nivel (expostos pra teste).
function ReadTailBlock(const APath: string; out AJson: string): Boolean;

implementation

uses
  Winapi.Windows,
  System.Classes,
  System.IOUtils,
  System.JSON,
  System.ZLib,
  System.SyncObjs,
  System.DateUtils,
  System.Generics.Collections,
  OBSLog,
  OBSPlayer,
  OBSTranscribe;

const
  MAGIC_HEAD: array[0..8] of AnsiChar = 'NOOBSMETA';
  MAGIC_FOOT: array[0..7] of AnsiChar = 'NOOBSEND';
  FOOT_SIZE = 24;
  BLOCK_VERSION = 1;

  // Chaves do <hash>.json que viajam. waveform/waveformHi/videoInfo ficam de
  // fora: sao refeitas sozinhas na outra maquina e pesam MB.
  PORTABLE_KEYS: array[0..7] of string =
    ('duration', 'canvas', 'monitors', 'codec', 'codecHw', 'fps', 'quality',
     'speakers');
  KEY_REV = 'tailRev';
  KEY_SIG = 'tailSig';

  PENDING_FILE = 'filemeta-pending.json';
  // Debounce: transcricao + nomes costumam chegar em rajada.
  DELAY_CHANGE_MS = 1500;
  RETRY_LOCKED_MS = 20_000;
  RETRY_CLOUD_MS  = 5 * 60_000;
  RETRY_ERROR_MS  = 60_000;

  ERROR_SHARING_VIOLATION_ = 32;
  ERROR_LOCK_VIOLATION_    = 33;

type
  TWriteResult = (wrOk, wrNothing, wrLocked, wrCloud, wrUnsafe, wrGone, wrError);

  TFileMetaThread = class(TThread)
  protected
    procedure Execute; override;
  end;

// kernel32 com Int64 direto (o Winapi.Windows declara LARGE_INTEGER).
function FM_SetFilePointerEx(hFile: THandle; liDistanceToMove: Int64;
  lpNewFilePointer: PInt64; dwMoveMethod: DWORD): BOOL; stdcall;
  external kernel32 name 'SetFilePointerEx';
function FM_GetFileSizeEx(hFile: THandle; out lpFileSize: Int64): BOOL; stdcall;
  external kernel32 name 'GetFileSizeEx';

var
  GLock: TCriticalSection = nil;
  GDue: TDictionary<string, UInt64> = nil;     // chave minuscula -> tick
  GPaths: TDictionary<string, string> = nil;   // chave minuscula -> caminho
  GDirty: Boolean = False;
  GWake: TEvent = nil;
  GWorker: TFileMetaThread = nil;
  GOnImported: TFileMetaImported;
  CrcTable: array[0..255] of Cardinal;
  CrcReady: Boolean = False;

// ---------------------------------------------------------------- util

procedure InitCrc;
var
  i, k: Integer;
  C: Cardinal;
begin
  for i := 0 to 255 do
  begin
    C := Cardinal(i);
    for k := 0 to 7 do
      if (C and 1) <> 0 then C := $EDB88320 xor (C shr 1) else C := C shr 1;
    CrcTable[i] := C;
  end;
  CrcReady := True;
end;

function BlockCrc32(const AData: TBytes; AStart, ALen: Integer): Cardinal;
var
  i: Integer;
begin
  if not CrcReady then InitCrc;
  Result := $FFFFFFFF;
  for i := AStart to AStart + ALen - 1 do
    Result := CrcTable[(Result xor AData[i]) and $FF] xor (Result shr 8);
  Result := Result xor $FFFFFFFF;
end;

function NowRev: string;
begin
  Result := FormatDateTime('yyyy"-"mm"-"dd"T"hh":"nn":"ss"."zzz"Z"',
    TTimeZone.Local.ToUniversalTime(Now));
end;

function KindOf(const APath: string): string;
var
  Ext: string;
begin
  Ext := LowerCase(ExtractFileExt(APath));
  if Ext = '.mkv' then Result := 'mkv'
  else if Ext = '.mp4' then Result := 'mp4'
  else Result := '';   // .mp3 e o resto: sem bloco "ignore isto" confiavel
end;

function IsCloudOnly(const APath: string): Boolean;
// Mesma regra do OBSBridge.IsFileCloudOnly: atributo e metadado local, nao
// dispara o download. Ler (ou escrever) o fim do arquivo dispararia.
const
  FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS = $00400000;
var
  Attrs: DWORD;
begin
  Attrs := GetFileAttributesW(PWideChar(APath));
  Result := (Attrs <> INVALID_FILE_ATTRIBUTES) and
    ((Attrs and FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) <> 0);
end;

function FileSig(const APath: string): string;
// Tamanho + data de modificacao: muda quando o arquivo foi trocado ou
// copiado por cima. Igual ao guardado = nem precisa ler o rodape.
var
  D: TWin32FileAttributeData;
begin
  Result := '';
  if not GetFileAttributesExW(PWideChar(APath), GetFileExInfoStandard, @D) then Exit;
  Result := Format('%d:%d', [
    (Int64(D.nFileSizeHigh) shl 32) or D.nFileSizeLow,
    (Int64(D.ftLastWriteTime.dwHighDateTime) shl 32) or D.ftLastWriteTime.dwLowDateTime]);
end;

function ReadAt(H: THandle; APos: Int64; ACount: Integer): TBytes;
var
  Got: DWORD;
begin
  SetLength(Result, 0);
  if (ACount <= 0) or (APos < 0) then Exit;
  if not FM_SetFilePointerEx(H, APos, nil, FILE_BEGIN) then Exit;
  SetLength(Result, ACount);
  Got := 0;
  if not ReadFile(H, Result[0], ACount, Got, nil) then Got := 0;
  SetLength(Result, Got);
end;

function BE(const B: TBytes; AOff, ALen: Integer): Int64;
var
  i: Integer;
begin
  Result := 0;
  for i := AOff to AOff + ALen - 1 do Result := (Result shl 8) or B[i];
end;

function LE(const B: TBytes; AOff, ALen: Integer): Int64;
var
  i: Integer;
begin
  Result := 0;
  for i := AOff + ALen - 1 downto AOff do Result := (Result shl 8) or B[i];
end;

// ---------------------------------------------------------------- rodape

function ReadFooter(H: THandle; ASize: Int64; out ATotal: Int64;
  out ACrc: Cardinal): Boolean;
var
  F: TBytes;
  i: Integer;
begin
  Result := False;
  ATotal := 0;
  ACrc := 0;
  if ASize < FOOT_SIZE then Exit;
  F := ReadAt(H, ASize - FOOT_SIZE, FOOT_SIZE);
  if Length(F) <> FOOT_SIZE then Exit;
  for i := 0 to 7 do
    if F[i] <> Byte(MAGIC_FOOT[i]) then Exit;
  if LE(F, 8, 2) <> BLOCK_VERSION then Exit;
  ATotal := LE(F, 12, 8);
  ACrc := Cardinal(LE(F, 20, 4));
  Result := (ATotal >= FOOT_SIZE + Length(MAGIC_HEAD)) and (ATotal <= ASize);
end;

function ReadTailBlockH(H: THandle; ASize: Int64; out AJson: string;
  out AEnvLen: Int64): Boolean;
var
  Total: Int64;
  Crc: Cardinal;
  Env, Z, Raw: TBytes;
  i, k, ZStart, ZLen: Integer;
  Hit: Boolean;
begin
  Result := False;
  AJson := '';
  AEnvLen := 0;
  if not ReadFooter(H, ASize, Total, Crc) then Exit;
  if Total > 64 * 1024 * 1024 then Exit;   // sanidade
  Env := ReadAt(H, ASize - Total, Integer(Total));
  if Length(Env) <> Total then Exit;
  // Envelope: 8 (mp4) ou 9 (mkv) bytes antes da assinatura.
  ZStart := -1;
  for i := 0 to 16 do
  begin
    Hit := True;
    for k := 0 to High(MAGIC_HEAD) do
      if Env[i + k] <> Byte(MAGIC_HEAD[k]) then begin Hit := False; Break; end;
    if Hit then begin ZStart := i + Length(MAGIC_HEAD); Break; end;
  end;
  if ZStart < 0 then Exit;
  ZLen := Length(Env) - FOOT_SIZE - ZStart;
  if ZLen <= 0 then Exit;
  if BlockCrc32(Env, ZStart, ZLen) <> Crc then Exit;
  Z := Copy(Env, ZStart, ZLen);
  try
    ZDecompress(Z, Raw);
  except
    Exit;
  end;
  AJson := TEncoding.UTF8.GetString(Raw);
  AEnvLen := Total;
  Result := True;
end;

function OpenRead(const APath: string): THandle;
begin
  Result := CreateFileW(PWideChar(APath), GENERIC_READ,
    FILE_SHARE_READ or FILE_SHARE_WRITE or FILE_SHARE_DELETE, nil,
    OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, 0);
end;

function ReadTailBlock(const APath: string; out AJson: string): Boolean;
var
  H: THandle;
  Size, EnvLen: Int64;
begin
  Result := False;
  AJson := '';
  H := OpenRead(APath);
  if H = INVALID_HANDLE_VALUE then Exit;
  try
    if not FM_GetFileSizeEx(H, Size) then Exit;
    Result := ReadTailBlockH(H, Size, AJson, EnvLen);
  finally
    CloseHandle(H);
  end;
end;

// ---------------------------------------------------------------- estrutura

function ParseEbml(const B: TBytes; var P: Integer; out AId: Cardinal;
  out ASize: Int64; out AUnknown: Boolean): Boolean;
// Le ID + tamanho (vint) de B a partir de P. False se nao couber/invalido.
var
  Len, i: Integer;
  M, First: Byte;
begin
  Result := False;
  AUnknown := False;
  if P >= Length(B) then Exit;
  First := B[P]; Len := 1; M := $80;
  while (Len <= 4) and ((First and M) = 0) do begin Inc(Len); M := M shr 1; end;
  if (Len > 4) or (P + Len > Length(B)) then Exit;
  AId := Cardinal(BE(B, P, Len));
  Inc(P, Len);
  if P >= Length(B) then Exit;
  First := B[P]; Len := 1; M := $80;
  while (Len <= 8) and ((First and M) = 0) do begin Inc(Len); M := M shr 1; end;
  if (Len > 8) or (P + Len > Length(B)) then Exit;
  ASize := First and (M - 1);
  for i := 1 to Len - 1 do ASize := (ASize shl 8) or B[P + i];
  // Todos os bits de valor em 1 = tamanho desconhecido.
  AUnknown := ASize = (Int64(1) shl (7 * Len)) - 1;
  Inc(P, Len);
  Result := True;
end;

function MkvDataEnd(H: THandle; AEnd: Int64): Int64;
// Onde o video termina (= onde o bloco entra), ou -1 se escrever ali
// cairia dentro de um elemento incompleto. AEnd = fim do arquivo SEM o
// nosso bloco anterior.
var
  B: TBytes;
  P: Integer;
  Id: Cardinal;
  Sz, Pos, Start, Next: Int64;
  Unk: Boolean;
begin
  Result := -1;
  B := ReadAt(H, 0, 64);
  P := 0;
  if not ParseEbml(B, P, Id, Sz, Unk) or (Id <> $1A45DFA3) or Unk then Exit;
  Pos := P + Sz;
  B := ReadAt(H, Pos, 16);
  P := 0;
  if not ParseEbml(B, P, Id, Sz, Unk) or (Id <> $18538067) then Exit;
  Pos := Pos + P;
  if not Unk then
  begin
    // Segmento fechado (trailer escrito): o que vier depois dele e nosso
    // (ou sobra de uma escrita interrompida) e sai junto.
    if Pos + Sz <= AEnd then Result := Pos + Sz;
    Exit;
  end;
  // Tamanho desconhecido (gravacao interrompida): anda pelos filhos, um
  // cabecalho por elemento (um cluster a cada poucos segundos de video).
  while Pos < AEnd do
  begin
    Start := Pos;
    B := ReadAt(H, Pos, 12);
    P := 0;
    if not ParseEbml(B, P, Id, Sz, Unk) or Unk then Exit;
    Next := Pos + P + Sz;
    if Next > AEnd then
    begin
      // Passa do fim: so serve se for um Void (sobra nossa); um cluster
      // aberto e a excecao que o prototipo mediu.
      if Id = $EC then Result := Start;
      Exit;
    end;
    Pos := Next;
  end;
  Result := Pos;
end;

function Mp4DataEnd(H: THandle; AEnd: Int64): Int64;
var
  B: TBytes;
  Pos, Sz, Hdr, Next: Int64;
  Typ: AnsiString;
begin
  Result := -1;
  Pos := 0;
  while Pos < AEnd do
  begin
    B := ReadAt(H, Pos, 16);
    if Length(B) < 8 then Exit;
    Sz := BE(B, 0, 4);
    SetString(Typ, PAnsiChar(@B[4]), 4);
    Hdr := 8;
    if Sz = 1 then
    begin
      if Length(B) < 16 then Exit;
      Sz := BE(B, 8, 8);
      Hdr := 16;
    end;
    if (Sz = 0) or (Sz < Hdr) then Exit;   // 0 = "ate o fim do arquivo"
    Next := Pos + Sz;
    if Next > AEnd then
    begin
      if (Typ = 'free') or (Typ = 'skip') then Result := Pos;
      Exit;
    end;
    Pos := Next;
  end;
  Result := Pos;
end;

// ---------------------------------------------------------------- escrita

function BuildEnvelope(const AJson, AKind: string): TBytes;
var
  Z, Body, Head, Foot: TBytes;
  N, Total: Int64;
  Crc: Cardinal;
  i: Integer;
begin
  ZCompress(TEncoding.UTF8.GetBytes(AJson), Z, zcMax);
  SetLength(Body, Length(MAGIC_HEAD) + Length(Z));
  for i := 0 to High(MAGIC_HEAD) do Body[i] := Byte(MAGIC_HEAD[i]);
  if Length(Z) > 0 then Move(Z[0], Body[Length(MAGIC_HEAD)], Length(Z));
  N := Length(Body) + FOOT_SIZE;
  if AKind = 'mkv' then
  begin
    // ID Void + tamanho em vint de 8 bytes (0x01 + 7 bytes).
    SetLength(Head, 9);
    Head[0] := $EC;
    Head[1] := $01;
    for i := 0 to 6 do Head[2 + i] := Byte(N shr (8 * (6 - i)));
  end
  else
  begin
    SetLength(Head, 8);
    Total := 8 + N;
    for i := 0 to 3 do Head[i] := Byte(Total shr (8 * (3 - i)));
    Head[4] := Ord('f'); Head[5] := Ord('r'); Head[6] := Ord('e'); Head[7] := Ord('e');
  end;
  Total := Length(Head) + N;
  Crc := BlockCrc32(Z, 0, Length(Z));
  SetLength(Foot, FOOT_SIZE);
  for i := 0 to 7 do Foot[i] := Byte(MAGIC_FOOT[i]);
  Foot[8] := BLOCK_VERSION; Foot[9] := 0; Foot[10] := 0; Foot[11] := 0;
  for i := 0 to 7 do Foot[12 + i] := Byte(Total shr (8 * i));
  for i := 0 to 3 do Foot[20 + i] := Byte(Crc shr (8 * i));
  Result := Head + Body + Foot;
end;

function WriteTailBlock(const APath, AJson: string): TWriteResult;
// Troca (ou cria) o bloco. Ordem pensada pra queda no meio: primeiro CORTA
// (o arquivo volta a ser o original limpo), depois escreve o novo. Datas
// restauradas no fim — a galeria agrupa pela modificacao.
var
  H: THandle;
  Kind, OldJson: string;
  Size, OldLen, DataEnd: Int64;
  CT, AT, WT: TFileTime;
  Env: TBytes;
  Wrote, Err: DWORD;
begin
  Kind := KindOf(APath);
  if Kind = '' then Exit(wrNothing);
  if not FileExists(APath) then Exit(wrGone);
  if IsCloudOnly(APath) then Exit(wrCloud);
  // So FILE_SHARE_READ: se OUTRO processo tem o arquivo aberto pra escrita
  // (o OBS gravando, uma copia em curso), a abertura falha e tentamos
  // depois — nunca escrevemos por baixo de quem esta escrevendo.
  H := CreateFileW(PWideChar(APath), GENERIC_READ or GENERIC_WRITE,
    FILE_SHARE_READ, nil, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, 0);
  if H = INVALID_HANDLE_VALUE then
  begin
    Err := GetLastError;
    if (Err = ERROR_SHARING_VIOLATION_) or (Err = ERROR_LOCK_VIOLATION_) then
      Exit(wrLocked);
    Log('FileMeta: nao abri "%s" pra escrita (erro %d).', [APath, Err]);
    Exit(wrError);
  end;
  try
    GetFileTime(H, @CT, @AT, @WT);
    if not FM_GetFileSizeEx(H, Size) then Exit(wrError);
    if not ReadTailBlockH(H, Size, OldJson, OldLen) then OldLen := 0;
    if Kind = 'mkv' then DataEnd := MkvDataEnd(H, Size - OldLen)
    else DataEnd := Mp4DataEnd(H, Size - OldLen);
    if DataEnd < 0 then Exit(wrUnsafe);
    Env := BuildEnvelope(AJson, Kind);
    if not FM_SetFilePointerEx(H, DataEnd, nil, FILE_BEGIN) then Exit(wrError);
    if not SetEndOfFile(H) then Exit(wrError);
    Wrote := 0;
    if not WriteFile(H, Env[0], Length(Env), Wrote, nil) or
       (Integer(Wrote) <> Length(Env)) then
    begin
      Log('FileMeta: escrita incompleta em "%s".', [APath]);
      Exit(wrError);
    end;
    SetFileTime(H, @CT, @AT, @WT);
    Result := wrOk;
  finally
    CloseHandle(H);
  end;
  // De novo depois de fechar: garante que o fechamento nao carimbou "agora".
  H := CreateFileW(PWideChar(APath), FILE_WRITE_ATTRIBUTES,
    FILE_SHARE_READ or FILE_SHARE_WRITE or FILE_SHARE_DELETE, nil,
    OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, 0);
  if H <> INVALID_HANDLE_VALUE then
  begin
    SetFileTime(H, @CT, @AT, @WT);
    CloseHandle(H);
  end;
end;

// ---------------------------------------------------------------- cache

procedure ReadCacheMarks(const APath: string; out ARev, ASig: string);
var
  V: TJSONValue;
  S: string;
begin
  ARev := '';
  ASig := '';
  V := TJSONObject.ParseJSONValue(OBSPlayer.LoadMetaSubset(APath, [KEY_REV, KEY_SIG]));
  try
    if V is TJSONObject then
    begin
      if TJSONObject(V).TryGetValue<string>(KEY_REV, S) then ARev := S;
      if TJSONObject(V).TryGetValue<string>(KEY_SIG, S) then ASig := S;
    end;
  finally
    V.Free;
  end;
end;

procedure WriteCacheMark(const APath, AKey, AValue: string);
var
  O: TJSONObject;
begin
  O := TJSONObject.Create;
  try
    O.AddPair(AKey, AValue);
    OBSPlayer.SaveMetaSubset(APath, O.ToJSON);
  finally
    O.Free;
  end;
end;

function BuildBlockJson(const APath, ARev: string): string;
var
  Root: TJSONObject;
  V: TJSONValue;
  TrPath, TxPath: string;
begin
  Root := TJSONObject.Create;
  try
    Root.AddPair('app', 'NoOBS');
    Root.AddPair('v', TJSONNumber.Create(BLOCK_VERSION));
    Root.AddPair('rev', ARev);
    V := TJSONObject.ParseJSONValue(OBSPlayer.LoadMetaSubset(APath, PORTABLE_KEYS));
    if V is TJSONObject then Root.AddPair('meta', V) else V.Free;
    TrPath := OBSTranscribe.TranscriptPath(APath);
    if TFile.Exists(TrPath) then
    begin
      V := TJSONObject.ParseJSONValue(TFile.ReadAllText(TrPath, TEncoding.UTF8));
      if V <> nil then
      begin
        Root.AddPair('transcript', V);
        TxPath := OBSTranscribe.TranscriptTextPath(APath);
        if TFile.Exists(TxPath) then
          Root.AddPair('transcriptText', TFile.ReadAllText(TxPath, TEncoding.UTF8))
        else
          Root.AddPair('transcriptText', '');
      end;
    end;
    Result := Root.ToJSON;
  finally
    Root.Free;
  end;
end;

function BlockRev(const AJson: string; out ARoot: TJSONObject): string;
var
  V: TJSONValue;
  S: string;
begin
  Result := '';
  ARoot := nil;
  V := TJSONObject.ParseJSONValue(AJson);
  if not (V is TJSONObject) then
  begin
    V.Free;
    Exit;
  end;
  ARoot := TJSONObject(V);
  if not (ARoot.TryGetValue<string>('app', S) and (S = 'NoOBS')) then
  begin
    FreeAndNil(ARoot);
    Exit;
  end;
  if ARoot.TryGetValue<string>('rev', S) then Result := S;
end;

// ---------------------------------------------------------------- fila

function PendingPath: string;
begin
  Result := IncludeTrailingPathDelimiter(
    ExtractFileDir(ExcludeTrailingPathDelimiter(OBSPlayer.CacheRootDir))) + PENDING_FILE;
end;

procedure Enqueue(const APath: string; ADelayMs: Cardinal);
var
  K: string;
begin
  if GLock = nil then Exit;
  K := LowerCase(APath);
  GLock.Enter;
  try
    GDue.AddOrSetValue(K, GetTickCount64 + ADelayMs);
    if not GPaths.ContainsKey(K) then GDirty := True;
    GPaths.AddOrSetValue(K, APath);
  finally
    GLock.Leave;
  end;
  if GWake <> nil then GWake.SetEvent;
end;

procedure Drop(const APath: string);
var
  K: string;
begin
  K := LowerCase(APath);
  GLock.Enter;
  try
    if GPaths.ContainsKey(K) then GDirty := True;
    GDue.Remove(K);
    GPaths.Remove(K);
  finally
    GLock.Leave;
  end;
end;

procedure SavePending;
var
  Arr: TJSONArray;
  P: string;
begin
  if GLock = nil then Exit;
  Arr := TJSONArray.Create;
  try
    GLock.Enter;
    try
      for P in GPaths.Values do Arr.Add(P);
      GDirty := False;
    finally
      GLock.Leave;
    end;
    try
      TFile.WriteAllText(PendingPath, Arr.ToJSON, TEncoding.UTF8);
    except
      on E: Exception do Log('FileMeta: falha ao gravar a lista de pendentes: %s', [E.Message]);
    end;
  finally
    Arr.Free;
  end;
end;

procedure LoadPending;
var
  V: TJSONValue;
  i: Integer;
begin
  if not TFile.Exists(PendingPath) then Exit;
  try
    V := TJSONObject.ParseJSONValue(TFile.ReadAllText(PendingPath, TEncoding.UTF8));
    try
      if V is TJSONArray then
        for i := 0 to TJSONArray(V).Count - 1 do
          if TJSONArray(V).Items[i] is TJSONString then
            // Folga no arranque: o resto do app esta subindo.
            Enqueue(TJSONString(TJSONArray(V).Items[i]).Value, 10_000);
    finally
      V.Free;
    end;
  except
    on E: Exception do Log('FileMeta: lista de pendentes ilegivel: %s', [E.Message]);
  end;
end;

function SyncOne(const APath: string): TWriteResult;
// Escreve no arquivo o que o cache tem, se o cache for mais novo.
var
  CacheRev, Sig, Json: string;
  Root: TJSONObject;
begin
  if not FileExists(APath) then Exit(wrGone);
  if KindOf(APath) = '' then Exit(wrNothing);
  ReadCacheMarks(APath, CacheRev, Sig);
  if CacheRev = '' then Exit(wrNothing);   // cache sem nada a levar
  if IsCloudOnly(APath) then Exit(wrCloud);
  if ReadTailBlock(APath, Json) then
  begin
    if BlockRev(Json, Root) >= CacheRev then
    begin
      Root.Free;
      WriteCacheMark(APath, KEY_SIG, FileSig(APath));
      Exit(wrNothing);    // o arquivo ja esta em dia
    end;
    Root.Free;
  end;
  Result := WriteTailBlock(APath, BuildBlockJson(APath, CacheRev));
  if Result = wrOk then
  begin
    WriteCacheMark(APath, KEY_SIG, FileSig(APath));
    Log('FileMeta: dados gravados no fim de "%s".', [ExtractFileName(APath)]);
  end;
end;

procedure TFileMetaThread.Execute;
var
  Path: string;
  Pair: TPair<string, UInt64>;
  Now64: UInt64;
  R: TWriteResult;
  Requeued: Boolean;
begin
  while not Terminated do
  begin
    GWake.WaitFor(500);
    if Terminated then Break;
    while not Terminated do
    begin
      Path := '';
      Now64 := GetTickCount64;
      GLock.Enter;
      try
        for Pair in GDue do
          if Pair.Value <= Now64 then
          begin
            Path := GPaths[Pair.Key];
            // Tira do relogio enquanto processa: um MarkChanged no meio
            // recoloca, e ele roda de novo depois.
            GDue.Remove(Pair.Key);
            Break;
          end;
      finally
        GLock.Leave;
      end;
      if Path = '' then Break;
      // except SEM filtro de tipo: assim o compilador ve que R sai atribuido
      // nos dois caminhos (com "on E: Exception" ele acusa W1036, e
      // inicializar antes do try troca o aviso pelo H2077).
      try
        R := SyncOne(Path);
      except
        R := wrError;
        if ExceptObject is Exception then
          Log('FileMeta: erro em "%s": %s', [Path, Exception(ExceptObject).Message]);
      end;
      GLock.Enter;
      try
        // Recolocado durante o processamento (mudou de novo): fica na fila
        // com o horario novo, seja qual for o resultado desta volta.
        Requeued := GDue.ContainsKey(LowerCase(Path));
      finally
        GLock.Leave;
      end;
      if Requeued then Continue;
      case R of
        wrLocked: Enqueue(Path, RETRY_LOCKED_MS);
        wrCloud:  Enqueue(Path, RETRY_CLOUD_MS);
        wrError:  Enqueue(Path, RETRY_ERROR_MS);
        wrUnsafe:
          begin
            Log('FileMeta: "%s" termina num elemento incompleto (gravacao ' +
              'interrompida) — os dados ficam so no cache.', [ExtractFileName(Path)]);
            Drop(Path);
          end;
      else
        Drop(Path);
      end;
    end;
    if GDirty then SavePending;
  end;
end;

// ---------------------------------------------------------------- publico

procedure Start;
begin
  if GLock <> nil then Exit;
  GLock := TCriticalSection.Create;
  GDue := TDictionary<string, UInt64>.Create;
  GPaths := TDictionary<string, string>.Create;
  GWake := TEvent.Create(nil, False, False, '');
  LoadPending;
  GDirty := False;
  // Nao-suspensa, sem Start explicito (pegadinha #45).
  GWorker := TFileMetaThread.Create(False);
  GWorker.FreeOnTerminate := False;
end;

procedure Shutdown;
begin
  if GWorker <> nil then
  begin
    GWorker.Terminate;
    GWake.SetEvent;
    GWorker.WaitFor;
    FreeAndNil(GWorker);
  end;
  if GLock <> nil then
  begin
    SavePending;
    FreeAndNil(GDue);
    FreeAndNil(GPaths);
    FreeAndNil(GWake);
    FreeAndNil(GLock);
  end;
  GOnImported := nil;
end;

procedure MarkChanged(const APath: string);
begin
  if (APath = '') or (KindOf(APath) = '') then Exit;
  try
    WriteCacheMark(APath, KEY_REV, NowRev);
  except
    on E: Exception do
    begin
      Log('FileMeta: MarkChanged falhou: %s', [E.Message]);
      Exit;
    end;
  end;
  Enqueue(APath, DELAY_CHANGE_MS);
end;

function ImportIfNewer(const APath: string): Boolean;
var
  CacheRev, CacheSig, Sig, Json, Rev, S: string;
  Root, Meta, Upd: TJSONObject;
  V: TJSONValue;
  i: Integer;
begin
  Result := False;
  if (KindOf(APath) = '') or not FileExists(APath) then Exit;
  if IsCloudOnly(APath) then Exit;
  Sig := FileSig(APath);
  ReadCacheMarks(APath, CacheRev, CacheSig);
  if (Sig <> '') and (Sig = CacheSig) then Exit;   // nada mudou no arquivo

  Root := nil;
  if ReadTailBlock(APath, Json) then Rev := BlockRev(Json, Root) else Rev := '';
  try
    if (Root <> nil) and (Rev > CacheRev) then
    begin
      // Bloco mais novo: o arquivo veio de outra maquina (ou foi trocado
      // por uma copia mais nova). Popula o cache.
      Upd := TJSONObject.Create;
      try
        if Root.TryGetValue<TJSONObject>('meta', Meta) then
          for i := 0 to Meta.Count - 1 do
            Upd.AddPair(Meta.Pairs[i].JsonString.Value,
              TJSONValue(Meta.Pairs[i].JsonValue.Clone));
        Upd.AddPair(KEY_REV, Rev);
        Upd.AddPair(KEY_SIG, Sig);
        OBSPlayer.SaveMetaSubset(APath, Upd.ToJSON);
      finally
        Upd.Free;
      end;
      V := Root.GetValue('transcript');
      if V <> nil then
      begin
        ForceDirectories(OBSPlayer.CacheRootDir);
        TFile.WriteAllText(OBSTranscribe.TranscriptPath(APath), V.ToJSON, TEncoding.UTF8);
        if not Root.TryGetValue<string>('transcriptText', S) then S := '';
        TFile.WriteAllText(OBSTranscribe.TranscriptTextPath(APath), S, TEncoding.UTF8);
      end;
      Log('FileMeta: cache de "%s" preenchido pelos dados do arquivo (rev %s).',
        [ExtractFileName(APath), Rev]);
      Result := True;
    end
    else
    begin
      // Cache mais novo que o arquivo (ou arquivo sem bloco com edicao
      // local): o arquivo precisa ser atualizado.
      if (CacheRev <> '') and (CacheRev > Rev) then Enqueue(APath, DELAY_CHANGE_MS);
      WriteCacheMark(APath, KEY_SIG, Sig);
    end;
  finally
    Root.Free;
  end;
  if Result and Assigned(GOnImported) then
    try GOnImported(APath); except end;
end;

procedure RenamePath(const AOld, ANew: string);
var
  OldL, K, NewPath: string;
  Keys: TArray<string>;
  Due: UInt64;
begin
  if GLock = nil then Exit;
  OldL := LowerCase(ExcludeTrailingPathDelimiter(AOld));
  GLock.Enter;
  try
    Keys := GPaths.Keys.ToArray;
    for K in Keys do
    begin
      if K = OldL then NewPath := ANew
      else if K.StartsWith(OldL + PathDelim) then
        NewPath := IncludeTrailingPathDelimiter(ANew) + Copy(GPaths[K], Length(OldL) + 2, MaxInt)
      else Continue;
      if not GDue.TryGetValue(K, Due) then Due := GetTickCount64 + DELAY_CHANGE_MS;
      GDue.Remove(K);
      GPaths.Remove(K);
      GDue.AddOrSetValue(LowerCase(NewPath), Due);
      GPaths.AddOrSetValue(LowerCase(NewPath), NewPath);
      GDirty := True;
    end;
  finally
    GLock.Leave;
  end;
end;

procedure SetOnImported(ACallback: TFileMetaImported);
begin
  GOnImported := ACallback;
end;

end.
