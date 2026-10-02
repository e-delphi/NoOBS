(*
  OBSAudioFilters - filtros de audio do OBS nos microfones da gravacao.

  Os filtros sao os do plugin obs-filters (supressao de ruido, porta de
  ruido, compressor, limitador, ganho, equalizador...). Esta unit:

    - LISTA os filtros de audio registrados (obs_enum_filter_types, so os
      com OBS_SOURCE_AUDIO e sem marca de obsoleto) numa ordem de cadeia
      que faz sentido (limpar o ruido antes de subir o ganho, limitar por
      ultimo);
    - descreve as PROPRIEDADES de cada um num JSON generico pra UI montar
      o editor (slider, numero, lista, caixa) — os rotulos vem do proprio
      plugin, no idioma do app (obs_set_locale);
    - guarda a escolha no config ('audioFilters': id -> enabled + settings);
    - PENDURA a cadeia habilitada em cada fonte de microfone
      (ApplyAudioFilters, chamado pelo OBSEngine ao montar o grafo);
    - faz o TESTE A/B: grava o microfone CRU uma vez e refaz a versao com
      filtros a partir desse mesmo trecho a cada mudanca (sem gravar de
      novo), empurrando as amostras numa fonte nossa sem som proprio
      (noobs_audio_feed) com a cadeia pendurada.

  Tudo aqui e libobs: MAIN THREAD, com o core ja inicializado (pegadinha
  #3). A unica excecao e o callback de captura do microfone cru, que roda
  na thread do WASAPI e so copia amostras sob lock.

  Propriedades SEMPRE de uma INSTANCIA do filtro, nunca do tipo
  (obs_get_source_properties): o expander_properties desreferencia o
  proprio estado (`cd->is_upwcomp`) e com data=NULL derrubaria o app.
*)
unit OBSAudioFilters;

interface

uses
  System.SysUtils,
  System.JSON,
  LibOBS;

// Ha algum filtro de audio registrado? (obs-filters.dll carregou.)
function AudioFiltersAvailable: Boolean;

// Lista completa pra UI: [{id, name, enabled, props:[...]}].
function BuildAudioFiltersJson: TJSONArray;
// Um filtro so — depois de mudar um valor a visibilidade dos campos pode
// mudar (ex.: metodo da supressao de ruido).
function BuildAudioFilterJson(const AId: string): TJSONObject;

procedure SetAudioFilterEnabled(const AId: string; AEnabled: Boolean);
// AValue: TJSONBool / TJSONNumber / TJSONString, conforme o tipo do campo.
procedure SetAudioFilterValue(const AId, AKey: string; AValue: TJSONValue);
procedure ResetAudioFilter(const AId: string);
// "Restaurar padroes" das Configuracoes: nenhum filtro ligado e os
// ajustes de todos de volta aos do plugin (apaga a chave do config).
procedure ResetAllAudioFilters;
function EnabledAudioFilterCount: Integer;

// Pendura os filtros habilitados, na ordem da cadeia, numa fonte de
// microfone recem-criada.
procedure ApplyAudioFilters(ASource: obs_source_t);

// Idioma dos rotulos que vem do plugin. ALang no vocabulario do app
// ('pt-BR', 'en', 'es'...).
procedure SetObsLocaleFromApp(const ALang: string);

// ---- teste A/B ----
function AudioFilterTestRunning: Boolean;
// ADeviceId: id do endpoint (o mesmo do WinAudioMeter/libobs) ou '' =
// microfone padrao do Windows.
function StartAudioFilterTest(const ADeviceId: string; out AErr: string): Boolean;
// Fecha as capturas e devolve os dois WAV (PCM 16 bits, mono, 48 kHz) e,
// de cada um, o pico e o nivel MEDIO (RMS) em dBFS (-100 = silencio). O
// medio e o que a supressao de ruido muda: os picos sao a voz, que ela
// preserva de proposito.
procedure FinishAudioFilterTest(out ARawWav, AFiltWav: TBytes;
  out ARawPeakDb, AFiltPeakDb, ARawRmsDb, AFiltRmsDb: Double);
// Ha um trecho gravado pra refazer? (Um teste ja terminou.)
function HasAudioFilterClip: Boolean;
// Refaz so a versao COM filtros, a partir do trecho guardado e da cadeia
// ATUAL — chamado a cada mudanca de filtro. False = nao ha trecho.
function RerenderAudioFilterTest(out AFiltWav: TBytes;
  out AFiltPeakDb, AFiltRmsDb: Double): Boolean;
procedure CancelAudioFilterTest;

implementation

uses
  Winapi.Windows,
  System.Math,
  // Depois do Math de proposito: o IfThen de string e o do StrUtils, e o
  // ultimo da lista e o que vale.
  System.StrUtils,
  System.Classes,
  System.SyncObjs,
  System.Generics.Collections,
  OBSConfig,
  OBSLog;

const
  CONFIG_KEY = 'audioFilters';

  // Ordem da cadeia. O que nao esta aqui (filtro de plugin que um dia
  // entre) vai no fim, na ordem em que a libobs o registrou.
  //   supressao/porta/expansor: tiram ruido ANTES de qualquer ganho, senao
  //     o ganho sobe o chiado junto;
  //   equalizador e ganho: moldam e nivelam o que sobrou;
  //   compressor e limitador: por ultimo, controlam os picos do resultado.
  CHAIN_ORDER: array[0..8] of string = (
    'noise_suppress_filter_v2',
    'noise_gate_filter',
    'expander_filter',
    'upward_compressor_filter',
    'basic_eq_filter',
    'gain_filter',
    'compressor_filter',
    'limiter_filter',
    'invert_polarity_filter'
  );

  // Campos que nao fazem sentido aqui: o sidechain do compressor aponta
  // pra OUTRA fonte da cena do OBS, que no NoOBS nao existe.
  SKIPPED_PROPS: array[0..0] of string = ('sidechain_source');

  SAMPLE_RATE = 48000;   // obs_reset_audio do OBSEngine
  MAX_TEST_SEC = 15;

function U8(const S: string): UTF8String; inline;
begin
  Result := UTF8String(S);
end;

function FromU8(P: PAnsiChar): string;
begin
  if P = nil then Result := '' else Result := UTF8ToString(P);
end;

// ---------------------------------------------------------------------
// Tipos registrados
// ---------------------------------------------------------------------

function RegisteredAudioFilterIds: TArray<string>;
var
  Idx: NativeUInt;
  P: PAnsiChar;
  Flags: Cardinal;
  All: TList<string>;
  Id: string;
  i: Integer;
begin
  All := TList<string>.Create;
  try
    Idx := 0;
    P := nil;
    while obs_enum_filter_types(Idx, P) do
    begin
      Inc(Idx);
      if P = nil then Continue;
      Flags := obs_get_source_output_flags(P);
      if (Flags and OBS_SOURCE_AUDIO) = 0 then Continue;
      if (Flags and (OBS_SOURCE_DEPRECATED or OBS_SOURCE_CAP_DISABLED)) <> 0 then
        Continue;
      All.Add(FromU8(P));
    end;
    SetLength(Result, 0);
    for i := 0 to High(CHAIN_ORDER) do
      if All.Contains(CHAIN_ORDER[i]) then
        Result := Result + [CHAIN_ORDER[i]];
    for Id in All do
      if not MatchStr(Id, CHAIN_ORDER) then
        Result := Result + [Id];
  finally
    All.Free;
  end;
end;

function AudioFiltersAvailable: Boolean;
begin
  Result := Length(RegisteredAudioFilterIds) > 0;
end;

// ---------------------------------------------------------------------
// Config: { "<id>": { "enabled": bool, "settings": { ... } } }
// ---------------------------------------------------------------------

function LoadCfg: TJSONObject;
var
  V: TJSONValue;
begin
  V := GetConfigJson(CONFIG_KEY);
  if V is TJSONObject then Exit(TJSONObject(V));
  V.Free;
  Result := TJSONObject.Create;
end;

function EntryOf(ACfg: TJSONObject; const AId: string; ACreate: Boolean): TJSONObject;
var
  V: TJSONValue;
begin
  V := ACfg.GetValue(AId);
  if V is TJSONObject then Exit(TJSONObject(V));
  Result := nil;
  if not ACreate then Exit;
  if V <> nil then ACfg.RemovePair(AId).Free;
  Result := TJSONObject.Create;
  Result.AddPair('enabled', TJSONBool.Create(False));
  Result.AddPair('settings', TJSONObject.Create);
  ACfg.AddPair(AId, Result);
end;

function IsEnabled(ACfg: TJSONObject; const AId: string): Boolean;
var
  E: TJSONObject;
begin
  E := EntryOf(ACfg, AId, False);
  Result := (E <> nil) and (E.GetValue('enabled') is TJSONBool) and
            TJSONBool(E.GetValue('enabled')).AsBoolean;
end;

function SettingsJsonOf(ACfg: TJSONObject; const AId: string): string;
var
  E: TJSONObject;
begin
  Result := '{}';
  E := EntryOf(ACfg, AId, False);
  if (E <> nil) and (E.GetValue('settings') is TJSONObject) then
    Result := E.GetValue('settings').ToJSON;
end;

// Instancia privada do filtro com as configuracoes salvas. Quem chama libera.
function CreateFilterInstance(const AId, ASettingsJson: string): obs_source_t;
var
  Settings: obs_data_t;
begin
  Settings := obs_data_create_from_json(PAnsiChar(U8(ASettingsJson)));
  if Settings = nil then Settings := obs_data_create;
  try
    Result := obs_source_create_private(PAnsiChar(U8(AId)),
      PAnsiChar(U8('NoOBS filtro ' + AId)), Settings);
  finally
    obs_data_release(Settings);
  end;
end;

// ---------------------------------------------------------------------
// Propriedades -> JSON
// ---------------------------------------------------------------------

procedure AddProps(AProps: obs_properties_t; ASettings: obs_data_t;
  AOut: TJSONArray);
var
  P: obs_property_t;
  Name: string;
  NameA: UTF8String;
  O: TJSONObject;
  Kind, Fmt: Integer;
  Items: TJSONArray;
  Item: TJSONObject;
  Count, i: NativeUInt;
  Sub: TJSONArray;
begin
  if AProps = nil then Exit;
  P := obs_properties_first(AProps);
  while P <> nil do
  begin
    Name := FromU8(obs_property_name(P));
    NameA := U8(Name);
    Kind := obs_property_get_type(P);
    if obs_property_visible(P) and not MatchStr(Name, SKIPPED_PROPS) then
    begin
      O := nil;
      case Kind of
        OBS_PROPERTY_BOOL:
          begin
            O := TJSONObject.Create;
            O.AddPair('type', 'bool');
            O.AddPair('value', TJSONBool.Create(
              obs_data_get_bool(ASettings, PAnsiChar(NameA))));
          end;
        OBS_PROPERTY_INT:
          begin
            O := TJSONObject.Create;
            O.AddPair('type', 'int');
            O.AddPair('min', TJSONNumber.Create(obs_property_int_min(P)));
            O.AddPair('max', TJSONNumber.Create(obs_property_int_max(P)));
            O.AddPair('step', TJSONNumber.Create(Max(1, obs_property_int_step(P))));
            O.AddPair('slider', TJSONBool.Create(
              obs_property_int_type(P) = OBS_NUMBER_SLIDER));
            O.AddPair('suffix', FromU8(obs_property_int_suffix(P)));
            O.AddPair('value', TJSONNumber.Create(
              obs_data_get_int(ASettings, PAnsiChar(NameA))));
          end;
        OBS_PROPERTY_FLOAT:
          begin
            O := TJSONObject.Create;
            O.AddPair('type', 'float');
            O.AddPair('min', TJSONNumber.Create(obs_property_float_min(P)));
            O.AddPair('max', TJSONNumber.Create(obs_property_float_max(P)));
            O.AddPair('step', TJSONNumber.Create(obs_property_float_step(P)));
            O.AddPair('slider', TJSONBool.Create(
              obs_property_float_type(P) = OBS_NUMBER_SLIDER));
            O.AddPair('suffix', FromU8(obs_property_float_suffix(P)));
            O.AddPair('value', TJSONNumber.Create(
              obs_data_get_double(ASettings, PAnsiChar(NameA))));
          end;
        OBS_PROPERTY_LIST:
          begin
            Fmt := obs_property_list_format(P);
            if Fmt in [OBS_COMBO_FORMAT_INT, OBS_COMBO_FORMAT_FLOAT,
                       OBS_COMBO_FORMAT_STRING] then
            begin
              O := TJSONObject.Create;
              O.AddPair('type', 'list');
              Items := TJSONArray.Create;
              Count := obs_property_list_item_count(P);
              if Count > 0 then
                for i := 0 to Count - 1 do
                begin
                  if obs_property_list_item_disabled(P, i) then Continue;
                  Item := TJSONObject.Create;
                  Item.AddPair('name', FromU8(obs_property_list_item_name(P, i)));
                  case Fmt of
                    OBS_COMBO_FORMAT_INT:
                      Item.AddPair('value', TJSONNumber.Create(
                        obs_property_list_item_int(P, i)));
                    OBS_COMBO_FORMAT_FLOAT:
                      Item.AddPair('value', TJSONNumber.Create(
                        obs_property_list_item_float(P, i)));
                  else
                    Item.AddPair('value', FromU8(obs_property_list_item_string(P, i)));
                  end;
                  Items.AddElement(Item);
                end;
              O.AddPair('items', Items);
              case Fmt of
                OBS_COMBO_FORMAT_INT:
                  begin
                    O.AddPair('format', 'int');
                    O.AddPair('value', TJSONNumber.Create(
                      obs_data_get_int(ASettings, PAnsiChar(NameA))));
                  end;
                OBS_COMBO_FORMAT_FLOAT:
                  begin
                    O.AddPair('format', 'float');
                    O.AddPair('value', TJSONNumber.Create(
                      obs_data_get_double(ASettings, PAnsiChar(NameA))));
                  end;
              else
                O.AddPair('format', 'string');
                O.AddPair('value', FromU8(obs_data_get_string(ASettings, PAnsiChar(NameA))));
              end;
            end;
          end;
        OBS_PROPERTY_TEXT:
          // So o texto informativo (aviso do plugin); campo de digitar nao
          // existe nos filtros de audio.
          if obs_property_text_type(P) = OBS_TEXT_INFO then
          begin
            O := TJSONObject.Create;
            O.AddPair('type', 'info');
          end;
        OBS_PROPERTY_GROUP:
          begin
            O := TJSONObject.Create;
            O.AddPair('type', 'group');
            Sub := TJSONArray.Create;
            AddProps(obs_property_group_content(P), ASettings, Sub);
            O.AddPair('props', Sub);
          end;
      end;
      if O <> nil then
      begin
        O.AddPair('name', Name);
        O.AddPair('label', FromU8(obs_property_description(P)));
        O.AddPair('hint', FromU8(obs_property_long_description(P)));
        O.AddPair('enabled', TJSONBool.Create(obs_property_enabled(P)));
        AOut.AddElement(O);
      end;
    end;
    if not obs_property_next(P) then Break;
  end;
end;

function FilterJson(ACfg: TJSONObject; const AId: string): TJSONObject;
var
  Inst: obs_source_t;
  Props: obs_properties_t;
  Settings: obs_data_t;
  Arr: TJSONArray;
begin
  Result := TJSONObject.Create;
  Result.AddPair('id', AId);
  Result.AddPair('name', FromU8(obs_source_get_display_name(PAnsiChar(U8(AId)))));
  Result.AddPair('enabled', TJSONBool.Create(IsEnabled(ACfg, AId)));
  Arr := TJSONArray.Create;
  Result.AddPair('props', Arr);
  Inst := CreateFilterInstance(AId, SettingsJsonOf(ACfg, AId));
  if Inst = nil then Exit;
  try
    Settings := obs_source_get_settings(Inst);
    Props := obs_source_properties(Inst);
    try
      if (Props <> nil) and (Settings <> nil) then
      begin
        // Roda os callbacks de "modificado" com os valores atuais: e o que
        // decide quais campos aparecem (o OBS faz o mesmo ao abrir a tela).
        obs_properties_apply_settings(Props, Settings);
        AddProps(Props, Settings, Arr);
      end;
    finally
      if Props <> nil then obs_properties_destroy(Props);
      if Settings <> nil then obs_data_release(Settings);
    end;
  finally
    obs_source_release(Inst);
  end;
end;

function BuildAudioFiltersJson: TJSONArray;
var
  Cfg: TJSONObject;
  Id: string;
begin
  Result := TJSONArray.Create;
  Cfg := LoadCfg;
  try
    for Id in RegisteredAudioFilterIds do
      try
        Result.AddElement(FilterJson(Cfg, Id));
      except
        on E: Exception do Log('AudioFilters: %s falhou ao listar: %s', [Id, E.Message]);
      end;
  finally
    Cfg.Free;
  end;
end;

function BuildAudioFilterJson(const AId: string): TJSONObject;
var
  Cfg: TJSONObject;
begin
  Cfg := LoadCfg;
  try
    Result := FilterJson(Cfg, AId);
  finally
    Cfg.Free;
  end;
end;

// ---------------------------------------------------------------------
// Alteracoes
// ---------------------------------------------------------------------

procedure SetAudioFilterEnabled(const AId: string; AEnabled: Boolean);
var
  Cfg, E: TJSONObject;
begin
  Cfg := LoadCfg;
  E := EntryOf(Cfg, AId, True);
  E.RemovePair('enabled').Free;
  E.AddPair('enabled', TJSONBool.Create(AEnabled));
  SetConfigJson(CONFIG_KEY, Cfg);
  Log('AudioFilters: %s %s', [AId, IfThen(AEnabled, 'ligado', 'desligado')]);
end;

procedure SetAudioFilterValue(const AId, AKey: string; AValue: TJSONValue);
// Grava pelo obs_data (e nao direto no JSON) pra o valor ter o TIPO que o
// plugin espera: um 3 que chega da UI num campo float tem que virar double,
// senao o obs_data_get_double do filtro le 0.
var
  Cfg, E: TJSONObject;
  Inst: obs_source_t;
  Settings, Cur: obs_data_t;
  Props: obs_properties_t;
  P: obs_property_t;
  KeyA: UTF8String;
  Kind, Fmt: Integer;
  Json: string;
  NewSettings: TJSONValue;
begin
  if (AValue = nil) or (AKey = '') then Exit;
  Cfg := LoadCfg;
  try
    E := EntryOf(Cfg, AId, True);
    Inst := CreateFilterInstance(AId, SettingsJsonOf(Cfg, AId));
    if Inst = nil then Exit;
    try
      KeyA := U8(AKey);
      Props := obs_source_properties(Inst);
      try
        P := nil;
        if Props <> nil then P := obs_properties_get(Props, PAnsiChar(KeyA));
        if P = nil then
        begin
          Log('AudioFilters: %s.%s nao existe.', [AId, AKey]);
          Exit;
        end;
        Kind := obs_property_get_type(P);
        Fmt := 0;
        if Kind = OBS_PROPERTY_LIST then Fmt := obs_property_list_format(P);
      finally
        if Props <> nil then obs_properties_destroy(Props);
      end;

      // Parte do que JA estava salvo (so os valores do usuario, sem os
      // defaults) — assim o config nao congela os defaults do plugin.
      Cur := obs_data_create_from_json(PAnsiChar(U8(SettingsJsonOf(Cfg, AId))));
      if Cur = nil then Cur := obs_data_create;
      try
        if (Kind = OBS_PROPERTY_BOOL) and (AValue is TJSONBool) then
          obs_data_set_bool(Cur, PAnsiChar(KeyA), ByteBool(TJSONBool(AValue).AsBoolean))
        else if (Kind = OBS_PROPERTY_INT) or
                ((Kind = OBS_PROPERTY_LIST) and (Fmt = OBS_COMBO_FORMAT_INT)) then
        begin
          if AValue is TJSONNumber then
            obs_data_set_int(Cur, PAnsiChar(KeyA), Round(TJSONNumber(AValue).AsDouble));
        end
        else if (Kind = OBS_PROPERTY_FLOAT) or
                ((Kind = OBS_PROPERTY_LIST) and (Fmt = OBS_COMBO_FORMAT_FLOAT)) then
        begin
          if AValue is TJSONNumber then
            obs_data_set_double(Cur, PAnsiChar(KeyA), TJSONNumber(AValue).AsDouble);
        end
        else if (Kind = OBS_PROPERTY_LIST) and (Fmt = OBS_COMBO_FORMAT_STRING) then
          obs_data_set_string(Cur, PAnsiChar(KeyA), PAnsiChar(U8(AValue.Value)))
        else
        begin
          Log('AudioFilters: %s.%s tipo %d nao editavel.', [AId, AKey, Kind]);
          Exit;
        end;

        // Alguns campos mexem em outros (preset do expansor preenche razao,
        // limiar...): passa pela instancia pra os callbacks rodarem e guarda
        // o que ficou.
        obs_source_update(Inst, Cur);
        Settings := obs_source_get_settings(Inst);
        Props := obs_source_properties(Inst);
        try
          if (Props <> nil) and (Settings <> nil) then
          begin
            P := obs_properties_get(Props, PAnsiChar(KeyA));
            if P <> nil then obs_property_modified(P, Settings);
            obs_data_apply(Cur, Settings);
          end;
        finally
          if Props <> nil then obs_properties_destroy(Props);
          if Settings <> nil then obs_data_release(Settings);
        end;

        Json := FromU8(obs_data_get_json(Cur));
      finally
        obs_data_release(Cur);
      end;
    finally
      obs_source_release(Inst);
    end;

    NewSettings := TJSONObject.ParseJSONValue(Json);
    if not (NewSettings is TJSONObject) then
    begin
      NewSettings.Free;
      Exit;
    end;
    E.RemovePair('settings').Free;
    E.AddPair('settings', NewSettings);
    SetConfigJson(CONFIG_KEY, Cfg);
    Cfg := nil;   // SetConfigJson assumiu a posse
  finally
    Cfg.Free;
  end;
end;

procedure ResetAllAudioFilters;
begin
  SetConfigJson(CONFIG_KEY, TJSONObject.Create);
end;

procedure ResetAudioFilter(const AId: string);
var
  Cfg, E: TJSONObject;
begin
  Cfg := LoadCfg;
  E := EntryOf(Cfg, AId, True);
  E.RemovePair('settings').Free;
  E.AddPair('settings', TJSONObject.Create);
  SetConfigJson(CONFIG_KEY, Cfg);
end;

function EnabledAudioFilterCount: Integer;
var
  Cfg: TJSONObject;
  Id: string;
begin
  Result := 0;
  Cfg := LoadCfg;
  try
    for Id in RegisteredAudioFilterIds do
      if IsEnabled(Cfg, Id) then Inc(Result);
  finally
    Cfg.Free;
  end;
end;

procedure ApplyAudioFilters(ASource: obs_source_t);
var
  Cfg: TJSONObject;
  Id: string;
  F: obs_source_t;
  Names: string;
begin
  if ASource = nil then Exit;
  Cfg := LoadCfg;
  try
    Names := '';
    // obs_source_filter_add insere na FRENTE da lista e o audio percorre a
    // lista de tras pra frente — ou seja, o primeiro adicionado e o
    // primeiro a processar. Adicionar na ordem da cadeia = cadeia certa.
    for Id in RegisteredAudioFilterIds do
    begin
      if not IsEnabled(Cfg, Id) then Continue;
      F := CreateFilterInstance(Id, SettingsJsonOf(Cfg, Id));
      if F = nil then
      begin
        Log('AudioFilters: nao consegui criar %s.', [Id]);
        Continue;
      end;
      obs_source_filter_add(ASource, F);
      obs_source_release(F);   // o filter_add pegou a propria referencia
      if Names <> '' then Names := Names + ', ';
      Names := Names + Id;
    end;
    if Names <> '' then Log('   filtros: %s', [Names]);
  finally
    Cfg.Free;
  end;
end;

procedure SetObsLocaleFromApp(const ALang: string);
var
  L, Loc: string;
begin
  L := LowerCase(Trim(ALang));
  if Pos('-', L) > 0 then Loc := ALang
  else if L = 'pt' then Loc := 'pt-BR'
  else if L = 'es' then Loc := 'es-ES'
  else if L = 'fr' then Loc := 'fr-FR'
  else if L = 'de' then Loc := 'de-DE'
  else if L = 'it' then Loc := 'it-IT'
  else if L = 'ja' then Loc := 'ja-JP'
  else if L = 'ko' then Loc := 'ko-KR'
  else if L = 'zh' then Loc := 'zh-CN'
  else if L = 'ru' then Loc := 'ru-RU'
  else Loc := 'en-US';
  try
    obs_set_locale(PAnsiChar(U8(Loc)));
  except
    on E: Exception do Log('AudioFilters: obs_set_locale(%s) falhou: %s', [Loc, E.Message]);
  end;
end;

// ---------------------------------------------------------------------
// Teste A/B: grava o ORIGINAL uma vez e refaz a versao filtrada a cada
// mudanca, sem gravar de novo.
//
// O trecho cru fica guardado (GClipL/GClipR, float estereo no formato do
// audio do OBS). Pra gerar uma versao, ele e EMPURRADO bloco a bloco
// (obs_source_output_audio) numa fonte nossa sem som proprio
// (noobs_audio_feed) com a cadeia de filtros pendurada; o callback de
// captura devolve o resultado. Tudo sincrono, na main thread: o
// obs_source_output_audio roda os filtros e o callback antes de voltar.
// Medido: 4 s de audio refeitos em 40 ms (ganho) a ~110 ms (RNNoise).
//
// A versao ORIGINAL passa pelo mesmo caminho, so que sem filtros — sai
// identica ao capturado (medido), e as duas ficam com o mesmo formato.
//
// Filtro com estado (porta, compressor, supressao) comeca do zero a cada
// versao: fonte nova, filtros novos. Os primeiros milissegundos podem
// soar um pouco diferente de uma gravacao continua.
// ---------------------------------------------------------------------

const
  FEED_SOURCE_ID: AnsiString = 'noobs_audio_feed';
  FEED_CHUNK = 1024;        // AUDIO_OUTPUT_FRAMES do OBS
  // Silencio empurrado depois do trecho: filtro com atraso (a supressao de
  // ruido guarda uns milissegundos) so devolve o fim do trecho se receber
  // mais audio depois dele.
  FEED_TAIL_SEC = 0.25;

type
  // struct obs_source_info (obs-source.h), SO os campos iniciais. O
  // obs_register_source_s recebe o TAMANHO e zera o resto — o layout que
  // importa e so este prefixo (8 + 4 + 4 + 3 ponteiros = 40 bytes).
  TObsSourceInfoHead = record
    id: PAnsiChar;
    type_: Integer;            // OBS_SOURCE_TYPE_INPUT = 0
    output_flags: Cardinal;
    get_name: Pointer;
    create: Pointer;
    destroy: Pointer;          // chamado sempre que o create devolveu data
  end;

  // Saida do callback: mono 16 bits pro WAV, com pico e soma dos quadrados.
  TTestCapture = class
    Lock: TCriticalSection;
    Samples: TList<SmallInt>;
    MaxSamples: Integer;
    Peak: Single;
    SumSq: Double;
    constructor Create(AMax: Integer);
    destructor Destroy; override;
  end;

  // Captura do microfone cru: float estereo, como o OBS entrega.
  TRawCapture = class
    Lock: TCriticalSection;
    L, R: TList<Single>;
    MaxSamples: Integer;
    constructor Create(AMax: Integer);
    destructor Destroy; override;
  end;

var
  GRawSrc: obs_source_t;
  GRawCap: TRawCapture;
  GClipL, GClipR: TArray<Single>;
  GFeedRegistered: Boolean;
  GFeedDummy: Integer;   // o "data" da fonte: so precisa ser nao-nulo

constructor TTestCapture.Create(AMax: Integer);
begin
  inherited Create;
  Lock := TCriticalSection.Create;
  Samples := TList<SmallInt>.Create;
  Samples.Capacity := AMax;
  MaxSamples := AMax;
end;

destructor TTestCapture.Destroy;
begin
  Samples.Free;
  Lock.Free;
  inherited;
end;

constructor TRawCapture.Create(AMax: Integer);
begin
  inherited Create;
  Lock := TCriticalSection.Create;
  L := TList<Single>.Create;
  R := TList<Single>.Create;
  L.Capacity := AMax;
  R.Capacity := AMax;
  MaxSamples := AMax;
end;

destructor TRawCapture.Destroy;
begin
  L.Free;
  R.Free;
  Lock.Free;
  inherited;
end;

// ---- fonte noobs_audio_feed ----

function FeedGetName(type_data: Pointer): PAnsiChar; cdecl;
begin
  Result := 'NoOBS audio feed';
end;

function FeedCreate(settings: obs_data_t; source: obs_source_t): Pointer; cdecl;
begin
  // Sem data a libobs loga "Failed to create source" e marca a fonte como
  // quebrada; o conteudo nao importa, a fonte nao faz nada sozinha.
  Result := @GFeedDummy;
end;

procedure FeedDestroy(data: Pointer); cdecl;
begin
end;

function EnsureFeedRegistered: Boolean;
var
  Info: TObsSourceInfoHead;
begin
  if not GFeedRegistered then
  begin
    // Uma vez por vida do libobs (que no NoOBS e a vida do processo). O
    // id e copiado (bstrdup) no registro.
    if obs_get_source_output_flags(PAnsiChar(FEED_SOURCE_ID)) = 0 then
    begin
      FillChar(Info, SizeOf(Info), 0);
      Info.id := PAnsiChar(FEED_SOURCE_ID);
      Info.type_ := 0;
      // So AUDIO: e o que deixa o obs_source_filter_add aceitar filtro de
      // audio (filter_compatible compara as flags).
      Info.output_flags := OBS_SOURCE_AUDIO;
      Info.get_name := @FeedGetName;
      Info.create := @FeedCreate;
      Info.destroy := @FeedDestroy;
      obs_register_source_s(@Info, SizeOf(Info));
    end;
    GFeedRegistered := obs_get_source_output_flags(PAnsiChar(FEED_SOURCE_ID)) <> 0;
    if not GFeedRegistered then
      Log('AudioFilters: nao consegui registrar a fonte %s.', [string(FEED_SOURCE_ID)]);
  end;
  Result := GFeedRegistered;
end;

// ---- callbacks de captura ----

// Saida de uma versao (original ou filtrada), ja DEPOIS dos filtros.
// Mistura os canais em mono (o microfone e mono; o OBS so o espalha).
procedure TestCaptureCb(param: Pointer; source: obs_source_t;
  audio: PObsAudioData; muted: ByteBool); cdecl;
var
  Cap: TTestCapture;
  L, R: PSingle;
  i: Integer;
  V: Single;
begin
  Cap := TTestCapture(param);
  if (Cap = nil) or (audio = nil) or (audio.data[0] = nil) then Exit;
  L := PSingle(audio.data[0]);
  R := PSingle(audio.data[1]);
  Cap.Lock.Enter;
  try
    for i := 0 to Integer(audio.frames) - 1 do
    begin
      if Cap.Samples.Count >= Cap.MaxSamples then Break;
      if R <> nil then
      begin
        V := (L^ + R^) * 0.5;
        Inc(R);
      end
      else
        V := L^;
      Inc(L);
      if Abs(V) > Cap.Peak then Cap.Peak := Abs(V);
      Cap.SumSq := Cap.SumSq + V * V;
      if V > 1 then V := 1 else if V < -1 then V := -1;
      Cap.Samples.Add(SmallInt(Round(V * 32767)));
    end;
  finally
    Cap.Lock.Leave;
  end;
end;

// Microfone cru, na thread de captura do WASAPI: so copia sob lock.
procedure RawCaptureCb(param: Pointer; source: obs_source_t;
  audio: PObsAudioData; muted: ByteBool); cdecl;
var
  Cap: TRawCapture;
  L, R: PSingle;
  i: Integer;
begin
  Cap := TRawCapture(param);
  if (Cap = nil) or (audio = nil) or (audio.data[0] = nil) then Exit;
  L := PSingle(audio.data[0]);
  R := PSingle(audio.data[1]);
  if R = nil then R := L;
  Cap.Lock.Enter;
  try
    for i := 0 to Integer(audio.frames) - 1 do
    begin
      if Cap.L.Count >= Cap.MaxSamples then Break;
      Cap.L.Add(L^);
      Cap.R.Add(R^);
      Inc(L);
      Inc(R);
    end;
  finally
    Cap.Lock.Leave;
  end;
end;

function AudioFilterTestRunning: Boolean;
begin
  Result := GRawSrc <> nil;
end;

function HasAudioFilterClip: Boolean;
begin
  Result := Length(GClipL) > 0;
end;

procedure ReleaseTest;
begin
  if GRawSrc <> nil then
  begin
    try obs_source_remove_audio_capture_callback(GRawSrc, RawCaptureCb, GRawCap); except end;
    try obs_source_release(GRawSrc); except end;
    GRawSrc := nil;
  end;
end;

function StartAudioFilterTest(const ADeviceId: string; out AErr: string): Boolean;
var
  Settings: obs_data_t;
  Dev: string;
begin
  Result := False;
  AErr := '';
  if AudioFilterTestRunning then
  begin
    AErr := 'busy';
    Exit;
  end;
  FreeAndNil(GRawCap);
  GRawCap := TRawCapture.Create(SAMPLE_RATE * (MAX_TEST_SEC + 1));
  try
    Dev := ADeviceId;
    if Dev = '' then Dev := 'default';
    Settings := obs_data_create;
    try
      obs_data_set_string(Settings, 'device_id', PAnsiChar(U8(Dev)));
      GRawSrc := obs_source_create_private('wasapi_input_capture',
        'NoOBS teste de filtros', Settings);
    finally
      obs_data_release(Settings);
    end;
    if GRawSrc = nil then
    begin
      AErr := 'source';
      FreeAndNil(GRawCap);
      Exit;
    end;
    obs_source_add_audio_capture_callback(GRawSrc, RawCaptureCb, GRawCap);
    Log('AudioFilters: teste gravando (device=%s).',
      [IfThen(ADeviceId = '', 'default', ADeviceId)]);
    Result := True;
  except
    on E: Exception do
    begin
      AErr := E.Message;
      ReleaseTest;
      FreeAndNil(GRawCap);
    end;
  end;
end;

function MakeWav(ACap: TTestCapture; out APeakDb, ARmsDb: Double): TBytes;
// RIFF/WAVE PCM 16 bits mono. Cabecalho de 44 bytes, little-endian.
var
  N, DataBytes: Integer;
  S: TMemoryStream;

  procedure W32(V: Cardinal); begin S.WriteBuffer(V, 4); end;
  procedure W16(V: Word); begin S.WriteBuffer(V, 2); end;
  procedure WStr(const A: AnsiString); begin S.WriteBuffer(PAnsiChar(A)^, Length(A)); end;

begin
  SetLength(Result, 0);
  APeakDb := -100;
  ARmsDb := -100;
  if ACap = nil then Exit;
  ACap.Lock.Enter;
  try
    N := ACap.Samples.Count;
    if ACap.Peak > 0.00001 then APeakDb := 20 * Log10(ACap.Peak);
    if (N > 0) and (ACap.SumSq / N > 1E-10) then
      ARmsDb := 10 * Log10(ACap.SumSq / N);
    DataBytes := N * 2;
    S := TMemoryStream.Create;
    try
      WStr('RIFF'); W32(36 + DataBytes); WStr('WAVE');
      WStr('fmt '); W32(16); W16(1); W16(1);
      W32(SAMPLE_RATE); W32(SAMPLE_RATE * 2); W16(2); W16(16);
      WStr('data'); W32(DataBytes);
      if N > 0 then
        S.WriteBuffer(ACap.Samples.List[0], DataBytes);
      SetLength(Result, S.Size);
      Move(S.Memory^, Result[0], S.Size);
    finally
      S.Free;
    end;
  finally
    ACap.Lock.Leave;
  end;
end;

function RenderClip(AWithFilters: Boolean; out APeakDb, ARmsDb: Double): TBytes;
// Passa o trecho guardado pela fonte noobs_audio_feed (com ou sem a cadeia
// de filtros) e devolve o WAV do que saiu.
var
  Src: obs_source_t;
  Cap: TTestCapture;
  Audio: TObsSourceAudio;
  BufL, BufR: TArray<Single>;
  N, Total, i, j, k: Integer;
  Ts: UInt64;
begin
  SetLength(Result, 0);
  APeakDb := -100;
  ARmsDb := -100;
  N := Length(GClipL);
  if (N = 0) or not EnsureFeedRegistered then Exit;
  Src := obs_source_create_private(PAnsiChar(FEED_SOURCE_ID), 'NoOBS teste', nil);
  if Src = nil then Exit;
  // MaxSamples = tamanho do original: o que sobra (atraso do filtro + o
  // silencio da cauda) fica de fora e as duas versoes tem o mesmo tamanho.
  Cap := TTestCapture.Create(N);
  try
    if AWithFilters then ApplyAudioFilters(Src);
    obs_source_add_audio_capture_callback(Src, TestCaptureCb, Cap);
    try
      Total := N + Round(FEED_TAIL_SEC * SAMPLE_RATE);
      SetLength(BufL, FEED_CHUNK);
      SetLength(BufR, FEED_CHUNK);
      Ts := os_gettime_ns;
      i := 0;
      while i < Total do
      begin
        k := Min(FEED_CHUNK, Total - i);
        for j := 0 to k - 1 do
          if i + j < N then
          begin
            BufL[j] := GClipL[i + j];
            BufR[j] := GClipR[i + j];
          end
          else
          begin
            BufL[j] := 0;
            BufR[j] := 0;
          end;
        FillChar(Audio, SizeOf(Audio), 0);
        Audio.data[0] := @BufL[0];
        Audio.data[1] := @BufR[0];
        Audio.frames := k;
        Audio.speakers := SPEAKERS_STEREO;
        Audio.format := AUDIO_FORMAT_FLOAT_PLANAR;
        Audio.samples_per_sec := SAMPLE_RATE;
        // Relogio continuo: salto de timestamp faz a libobs reiniciar o
        // alinhamento da fonte.
        Audio.timestamp := Ts + (UInt64(i) * 1000000000) div SAMPLE_RATE;
        obs_source_output_audio(Src, @Audio);
        Inc(i, k);
      end;
    finally
      obs_source_remove_audio_capture_callback(Src, TestCaptureCb, Cap);
    end;
    Result := MakeWav(Cap, APeakDb, ARmsDb);
  finally
    obs_source_release(Src);
    Cap.Free;
  end;
end;

procedure FinishAudioFilterTest(out ARawWav, AFiltWav: TBytes;
  out ARawPeakDb, AFiltPeakDb, ARawRmsDb, AFiltRmsDb: Double);
begin
  // Solta a fonte ANTES de ler: com o callback removido nada mais escreve.
  ReleaseTest;
  SetLength(GClipL, 0);
  SetLength(GClipR, 0);
  if GRawCap <> nil then
  begin
    GRawCap.Lock.Enter;
    try
      GClipL := GRawCap.L.ToArray;
      GClipR := GRawCap.R.ToArray;
    finally
      GRawCap.Lock.Leave;
    end;
    FreeAndNil(GRawCap);
  end;
  ARawWav := RenderClip(False, ARawPeakDb, ARawRmsDb);
  AFiltWav := RenderClip(True, AFiltPeakDb, AFiltRmsDb);
  Log('AudioFilters: teste gravado (%d amostras, %d filtro(s); original: media %.1f / pico %.1f dB; com filtros: media %.1f / pico %.1f dB).',
    [Length(GClipL), EnabledAudioFilterCount, ARawRmsDb, ARawPeakDb,
     AFiltRmsDb, AFiltPeakDb]);
end;

function RerenderAudioFilterTest(out AFiltWav: TBytes;
  out AFiltPeakDb, AFiltRmsDb: Double): Boolean;
begin
  Result := HasAudioFilterClip and not AudioFilterTestRunning;
  if not Result then Exit;
  AFiltWav := RenderClip(True, AFiltPeakDb, AFiltRmsDb);
  Result := Length(AFiltWav) > 0;
end;

procedure CancelAudioFilterTest;
begin
  ReleaseTest;
  FreeAndNil(GRawCap);
  SetLength(GClipL, 0);
  SetLength(GClipR, 0);
end;

end.
