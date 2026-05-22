(*
  OBSAudioTracks - atribuição de faixas de áudio e enumeração de
  dispositivos via libobs.

  Conteudo coeso:
    - Enumeracao de devices (mic/speaker) via obs_properties da fonte
      wasapi_input_capture / wasapi_output_capture, com timeout pra
      proteger contra WASAPI doente.
    - ComputeAudioTrackAssignments: single source of truth do algoritmo
      de tracks (mix + 5 isoladas, agrupamento parcial, default
      preservado individual sempre que possivel).
    - BuildTrackNames: nomes humanos pra cada track (vai como metadata
      "title" do encoder de audio no MKV).

  Esta unit nao guarda estado. Logica pura — pode ser testada isolada
  (com mocks de libobs se quiser).
*)
unit OBSAudioTracks;

interface

uses
  System.SysUtils,
  System.Classes,
  Winapi.Windows,
  LibOBS,
  NoOBSTypes;

// Conta quantos True ha num array (helper utilitario).
function CountTrue(const A: TArray<Boolean>): Integer;

// Enumera dispositivos de audio expostos por uma fonte do libobs
// (AKind = 'wasapi_input_capture' ou 'wasapi_output_capture').
// Roda a chamada WASAPI em worker com timeout — devolve lista vazia
// se o sistema travar mais de 3s (acontece se o audio service quebrar).
function EnumerateObsAudioDevices(const AKind: AnsiString): TArray<TObsAudioDev>;

// Logica unica de atribuicao de tracks de audio.
// Track 1 = Mix (sempre, todos contribuem). Tracks 2-6 = isoladas
// (5 slots). So devices enabled recebem track isolada — disabled
// retornam 0. Quando ha excedente, agrupa outputs primeiro (parcial);
// se outputs sozinhos nao bastarem, agrupa mics tambem. Default sempre
// preservado individual quando possivel. **A faixa agrupada (com 2+
// devices) fica sempre na ultima posicao usada (track 6 quando todos
// os 5 slots isolados sao preenchidos).**
//
// Esta funcao e o "single source of truth" do agrupamento — tanto
// BuildAndStartRecording (gravacao real) quanto BuildAudioJsonWithTracks
// (lista pra UI) a chamam. JS nao re-implementa nada: le `track` por
// device direto do JSON.
//
// AMicEnabled, AMicDefault: arrays paralelos com flags por mic.
// AOutEnabled, AOutDefault: idem pra outputs.
// Saidas AMicTracks, AOutTracks: track de cada device (0 = mix only).
// ATotalTracks: total de tracks usadas (inclui Mix).
procedure ComputeAudioTrackAssignments(
  const AMicEnabled, AMicDefault: TArray<Boolean>;
  const AOutEnabled, AOutDefault: TArray<Boolean>;
  out AMicTracks, AOutTracks: TArray<Integer>;
  out ATotalTracks: Integer);

// Calcula nomes humanos pra cada track (track 0 = Mix). Esses nomes
// viram a metadata "title" do encoder de audio no MKV — visiveis no
// info panel e em editores externos.
function BuildTrackNames(ATotalTracks: Integer;
  const AMics, AOutputs: TArray<TObsAudioDev>;
  const AMicTracks, AOutTracks: TArray<Integer>): TArray<string>;

implementation

uses
  OBSLog;

function CountTrue(const A: TArray<Boolean>): Integer;
var i: Integer;
begin
  Result := 0;
  for i := 0 to High(A) do if A[i] then Inc(Result);
end;

function FromAnsi(P: PAnsiChar): string;
// Strings do libobs sao UTF-8. UTF8ToString converte independente da
// locale do sistema (cp1252 quebra acentos em PT-BR).
begin
  if P = nil then Result := ''
  else Result := UTF8ToString(P);
end;

function EnumerateObsAudioDevicesRaw(const AKind: AnsiString): TArray<TObsAudioDev>;
var
  Props: obs_properties_t;
  Prop: obs_property_t;
  Count, i: NativeUInt;
  ItemValue: AnsiString;
  Dev: TObsAudioDev;
begin
  SetLength(Result, 0);
  Props := obs_get_source_properties(PAnsiChar(AKind));
  if Props = nil then Exit;
  try
    Prop := obs_properties_get(Props, 'device_id');
    if Prop = nil then Exit;
    Count := obs_property_list_item_count(Prop);
    // Count e NativeUInt — se 0 (sem mic conectado, por exemplo),
    // Count-1 underflowa pra $FFFFFFFF e dispara EIntOverflow.
    if Count = 0 then Exit;
    for i := 0 to Count - 1 do
    begin
      // OBS retorna strings em UTF-8. string(AnsiString(...)) interpretaria
      // como cp1252 e quebraria acentos ("Saida" -> "SaA­da"). Usar FromAnsi
      // (UTF8ToString) garante decodificacao correta.
      Dev.Name := FromAnsi(obs_property_list_item_name(Prop, i));
      ItemValue := AnsiString(obs_property_list_item_string(Prop, i));
      if ItemValue = 'default' then Continue;
      Dev.DeviceId := ItemValue;
      SetLength(Result, Length(Result) + 1);
      Result[High(Result)] := Dev;
    end;
  finally
    obs_properties_destroy(Props);
  end;
end;

function EnumerateObsAudioDevices(const AKind: AnsiString): TArray<TObsAudioDev>;
// Wrapper com timeout: obs_get_source_properties('wasapi_*') chama
// internamente o WASAPI do Windows pra listar devices. Quando o audio
// service esta doente (ex.: depois de remover o ultimo mic), essa
// chamada pode travar 60s+. Rodamos em worker e damos 3s — se nao
// retornar, devolve lista vazia (gravacao continua sem audio).
// 3s e folga: caso saudavel retorna em <50ms.
const
  TIMEOUT_MS = 3000;
var
  Worker: TThread;
  Output: TArray<TObsAudioDev>;
  Wait: DWORD;
  T0, Elapsed: UInt64;
begin
  SetLength(Output, 0);
  T0 := GetTickCount64;
  Log('   enum %s: iniciando (timeout=%dms)...', [string(AKind), TIMEOUT_MS]);
  Worker := TThread.CreateAnonymousThread(
    procedure
    begin
      try
        Output := EnumerateObsAudioDevicesRaw(AKind);
      except
        on E: Exception do
        begin
          SetLength(Output, 0);
          Log('   enum %s: excecao no worker: %s', [string(AKind), E.Message]);
        end;
      end;
    end);
  Worker.FreeOnTerminate := False;
  Worker.Start;
  Wait := WaitForSingleObject(Worker.Handle, TIMEOUT_MS);
  Elapsed := GetTickCount64 - T0;
  if Wait = WAIT_TIMEOUT then
  begin
    Log('   enum %s: TIMEOUT apos %dms — sem audio (WASAPI travado).',
      [string(AKind), Elapsed]);
    SetLength(Result, 0);
    // Vazado de proposito: thread ainda esta presa no WASAPI. O OS
    // limpa quando o processo morrer; tentar Free aqui bloqueia.
  end
  else
  begin
    Result := Output;
    Worker.Free;
    Log('   enum %s: %d device(s) em %dms.',
      [string(AKind), Length(Result), Elapsed]);
  end;
end;

procedure ComputeAudioTrackAssignments(
  const AMicEnabled, AMicDefault: TArray<Boolean>;
  const AOutEnabled, AOutDefault: TArray<Boolean>;
  out AMicTracks, AOutTracks: TArray<Integer>;
  out ATotalTracks: Integer);
// Ordem fisica das tracks no MKV gravado:
//   Track 1 = Mix (sempre)
//   Track 2 = mic default (se habilitado)
//   Track 3 = output default (se habilitado)
//   Track 4..N = outros mics individuais
//   Track N+1..5 = outros outputs individuais
//   Track 6 = faixa agrupada (quando ha mais devices do que slots)
//
// Defaults sempre individuais — agrupamento so atinge nao-defaults.
// A faixa agrupada ocupa SEMPRE a posicao 6 (ultima) quando existe —
// individuais ficam em 2..5, sobra apenas se nao houver agrupamento.
const
  ISOLATED_SLOTS = 5;  // tracks 2-6
var
  j, NextTrack, GroupedSoFar: Integer;
  EnabledMics, EnabledOuts: Integer;
  DefaultMicIdx, DefaultOutIdx: Integer;
  HasDefaultMic, HasDefaultOut: Boolean;
  OtherMics, OtherOuts, RemainingSlots, OtherTotal, Surplus: Integer;
  OutputGroupSize: Integer;
  GroupOtherMics: Boolean;
  MicGroupTrack, OutGroupTrack: Integer;
  // Vars do post-processing (faixa agrupada vai pro final)
  TrackCount: array[0..6] of Integer;
  Remap: array[0..6] of Integer;
  Individuals, Groups: TArray<Integer>;
  t, NewTrack, HasGroup: Integer;
begin
  SetLength(AMicTracks, Length(AMicEnabled));
  SetLength(AOutTracks, Length(AOutEnabled));
  for j := 0 to High(AMicTracks) do AMicTracks[j] := 0;
  for j := 0 to High(AOutTracks) do AOutTracks[j] := 0;

  EnabledMics := CountTrue(AMicEnabled);
  EnabledOuts := CountTrue(AOutEnabled);

  // Localiza index do default mic + default output (so se enabled).
  DefaultMicIdx := -1;
  for j := 0 to High(AMicEnabled) do
    if AMicEnabled[j] and AMicDefault[j] then
    begin DefaultMicIdx := j; Break; end;
  DefaultOutIdx := -1;
  for j := 0 to High(AOutEnabled) do
    if AOutEnabled[j] and AOutDefault[j] then
    begin DefaultOutIdx := j; Break; end;
  HasDefaultMic := DefaultMicIdx >= 0;
  HasDefaultOut := DefaultOutIdx >= 0;

  // Slots ocupados pelos defaults (sempre individuais).
  OtherMics := EnabledMics;
  if HasDefaultMic then Dec(OtherMics);
  OtherOuts := EnabledOuts;
  if HasDefaultOut then Dec(OtherOuts);
  RemainingSlots := ISOLATED_SLOTS;
  if HasDefaultMic then Dec(RemainingSlots);
  if HasDefaultOut then Dec(RemainingSlots);
  OtherTotal := OtherMics + OtherOuts;

  // Decide agrupamento entre os NAO-default.
  GroupOtherMics := False;
  OutputGroupSize := 0;
  if RemainingSlots > 0 then
  begin
    if OtherTotal > RemainingSlots then
    begin
      Surplus := OtherTotal - RemainingSlots;
      if OtherOuts >= Surplus + 1 then
        // Agrupa apenas o excesso+1 outputs (poupa exatamente Surplus).
        OutputGroupSize := Surplus + 1
      else
      begin
        // Outputs nao bastam pro excesso. Agrupa todos os outros outputs.
        OutputGroupSize := OtherOuts;
        // Mics ainda nao cabem? Agrupa tambem (ultimo recurso).
        if OtherMics + 1 > RemainingSlots then GroupOtherMics := True;
      end;
    end;
  end
  else
  begin
    // Sem slot pra nenhum nao-default. Agrupa tudo o que sobrou.
    if OtherOuts > 0 then OutputGroupSize := OtherOuts;
    if OtherMics > 0 then GroupOtherMics := True;
  end;

  NextTrack := 2;

  // 1. Default mic (track 2)
  if HasDefaultMic then
  begin
    AMicTracks[DefaultMicIdx] := NextTrack;
    Inc(NextTrack);
  end;

  // 2. Default output (track 3, ou 2 se nao havia default mic)
  if HasDefaultOut then
  begin
    AOutTracks[DefaultOutIdx] := NextTrack;
    Inc(NextTrack);
  end;

  // 3. Outros mics (individuais ou agrupados)
  if OtherMics > 0 then
  begin
    if GroupOtherMics then
    begin
      MicGroupTrack := NextTrack;
      Inc(NextTrack);
      for j := 0 to High(AMicEnabled) do
        if (j <> DefaultMicIdx) and AMicEnabled[j] then
          AMicTracks[j] := MicGroupTrack;
    end
    else
      for j := 0 to High(AMicEnabled) do
        if (j <> DefaultMicIdx) and AMicEnabled[j] then
        begin
          AMicTracks[j] := NextTrack;
          Inc(NextTrack);
        end;
  end;

  // 4. Outros outputs (individuais ou agrupados)
  if OtherOuts > 0 then
  begin
    if OutputGroupSize > 0 then
    begin
      OutGroupTrack := NextTrack;
      Inc(NextTrack);
      GroupedSoFar := 0;
      // Primeiro preenche o grupo na ordem original (pulando o default).
      for j := 0 to High(AOutEnabled) do
        if (j <> DefaultOutIdx) and AOutEnabled[j] and
           (GroupedSoFar < OutputGroupSize) then
        begin
          AOutTracks[j] := OutGroupTrack;
          Inc(GroupedSoFar);
        end;
      // Restantes nao-default ficam individuais.
      for j := 0 to High(AOutEnabled) do
        if (j <> DefaultOutIdx) and AOutEnabled[j] and
           (AOutTracks[j] = 0) then
        begin
          AOutTracks[j] := NextTrack;
          Inc(NextTrack);
        end;
    end
    else
      for j := 0 to High(AOutEnabled) do
        if (j <> DefaultOutIdx) and AOutEnabled[j] then
        begin
          AOutTracks[j] := NextTrack;
          Inc(NextTrack);
        end;
  end;

  ATotalTracks := NextTrack - 1;
  if ATotalTracks > 6 then ATotalTracks := 6;

  // ===================================================================
  // Post-processing: faixa(s) agrupada(s) ao FINAL.
  //
  // A logica anterior atribui tracks na ordem que faz sentido pra
  // priorizacao (defaults primeiro, mics individuais, outputs com
  // grupo no meio se necessario). Mas o user pediu que a faixa
  // agrupada (track com >1 device) fique sempre na ULTIMA posicao
  // usada — ex: com 5 individuais + 1 grupo, o grupo vai pra track 6.
  //
  // Estrategia: conta devices por track; separa em "individuais"
  // (count=1) e "grupos" (count>=2) preservando a ordem original;
  // remapeia pra que individuais venham primeiro (tracks 2..) e
  // grupos depois (tracks ..6).
  // ===================================================================
  for t := 0 to 6 do begin TrackCount[t] := 0; Remap[t] := t; end;
  for j := 0 to High(AMicTracks) do
    if (AMicTracks[j] >= 2) and (AMicTracks[j] <= 6) then
      Inc(TrackCount[AMicTracks[j]]);
  for j := 0 to High(AOutTracks) do
    if (AOutTracks[j] >= 2) and (AOutTracks[j] <= 6) then
      Inc(TrackCount[AOutTracks[j]]);

  HasGroup := 0;
  SetLength(Individuals, 0);
  SetLength(Groups, 0);
  for t := 2 to 6 do
  begin
    if TrackCount[t] = 1 then
    begin
      SetLength(Individuals, Length(Individuals) + 1);
      Individuals[High(Individuals)] := t;
    end
    else if TrackCount[t] >= 2 then
    begin
      SetLength(Groups, Length(Groups) + 1);
      Groups[High(Groups)] := t;
      Inc(HasGroup);
    end;
  end;

  // So remapeia se ha pelo menos um grupo (caso comum sem grouping
  // nao precisa de remap).
  if HasGroup > 0 then
  begin
    // Reconstroi a numeracao: individuais 2..N, grupos N+1..M.
    NewTrack := 2;
    for j := 0 to High(Individuals) do
    begin
      Remap[Individuals[j]] := NewTrack;
      Inc(NewTrack);
    end;
    for j := 0 to High(Groups) do
    begin
      Remap[Groups[j]] := NewTrack;
      Inc(NewTrack);
    end;

    // Aplica o remap em todos os devices.
    for j := 0 to High(AMicTracks) do
      if AMicTracks[j] > 0 then AMicTracks[j] := Remap[AMicTracks[j]];
    for j := 0 to High(AOutTracks) do
      if AOutTracks[j] > 0 then AOutTracks[j] := Remap[AOutTracks[j]];
  end;
end;

function BuildTrackNames(ATotalTracks: Integer;
  const AMics, AOutputs: TArray<TObsAudioDev>;
  const AMicTracks, AOutTracks: TArray<Integer>): TArray<string>;
// Calcula os nomes humanos pra cada track de audio. Mapeamento:
//   Result[0] = nome da track 1 (sempre "Mix")
//   Result[1] = nome da track 2
//   ...
//   Result[ATotalTracks-1] = nome da track ATotalTracks
// Esses nomes viram a metadata "title" do encoder de audio no MKV.
//
// Suporta agrupamento parcial: track com 1 device usa o nome do device;
// track com varios devices junta os nomes ou usa rotulo generico se
// ficar muito longo.
var
  j, k, Track, Idx: Integer;
  TrackDevs: TArray<TArray<string>>;
  TrackKind: TArray<Integer>; // 0=nenhum, 1=mic, 2=out, 3=misto
  Combined: string;
begin
  SetLength(Result, ATotalTracks);
  if ATotalTracks <= 0 then Exit;

  Result[0] := 'Mix';

  // TrackDevs/TrackKind sao indexados como Result (0-based):
  // TrackDevs[k] tem devices da track (k+1). TrackDevs[0] fica vazio
  // pois e o Mix (alimentado por todos os devices via bitmask, mas o
  // nome e fixo).
  SetLength(TrackDevs, ATotalTracks);
  SetLength(TrackKind, ATotalTracks);

  for j := 0 to High(AMics) do
  begin
    Track := AMicTracks[j];
    Idx := Track - 1;
    if (Idx >= 1) and (Idx <= ATotalTracks - 1) then
    begin
      SetLength(TrackDevs[Idx], Length(TrackDevs[Idx]) + 1);
      TrackDevs[Idx][High(TrackDevs[Idx])] := AMics[j].Name;
      if TrackKind[Idx] = 0 then TrackKind[Idx] := 1
      else if TrackKind[Idx] = 2 then TrackKind[Idx] := 3;
    end;
  end;
  for j := 0 to High(AOutputs) do
  begin
    Track := AOutTracks[j];
    Idx := Track - 1;
    if (Idx >= 1) and (Idx <= ATotalTracks - 1) then
    begin
      SetLength(TrackDevs[Idx], Length(TrackDevs[Idx]) + 1);
      TrackDevs[Idx][High(TrackDevs[Idx])] := AOutputs[j].Name;
      if TrackKind[Idx] = 0 then TrackKind[Idx] := 2
      else if TrackKind[Idx] = 1 then TrackKind[Idx] := 3;
    end;
  end;

  for j := 1 to ATotalTracks - 1 do
  begin
    if Length(TrackDevs[j]) = 0 then
      Result[j] := Format('Faixa %d', [j + 1])
    else if Length(TrackDevs[j]) = 1 then
      Result[j] := TrackDevs[j][0]
    else
    begin
      // Multi-device: tenta concatenar com " + " ate 80 chars. Acima
      // disso usa rotulo curto baseado no kind dominante.
      Combined := TrackDevs[j][0];
      for k := 1 to High(TrackDevs[j]) do
        Combined := Combined + ' + ' + TrackDevs[j][k];
      if Length(Combined) <= 80 then
        Result[j] := Combined
      else
      begin
        case TrackKind[j] of
          1: Result[j] := Format('Microfones (%d)', [Length(TrackDevs[j])]);
          2: Result[j] := Format('Saidas (%d)', [Length(TrackDevs[j])]);
        else
          Result[j] := Format('Audio agrupado (%d)', [Length(TrackDevs[j])]);
        end;
      end;
    end;
  end;
end;

end.
