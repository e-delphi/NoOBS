(*
  WinGpuUsage - uso da placa de video POR MOTOR, pelos contadores de
  desempenho do Windows (PDH, contador "Utilization Percentage" de todas
  as instancias de "GPU Engine" — o caminho exato esta em COUNTER_PATH).

  Serve ao monitor de desempenho do OBSBridge: com o buffer ou a gravacao
  ligados, uma linha no log a cada 30 s diz quanto o JOGO usa da placa
  (motor 3D) e quanto o NoOBS usa (o motor de codificacao de video). E o
  que separa "o buffer pesou" de "a placa ja estava no limite".

  Cada instancia do contador e um par processo x motor:
    pid_<pid>_luid_<adaptador>_phys_0_eng_<n>_engtype_<tipo>
  Somamos por (adaptador, tipo) e mostramos o adaptador mais ocupado — com
  video integrado + placa dedicada, o outro adaptador so poluiria a linha.
  O nome do tipo vem do driver e NAO e padronizado: na AMD a codificacao e
  "Video Codec Engine", na NVIDIA "VideoEncode". Por isso a linha lista os
  motores mais usados pelo nome que vier, sem tabela fixa.

  O contador e uma TAXA: o valor de cada leitura e a media desde a leitura
  anterior. Medido: ~1,5 ms por leitura com ~400 instancias — barato o
  bastante pra rodar na main thread a cada 30 s.
*)
unit WinGpuUsage;

interface

// Abre a consulta e faz a 1a leitura (a base da media). Idempotente.
procedure GpuUsageStart;
// Le o intervalo desde a leitura anterior e devolve um resumo pronto pro
// log, ex.: "3D 87% (NoOBS 2%), Video Codec Engine 14% (NoOBS 14%)".
// '' se os contadores nao existem (Windows antigo, driver sem suporte).
function GpuUsageSample(AOwnPid: Cardinal): string;
procedure GpuUsageStop;

implementation

uses
  Winapi.Windows,
  System.SysUtils,
  System.Generics.Collections,
  System.Generics.Defaults,
  OBSLog;

const
  PDH_FMT_DOUBLE   = $00000200;
  PDH_FMT_NOCAP100 = $00008000;   // soma de motores pode passar de 100
  PDH_MORE_DATA    = Cardinal($800007D2);
  COUNTER_PATH     = '\GPU Engine(*)\Utilization Percentage';
  MAX_ENGINES      = 4;            // motores mostrados na linha
  MIN_PCT          = 1.0;          // abaixo disso nao entra

type
  PDH_FMT_COUNTERVALUE = record
    CStatus: DWORD;
    Pad: DWORD;                    // alinhamento do union de 8 bytes (x64)
    doubleValue: Double;
  end;
  PDH_FMT_COUNTERVALUE_ITEM_W = record
    szName: PWideChar;
    FmtValue: PDH_FMT_COUNTERVALUE;
  end;                             // 24 bytes no x64 (conferido no prototipo)
  PPdhItem = ^PDH_FMT_COUNTERVALUE_ITEM_W;

function PdhOpenQueryW(szDataSource: PWideChar; dwUserData: NativeUInt;
  out phQuery: THandle): Cardinal; stdcall; external 'pdh.dll';
function PdhAddEnglishCounterW(hQuery: THandle; szFullCounterPath: PWideChar;
  dwUserData: NativeUInt; out phCounter: THandle): Cardinal; stdcall;
  external 'pdh.dll';
function PdhCollectQueryData(hQuery: THandle): Cardinal; stdcall;
  external 'pdh.dll';
function PdhGetFormattedCounterArrayW(hCounter: THandle; dwFormat: DWORD;
  var lpdwBufferSize: DWORD; var lpdwItemCount: DWORD;
  ItemBuffer: Pointer): Cardinal; stdcall; external 'pdh.dll';
function PdhCloseQuery(hQuery: THandle): Cardinal; stdcall; external 'pdh.dll';

var
  GQuery: THandle = 0;
  GCounter: THandle = 0;
  GFailed: Boolean = False;

procedure GpuUsageStart;
var
  R: Cardinal;
begin
  if (GQuery <> 0) or GFailed then Exit;
  R := PdhOpenQueryW(nil, 0, GQuery);
  if R <> 0 then
  begin
    GQuery := 0;
    GFailed := True;
    Log('GpuUsage: PdhOpenQuery falhou ($%x).', [R]);
    Exit;
  end;
  R := PdhAddEnglishCounterW(GQuery, COUNTER_PATH, 0, GCounter);
  if R <> 0 then
  begin
    // Windows sem os contadores de GPU (anterior ao 10 1709) ou driver que
    // nao os publica: o monitor segue sem esta parte.
    Log('GpuUsage: contador de GPU indisponivel ($%x).', [R]);
    PdhCloseQuery(GQuery);
    GQuery := 0;
    GFailed := True;
    Exit;
  end;
  PdhCollectQueryData(GQuery);   // base: a 1a media sai na proxima leitura
end;

procedure GpuUsageStop;
begin
  if GQuery <> 0 then PdhCloseQuery(GQuery);
  GQuery := 0;
  GCounter := 0;
end;

// Partes do nome da instancia: pid, adaptador e tipo de motor.
function ParseInstance(const AName: string; out APid: Cardinal;
  out ALuid, AEng: string): Boolean;
var
  P1, P2: Integer;
begin
  Result := False;
  P1 := Pos('pid_', AName);
  P2 := Pos('_luid_', AName);
  if (P1 <> 1) or (P2 = 0) then Exit;
  APid := StrToIntDef(Copy(AName, 5, P2 - 5), 0);
  P1 := P2 + Length('_luid_');
  P2 := Pos('_phys_', AName);
  if P2 <= P1 then Exit;
  ALuid := Copy(AName, P1, P2 - P1);
  P1 := Pos('_engtype_', AName);
  if P1 = 0 then Exit;
  AEng := Copy(AName, P1 + Length('_engtype_'), MaxInt);
  Result := AEng <> '';
end;

procedure AddTo(D: TDictionary<string, Double>; const K: string; V: Double);
var
  Cur: Double;
begin
  if not D.TryGetValue(K, Cur) then Cur := 0;
  D.AddOrSetValue(K, Cur + V);
end;

function GetOr0(D: TDictionary<string, Double>; const K: string): Double;
begin
  if not D.TryGetValue(K, Result) then Result := 0;
end;

function GpuUsageSample(AOwnPid: Cardinal): string;
type
  TEngRow = record
    Eng: string;
    Total, Own: Double;
  end;
var
  Size, Count, i: DWORD;
  Buf: TBytes;
  Item: PPdhItem;
  Pid: Cardinal;
  Luid, Eng, Key, BestLuid: string;
  V, BestSum: Double;
  Total, Own, LuidSum: TDictionary<string, Double>;
  Rows: TList<TEngRow>;
  Row: TEngRow;
  Pair: TPair<string, Double>;
  Shown: Integer;
begin
  Result := '';
  if GQuery = 0 then Exit;
  if PdhCollectQueryData(GQuery) <> 0 then Exit;
  Size := 0;
  Count := 0;
  if PdhGetFormattedCounterArrayW(GCounter, PDH_FMT_DOUBLE or PDH_FMT_NOCAP100,
       Size, Count, nil) <> PDH_MORE_DATA then Exit;
  SetLength(Buf, Size);
  if PdhGetFormattedCounterArrayW(GCounter, PDH_FMT_DOUBLE or PDH_FMT_NOCAP100,
       Size, Count, @Buf[0]) <> 0 then Exit;

  Total := TDictionary<string, Double>.Create;
  Own := TDictionary<string, Double>.Create;
  LuidSum := TDictionary<string, Double>.Create;
  Rows := TList<TEngRow>.Create;
  try
    Item := PPdhItem(@Buf[0]);
    for i := 1 to Count do
    begin
      if (Item.FmtValue.CStatus = 0) and (Item.szName <> nil) and
         ParseInstance(string(Item.szName), Pid, Luid, Eng) then
      begin
        V := Item.FmtValue.doubleValue;
        // Uma instancia e UM motor de UM processo: passa de 100% so com
        // leitura quebrada do contador (visto: 6149686312366% numa amostra
        // com status OK, durante um jogo). Fora da faixa, descarta.
        if (V < 0) or (V > 100.5) then
        begin
          Inc(Item);
          Continue;
        end;
        Key := Luid + #9 + Eng;
        AddTo(Total, Key, V);
        if Pid = AOwnPid then AddTo(Own, Key, V);
        AddTo(LuidSum, Luid, V);
      end;
      Inc(Item);
    end;

    // Adaptador mais ocupado (a placa do jogo).
    BestLuid := '';
    BestSum := -1;
    for Pair in LuidSum do
      if Pair.Value > BestSum then
      begin
        BestSum := Pair.Value;
        BestLuid := Pair.Key;
      end;
    if BestLuid = '' then Exit;

    for Pair in Total do
      if Pair.Key.StartsWith(BestLuid + #9) and (Pair.Value >= MIN_PCT) then
      begin
        Row.Eng := Copy(Pair.Key, Length(BestLuid) + 2, MaxInt);
        Row.Total := Pair.Value;
        Row.Own := GetOr0(Own, Pair.Key);
        Rows.Add(Row);
      end;
    Rows.Sort(TComparer<TEngRow>.Construct(
      function(const A, B: TEngRow): Integer
      begin
        if A.Total > B.Total then Result := -1
        else if A.Total < B.Total then Result := 1
        else Result := 0;
      end));

    Shown := 0;
    for Row in Rows do
    begin
      if Shown >= MAX_ENGINES then Break;
      if Result <> '' then Result := Result + ', ';
      Result := Result + Format('%s %.0f%% (NoOBS %.0f%%)',
        [Row.Eng, Row.Total, Row.Own]);
      Inc(Shown);
    end;
    if Result = '' then Result := 'ociosa';
  finally
    Rows.Free;
    LuidSum.Free;
    Own.Free;
    Total.Free;
  end;
end;

end.
