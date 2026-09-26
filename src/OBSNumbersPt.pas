(*
  OBSNumbersPt - numeros por extenso em portugues viram algarismos.

  O Qwen3-ASR (transcricao local, OBSLocalAsr) escreve os numeros por
  extenso ("setenta"), o que alonga a legenda. Segue a convencao comum em
  legendagem: de zero a dez por extenso, acima disso em algarismos. Manter
  os pequenos por extenso tambem evita converter o artigo "um".

  O "e" so liga uma parte a outra MENOR, como na fala: dezena a unidade
  ("vinte e cinco"), centena a dezena ("cento e dez"), milhar ao resto
  ("dois mil e vinte"). Assim "dois e tres" continua sendo dois numeros, e
  nao vira 5.

  Traducao direta do numbers_pt.py da Transcritor API (versao Windows),
  validado la contra 19 frases reais. Mudou um, mude o outro.

  O arquivo e UTF-8 COM BOM de proposito: as palavras tem acento ("tres",
  "milhao"), e sem o BOM o compilador le o literal na pagina de codigo do
  sistema.
*)
unit OBSNumbersPt;

interface

// Troca por algarismos as frases numericas acima de AKeepUpTo.
// "dois mil e vinte e seis," -> "2026,"; "trinta e cinco por cento" -> "35%".
function ToDigits(const AText: string; AKeepUpTo: Integer = 10): string;

implementation

uses
  System.SysUtils,
  System.Generics.Collections;

type
  TNumKind = (nkNone, nkUnit, nkTeen, nkTen, nkHundred, nkScale);
  TNumWord = record
    Kind: TNumKind;
    Value: Int64;
  end;

const
  // Pontuacao que pode vir colada na palavra, antes e depois. A palavra
  // e casada SEM ela, e ela volta intacta em volta do numero.
  LEAD_CHARS  = '«"''([';
  TRAIL_CHARS = '.,;:!?»"'')]…';
  // Pontuacao que ENCERRA a frase numerica ("vinte, trinta" sao dois).
  STOP_CHARS  = '.,;:!?…';

var
  Words: TDictionary<string, TNumWord> = nil;

procedure AddWords(const AList: array of string; AKind: TNumKind;
  const AValues: array of Int64);
var
  i: Integer;
  W: TNumWord;
begin
  for i := 0 to High(AList) do
  begin
    W.Kind := AKind;
    W.Value := AValues[i];
    Words.AddOrSetValue(AList[i], W);
  end;
end;

procedure BuildWords;
begin
  Words := TDictionary<string, TNumWord>.Create;
  AddWords(['zero', 'um', 'uma', 'dois', 'duas', 'três', 'tres', 'quatro',
    'cinco', 'seis', 'sete', 'oito', 'nove'], nkUnit,
    [0, 1, 1, 2, 2, 3, 3, 4, 5, 6, 7, 8, 9]);
  AddWords(['dez', 'onze', 'doze', 'treze', 'quatorze', 'catorze', 'quinze',
    'dezesseis', 'dezasseis', 'dezessete', 'dezassete', 'dezoito', 'dezenove',
    'dezanove'], nkTeen,
    [10, 11, 12, 13, 14, 14, 15, 16, 16, 17, 17, 18, 19, 19]);
  AddWords(['vinte', 'trinta', 'quarenta', 'cinquenta', 'sessenta', 'setenta',
    'oitenta', 'noventa'], nkTen,
    [20, 30, 40, 50, 60, 70, 80, 90]);
  AddWords(['cem', 'cento', 'duzentos', 'duzentas', 'trezentos', 'trezentas',
    'quatrocentos', 'quatrocentas', 'quinhentos', 'quinhentas', 'seiscentos',
    'seiscentas', 'setecentos', 'setecentas', 'oitocentos', 'oitocentas',
    'novecentos', 'novecentas'], nkHundred,
    [100, 100, 200, 200, 300, 300, 400, 400, 500, 500, 600, 600, 700, 700,
     800, 800, 900, 900]);
  AddWords(['mil', 'milhão', 'milhões', 'bilhão', 'bilhões'], nkScale,
    [1000, 1000000, 1000000, 1000000000, 1000000000]);
end;

procedure SplitToken(const AWord: string; out ALead, ACore, ATrail: string);
// Mesma regra do regex do Python: pontuacao inicial gulosa, depois a
// maior sequencia de pontuacao final possivel, e o meio e a palavra.
var
  a, b: Integer;
begin
  a := 1;
  while (a <= Length(AWord)) and (Pos(AWord[a], LEAD_CHARS) > 0) do Inc(a);
  b := Length(AWord);
  while (b >= a) and (Pos(AWord[b], TRAIL_CHARS) > 0) do Dec(b);
  ALead := Copy(AWord, 1, a - 1);
  ACore := Copy(AWord, a, b - a + 1);
  ATrail := Copy(AWord, b + 1, MaxInt);
end;

function CoreOf(const AWord: string): string;
var
  L, T: string;
begin
  SplitToken(AWord, L, Result, T);
end;

function LeadOf(const AWord: string): string;
var
  C, T: string;
begin
  SplitToken(AWord, Result, C, T);
end;

function TrailOf(const AWord: string): string;
var
  L, C: string;
begin
  SplitToken(AWord, L, C, Result);
end;

function KindOf(const ACore: string): TNumWord;
begin
  if not Words.TryGetValue(AnsiLowerCase(ACore), Result) then
  begin
    Result.Kind := nkNone;
    Result.Value := 0;
  end;
end;

function AfterELimit(AKind: TNumKind): Int64;
// Depois de cada tipo de parte, o maior valor que pode vir ligado por "e".
// 0 = esse tipo nao aceita "e" depois (unidade, dezena de 10 a 19).
begin
  case AKind of
    nkTen:     Result := 10;
    nkHundred: Result := 100;
    nkScale:   Result := 1000;
  else
    Result := 0;
  end;
end;

function ParseNumber(const W: TArray<string>; AFrom: Integer;
  out AValue: Int64; out AUsed: Integer): Boolean;
// Le a maior frase numerica a partir de W[AFrom]. AUsed = palavras lidas.
var
  Total, Group: Int64;
  Last: TNumKind;
  i: Integer;
  Lead, Core, Trail, L2, Nxt, T2: string;
  Cur, NextW: TNumWord;
  Connector: Boolean;
begin
  Total := 0;
  Group := 0;
  AUsed := 0;
  Last := nkNone;
  i := AFrom;
  while i < Length(W) do
  begin
    SplitToken(W[i], Lead, Core, Trail);
    Cur := KindOf(Core);
    Connector := False;
    if (Cur.Kind = nkNone) and (AnsiLowerCase(Core) = 'e') and
       (AfterELimit(Last) > 0) and (i + 1 < Length(W)) then
    begin
      SplitToken(W[i + 1], L2, Nxt, T2);
      NextW := KindOf(Nxt);
      if (NextW.Kind <> nkNone) and (NextW.Kind <> nkScale) and
         (NextW.Value < AfterELimit(Last)) then
      begin
        Connector := True;
        Inc(i);
        Trail := T2;
        Cur := NextW;
      end;
    end;
    if Cur.Kind = nkNone then Break;
    // Sem "e", so valem: parte seguida de escala ("dois mil") ou escala
    // seguida de centena/dezena/unidade ("mil novecentos").
    if (not Connector) and (Last <> nkNone) and
       not ((Cur.Kind = nkScale) or (Last = nkScale)) then Break;
    if Cur.Kind = nkScale then
    begin
      // "mil milhoes" e "milhao" sem numero antes ficam fora do escopo.
      if (Last = nkScale) or ((Last = nkNone) and (Cur.Value > 1000)) then Break;
      if Group = 0 then Total := Total + Cur.Value
      else Total := Total + Group * Cur.Value;
      Group := 0;
    end
    else
      Group := Group + Cur.Value;
    AUsed := i + 1 - AFrom;
    Last := Cur.Kind;
    Inc(i);
    // Pontuacao encerra a frase numerica.
    if (Trail <> '') and (Pos(Trail[Length(Trail)], STOP_CHARS) > 0) then Break;
  end;
  AValue := Total + Group;
  Result := AUsed > 0;
end;

function GroupThousands(AValue: Int64): string;
// 1234567 -> "1.234.567" (separador de milhar do portugues).
var
  S: string;
  n: Integer;
begin
  S := IntToStr(AValue);
  Result := '';
  n := 0;
  while S <> '' do
  begin
    if (n > 0) and (n mod 3 = 0) then Result := '.' + Result;
    Result := S[Length(S)] + Result;
    Delete(S, Length(S), 1);
    Inc(n);
  end;
end;

function Render(AValue: Int64): string;
// Milhao e bilhao redondos ficam com a palavra: "3 milhoes" le melhor
// que "3.000.000". O resto vira algarismo, com ponto a partir de 10.000.
const
  SCALES: array[0..1] of Int64 = (1000000, 1000000000);
  SINGULAR: array[0..1] of string = ('milhão', 'bilhão');
  PLURAL: array[0..1] of string = ('milhões', 'bilhões');
var
  k: Integer;
  n: Int64;
begin
  for k := 0 to High(SCALES) do
    if (AValue >= SCALES[k]) and (AValue mod SCALES[k] = 0) and
       (AValue < SCALES[k] * 1000) then
    begin
      n := AValue div SCALES[k];
      if n = 1 then Exit(IntToStr(n) + ' ' + SINGULAR[k]);
      Exit(IntToStr(n) + ' ' + PLURAL[k]);
    end;
  if AValue >= 10000 then Exit(GroupThousands(AValue));
  Result := IntToStr(AValue);
end;

function ToDigits(const AText: string; AKeepUpTo: Integer): string;
var
  W: TArray<string>;
  Parts: TList<string>;
  i, k, Used: Integer;
  Value: Int64;
  Rendered, Trail: string;
  SingleMil: Boolean;
begin
  // Split por espaco UNICO, como o Python: espacos seguidos viram
  // palavras vazias e o Join devolve o texto com o mesmo espacamento.
  W := AText.Split([' ']);
  Parts := TList<string>.Create;
  try
    i := 0;
    while i < Length(W) do
    begin
      if (W[i] = '') or not ParseNumber(W, i, Value, Used) then
      begin
        Parts.Add(W[i]);
        Inc(i);
        Continue;
      end;
      Trail := TrailOf(W[i + Used - 1]);
      SingleMil := (Used = 1) and (AnsiLowerCase(CoreOf(W[i])) = 'mil');
      if (Value <= AKeepUpTo) or SingleMil then
      begin
        // Pequenos e "mil" sozinho ficam por extenso.
        for k := i to i + Used - 1 do Parts.Add(W[k]);
      end
      else
      begin
        Rendered := LeadOf(W[i]) + Render(Value);
        if (Trail = '') and (i + Used + 1 < Length(W)) and
           (AnsiLowerCase(W[i + Used]) = 'por') and
           (AnsiLowerCase(CoreOf(W[i + Used + 1])) = 'cento') then
        begin
          Parts.Add(Rendered + '%' + TrailOf(W[i + Used + 1]));
          Inc(i, Used + 2);
          Continue;
        end;
        Parts.Add(Rendered + Trail);
      end;
      Inc(i, Used);
    end;
    Result := string.Join(' ', Parts.ToArray);
  finally
    Parts.Free;
  end;
end;

initialization
  // Montado aqui, e nao na primeira chamada: a transcricao roda em worker
  // thread, e montar sob demanda seria uma corrida se um dia houver duas.
  BuildWords;

finalization
  Words.Free;

end.
