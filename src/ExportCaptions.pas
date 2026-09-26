(*
  ExportCaptions - legenda "queimada" no video exportado.

  A transcricao (turnos: inicio, fim, texto) vira texto desenhado DENTRO
  dos quadros de saida, no mesmo estilo da legenda do player: branco,
  negrito, contorno escuro, centralizado embaixo, no maximo DUAS linhas por
  vez. Um turno tem ate 20 s de fala (pegadinha #60k) — em letra de
  legenda sao cinco, seis linhas —, entao cada turno e partido em BLOCOS
  que cabem em duas linhas, medidos com a fonte e a largura REAIS da
  saida, e o tempo do turno e dividido entre os blocos na proporcao dos
  caracteres. E a mesma regra do Player._captionChunks, so que medida aqui
  no tamanho do arquivo final, nao no da janela.

  Por que GDI e nao filtro do libav: as DLLs empacotadas pelo OBS nao
  trazem libavfilter/libass bindados no projeto, e o texto e simples
  (sem estilo por palavra). O GDI rasteriza em escala de cinza, o
  contorno sai de uma dilatacao da cobertura, e a mistura e feita direto
  nos planos YUV 4:2:0 do quadro — sem converter o quadro pra RGB.

  Roda na thread da exportacao. Os objetos GDI (DC, fonte, DIB) sao
  criados e usados na MESMA thread, que e o que o GDI exige.
*)
unit ExportCaptions;

interface

uses
  Winapi.Windows,
  System.SysUtils,
  FFmpegLib;

type
  // Um turno da transcricao, no relogio da ORIGEM (segundos).
  TCaptionTurn = record
    StartSec, EndSec: Double;
    Text: string;
  end;
  TCaptionTurnArray = TArray<TCaptionTurn>;

  TCaptionBurner = class
  private type
    TChunk = record
      StartSec, EndSec: Double;
      Text: string;
    end;
  private
    FOutW, FOutH: Integer;
    FFontPx, FLineH, FMaxTextW, FRadius, FPad, FBottom: Integer;
    FChunks: TArray<TChunk>;
    FDC: HDC;
    FFont: HFONT;
    FBmp: HBITMAP;
    FOldFont, FOldBmp: HGDIOBJ;
    FBits: PByte;         // DIB 32 bits, top-down
    FBmpW, FBmpH: Integer;
    // Bloco rasterizado em cache: so o CORRENTE. A legenda anda em ordem,
    // e guardar todos custaria centenas de MB numa transcricao longa.
    FCurIdx: Integer;
    FCurX, FCurY, FCurW, FCurH: Integer;
    FAlpha: TBytes;       // cobertura final 0..255 (texto + contorno)
    FLuma: TBytes;        // Y do pixel da legenda (branco no texto, preto no contorno)
    FChromaA: TBytes;     // alfa medio do bloco 2x2, pra U/V
    function MeasureHeight(const AText: string): Integer;
    procedure BuildChunks(const ATurns: TCaptionTurnArray);
    function FindChunk(ASec: Double): Integer;
    procedure Rasterize(AIdx: Integer);
  public
    constructor Create(AOutW, AOutH: Integer; const ATurns: TCaptionTurnArray);
    destructor Destroy; override;
    function ChunkCount: Integer;
    // Desenha no quadro (YUV420P, AOutW x AOutH) o bloco que estiver no ar
    // no instante ASrcSec da origem. Sem bloco no ar, nao faz nada.
    procedure BlendAt(ASrcSec: Double; AFrame: PAVFrame);
  end;

implementation

uses
  System.Types,     // Rect() — a Winapi.Windows traz o TRect, mas nao a funcao
  System.Math,
  System.Generics.Collections,
  System.Generics.Defaults,
  OBSLog;

const
  // Tamanho da letra em fracao da ALTURA do quadro de saida. 4,5% da altura
  // e ~48 px em 1080p — legenda de TV. Pela altura, e nao pela largura:
  // num canvas de dois monitores lado a lado a largura dobra e a letra
  // ficaria enorme.
  CAPTION_FONT_FRACTION = 0.045;
  CAPTION_FONT_MIN = 14;
  CAPTION_FONT_MAX = 110;
  // Largura maxima do texto: 86% do quadro, e no maximo ~32 letras de
  // largura — as mesmas proporcoes do .player-caption (min(86%, 960px) com
  // fonte de 30 px). Linha comprida demais se le mal em qualquer tela.
  CAPTION_WIDTH_FRACTION = 0.86;
  CAPTION_WIDTH_EMS = 32;
  // Distancia da borda de baixo, em fracao da altura.
  CAPTION_BOTTOM_FRACTION = 0.06;
  // Y em faixa limitada (BT.601/709 "TV range"), que e o que os encoders
  // do projeto recebem em YUV420P.
  Y_WHITE = 235;
  Y_BLACK = 16;

{$POINTERMATH ON}

constructor TCaptionBurner.Create(AOutW, AOutH: Integer;
  const ATurns: TCaptionTurnArray);
var
  Bmi: TBitmapInfo;
  Tm: TTextMetric;
  Bits: Pointer;
begin
  inherited Create;
  FOutW := AOutW;
  FOutH := AOutH;
  FCurIdx := -1;
  FFontPx := EnsureRange(Round(AOutH * CAPTION_FONT_FRACTION),
    CAPTION_FONT_MIN, CAPTION_FONT_MAX);
  FMaxTextW := Min(Round(AOutW * CAPTION_WIDTH_FRACTION),
    FFontPx * CAPTION_WIDTH_EMS);
  // Contorno proporcional a letra: ~1/14 da altura dela.
  FRadius := Max(2, FFontPx div 14);
  FPad := FRadius + 2;
  FBottom := Round(AOutH * CAPTION_BOTTOM_FRACTION);

  FDC := CreateCompatibleDC(0);
  if FDC = 0 then raise Exception.Create('CreateCompatibleDC falhou');
  // ANTIALIASED_QUALITY e nao ClearType: o ClearType rasteriza cada canal
  // de cor numa posicao de subpixel diferente, e aqui a cobertura e lida
  // de UM canal so — com ClearType as bordas das letras sairiam serrilhadas.
  FFont := CreateFontW(-FFontPx, 0, 0, 0, FW_BOLD, 0, 0, 0, DEFAULT_CHARSET,
    OUT_TT_PRECIS, CLIP_DEFAULT_PRECIS, ANTIALIASED_QUALITY,
    DEFAULT_PITCH or FF_SWISS, 'Segoe UI');
  if FFont = 0 then raise Exception.Create('CreateFont falhou');
  FOldFont := SelectObject(FDC, FFont);
  GetTextMetrics(FDC, Tm);
  FLineH := Max(1, Tm.tmHeight);

  // Um DIB so, do tamanho do maior bloco possivel (duas linhas na largura
  // maxima + folga do contorno). Top-down (altura negativa) pra linha 0
  // ser a de cima, como nos planos do quadro.
  FBmpW := FMaxTextW + FPad * 2 + 4;
  FBmpH := FLineH * 3 + FPad * 2;
  FillChar(Bmi, SizeOf(Bmi), 0);
  Bmi.bmiHeader.biSize := SizeOf(Bmi.bmiHeader);
  Bmi.bmiHeader.biWidth := FBmpW;
  Bmi.bmiHeader.biHeight := -FBmpH;
  Bmi.bmiHeader.biPlanes := 1;
  Bmi.bmiHeader.biBitCount := 32;
  Bmi.bmiHeader.biCompression := BI_RGB;
  Bits := nil;
  FBmp := CreateDIBSection(FDC, Bmi, DIB_RGB_COLORS, Bits, 0, 0);
  if (FBmp = 0) or (Bits = nil) then raise Exception.Create('CreateDIBSection falhou');
  FBits := PByte(Bits);
  FOldBmp := SelectObject(FDC, FBmp);
  SetBkMode(FDC, TRANSPARENT);
  SetTextColor(FDC, RGB(255, 255, 255));

  BuildChunks(ATurns);
  Log('Export: legenda %d bloco(s), fonte %dpx, largura max %dpx.',
    [Length(FChunks), FFontPx, FMaxTextW]);
end;

destructor TCaptionBurner.Destroy;
begin
  if FDC <> 0 then
  begin
    if FOldBmp <> 0 then SelectObject(FDC, FOldBmp);
    if FOldFont <> 0 then SelectObject(FDC, FOldFont);
    DeleteDC(FDC);
  end;
  if FBmp <> 0 then DeleteObject(FBmp);
  if FFont <> 0 then DeleteObject(FFont);
  inherited;
end;

function TCaptionBurner.ChunkCount: Integer;
begin
  Result := Length(FChunks);
end;

function TCaptionBurner.MeasureHeight(const AText: string): Integer;
// Altura que o texto ocupa quebrando palavra na largura maxima.
var
  R: TRect;
begin
  R := Rect(0, 0, FMaxTextW, 0);
  DrawTextW(FDC, PWideChar(AText), Length(AText), R,
    DT_CALCRECT or DT_WORDBREAK or DT_CENTER or DT_NOPREFIX);
  Result := R.Bottom - R.Top;
end;

procedure TCaptionBurner.BuildChunks(const ATurns: TCaptionTurnArray);
// Guloso palavra a palavra: fecha o bloco quando a proxima palavra o
// faria passar de duas linhas. O tempo do turno e dividido entre os
// blocos na proporcao dos caracteres — a transcricao guardada tem tempo
// por palavra, mas o turno curto tem ritmo quase constante e esta e a
// mesma regra do player, o que mantem a previa e o arquivo iguais.
var
  t, k: Integer;
  Words, Texts: TArray<string>;
  Cur, Next: string;
  MaxH, Total, Acc: Integer;
  Dur: Double;
  C: TChunk;
begin
  FChunks := nil;
  MaxH := FLineH * 2 + FLineH div 2;   // folga de arredondamento
  for t := 0 to High(ATurns) do
  begin
    Words := Trim(ATurns[t].Text).Split([' ', #9, #10, #13],
      TStringSplitOptions.ExcludeEmpty);
    if Length(Words) = 0 then Continue;
    Texts := nil;
    Cur := '';
    for k := 0 to High(Words) do
    begin
      if Cur = '' then Next := Words[k] else Next := Cur + ' ' + Words[k];
      if (Cur <> '') and (MeasureHeight(Next) > MaxH) then
      begin
        Texts := Texts + [Cur];
        Cur := Words[k];
      end
      else
        Cur := Next;
    end;
    if Cur <> '' then Texts := Texts + [Cur];

    Total := 0;
    for k := 0 to High(Texts) do Inc(Total, Length(Texts[k]));
    if Total <= 0 then Total := 1;
    Dur := Max(0, ATurns[t].EndSec - ATurns[t].StartSec);
    Acc := 0;
    for k := 0 to High(Texts) do
    begin
      C.Text := Texts[k];
      C.StartSec := ATurns[t].StartSec + Dur * Acc / Total;
      Inc(Acc, Length(Texts[k]));
      C.EndSec := ATurns[t].StartSec + Dur * Acc / Total;
      FChunks := FChunks + [C];
    end;
  end;
  // Em ordem de INICIO: turnos sobrepostos (faixas isoladas) poem blocos
  // de um turno no meio dos do anterior, e a busca do FindChunk e binaria.
  TArray.Sort<TChunk>(FChunks, TComparer<TChunk>.Construct(
    function(const L, R: TChunk): Integer
    begin
      Result := CompareValue(L.StartSec, R.StartSec);
    end));
end;

function TCaptionBurner.FindChunk(ASec: Double): Integer;
// Bloco no ar no instante ASec. Turnos podem se SOBREPOR (faixas isoladas,
// pegadinha #60p): vale o que COMECOU POR ULTIMO entre os que contem o
// instante — a mesma regra do destaque no player.
var
  Lo, Hi, Mid, k, Stop: Integer;
begin
  Result := -1;
  if Length(FChunks) = 0 then Exit;
  // Ultimo bloco com inicio <= ASec (a lista esta em ordem de inicio
  // dentro de cada turno, e os turnos vem em ordem de inicio).
  Lo := 0;
  Hi := High(FChunks);
  if FChunks[0].StartSec > ASec then Exit;
  while Lo < Hi do
  begin
    Mid := (Lo + Hi + 1) div 2;
    if FChunks[Mid].StartSec <= ASec then Lo := Mid else Hi := Mid - 1;
  end;
  // Volta alguns blocos: com turnos sobrepostos, o ultimo que comecou pode
  // ja ter acabado enquanto um anterior ainda esta no ar.
  Stop := Max(0, Lo - 64);
  for k := Lo downto Stop do
    if (FChunks[k].StartSec <= ASec) and (ASec < FChunks[k].EndSec) then
      Exit(k);
end;

procedure TCaptionBurner.Rasterize(AIdx: Integer);
// Desenha o bloco no DIB, tira a cobertura, gera o contorno por dilatacao
// e guarda alfa + luma prontos pra misturar.
var
  R: TRect;
  TextW, TextH, W, H, x, y, dx, dy, ix, iy: Integer;
  Cov, Tmp, Outl: TBytes;
  Row: PByte;
  F, O, A, M: Integer;
begin
  FCurIdx := AIdx;
  // Medida do texto do bloco (centralizado, quebrando na largura maxima).
  R := Rect(0, 0, FMaxTextW, 0);
  DrawTextW(FDC, PWideChar(FChunks[AIdx].Text), Length(FChunks[AIdx].Text), R,
    DT_CALCRECT or DT_WORDBREAK or DT_CENTER or DT_NOPREFIX);
  TextW := Min(FMaxTextW, R.Right - R.Left);
  TextH := Min(FBmpH - FPad * 2, R.Bottom - R.Top);
  // Dimensoes PARES: o croma do YUV420 tem metade da resolucao, e bloco
  // com offset/lado impar desalinharia a mistura do U/V.
  W := (Min(FBmpW, TextW + FPad * 2) + 1) and not 1;
  H := (Min(FBmpH, TextH + FPad * 2) + 1) and not 1;
  W := Min(W, FOutW and not 1);
  H := Min(H, FOutH and not 1);
  FCurW := W;
  FCurH := H;
  FCurX := ((FOutW - W) div 2) and not 1;
  FCurY := Max(0, (FOutH - FBottom - H)) and not 1;

  // Fundo preto, texto branco: a cobertura e o canal B do pixel.
  FillChar(FBits^, FBmpW * FBmpH * 4, 0);
  R := Rect(FPad, FPad, FPad + TextW, FPad + TextH);
  DrawTextW(FDC, PWideChar(FChunks[AIdx].Text), Length(FChunks[AIdx].Text), R,
    DT_WORDBREAK or DT_CENTER or DT_NOPREFIX);
  GdiFlush;

  SetLength(Cov, W * H);
  for y := 0 to H - 1 do
  begin
    Row := FBits + (y * FBmpW) * 4;
    for x := 0 to W - 1 do
      Cov[y * W + x] := Row[x * 4];
  end;

  // Contorno = cobertura dilatada (maximo num quadrado de raio FRadius),
  // separavel: primeiro na horizontal, depois na vertical.
  SetLength(Tmp, W * H);
  for y := 0 to H - 1 do
    for x := 0 to W - 1 do
    begin
      M := 0;
      for dx := -FRadius to FRadius do
      begin
        ix := x + dx;
        if (ix >= 0) and (ix < W) and (Cov[y * W + ix] > M) then
          M := Cov[y * W + ix];
      end;
      Tmp[y * W + x] := M;
    end;
  SetLength(Outl, W * H);
  for y := 0 to H - 1 do
    for x := 0 to W - 1 do
    begin
      M := 0;
      for dy := -FRadius to FRadius do
      begin
        iy := y + dy;
        if (iy >= 0) and (iy < H) and (Tmp[iy * W + x] > M) then
          M := Tmp[iy * W + x];
      end;
      Outl[y * W + x] := M;
    end;

  SetLength(FAlpha, W * H);
  SetLength(FLuma, W * H);
  for x := 0 to W * H - 1 do
  begin
    F := Cov[x];
    // Contorno um pouco translucido: preto chapado a 100% pesa demais.
    O := (Outl[x] * 230) div 255;
    A := Max(F, O);
    FAlpha[x] := A;
    if A = 0 then FLuma[x] := Y_BLACK
    else
      // Branco onde ha letra, preto no resto do contorno, com a borda da
      // letra misturando os dois.
      FLuma[x] := (F * Y_WHITE + (A - F) * Y_BLACK) div A;
  end;

  // Alfa do croma: media do bloco 2x2 correspondente.
  SetLength(FChromaA, (W div 2) * (H div 2));
  for y := 0 to (H div 2) - 1 do
    for x := 0 to (W div 2) - 1 do
      FChromaA[y * (W div 2) + x] :=
        (FAlpha[(2 * y) * W + 2 * x] + FAlpha[(2 * y) * W + 2 * x + 1] +
         FAlpha[(2 * y + 1) * W + 2 * x] + FAlpha[(2 * y + 1) * W + 2 * x + 1]) div 4;
end;

procedure TCaptionBurner.BlendAt(ASrcSec: Double; AFrame: PAVFrame);
var
  Idx, x, y, A: Integer;
  PY, PU, PV: PByte;
  CW: Integer;
begin
  if AFrame = nil then Exit;
  Idx := FindChunk(ASrcSec);
  if Idx < 0 then Exit;
  if Idx <> FCurIdx then Rasterize(Idx);
  if (FCurW <= 0) or (FCurH <= 0) then Exit;

  // Luma. Mistura com o alfa da legenda; o que o texto nao cobre fica
  // intocado (A = 0 pula o pixel).
  for y := 0 to FCurH - 1 do
  begin
    PY := PByte(NativeUInt(AFrame.data[0]) +
      NativeUInt(FCurY + y) * NativeUInt(AFrame.linesize[0]) + NativeUInt(FCurX));
    for x := 0 to FCurW - 1 do
    begin
      A := FAlpha[y * FCurW + x];
      if A = 0 then Continue;
      PY[x] := (PY[x] * (255 - A) + FLuma[y * FCurW + x] * A) div 255;
    end;
  end;

  // Croma: a legenda e branca/preta, entao o U/V dela e neutro (128) —
  // misturar puxa a cor de baixo pro cinza na medida do alfa.
  CW := FCurW div 2;
  for y := 0 to (FCurH div 2) - 1 do
  begin
    PU := PByte(NativeUInt(AFrame.data[1]) +
      NativeUInt(FCurY div 2 + y) * NativeUInt(AFrame.linesize[1]) +
      NativeUInt(FCurX div 2));
    PV := PByte(NativeUInt(AFrame.data[2]) +
      NativeUInt(FCurY div 2 + y) * NativeUInt(AFrame.linesize[2]) +
      NativeUInt(FCurX div 2));
    for x := 0 to CW - 1 do
    begin
      A := FChromaA[y * CW + x];
      if A = 0 then Continue;
      PU[x] := (PU[x] * (255 - A) + 128 * A) div 255;
      PV[x] := (PV[x] * (255 - A) + 128 * A) div 255;
    end;
  end;
end;

{$POINTERMATH OFF}

end.
