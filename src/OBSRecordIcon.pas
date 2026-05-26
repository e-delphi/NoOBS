(*
  OBSRecordIcon - gera variante do icone do app com uma bolinha vermelha
  no canto inferior direito, usada como indicador visual de "gravacao
  ativa" no icone da bandeja e na barra de tarefas.

  Tudo em runtime via GDI/VCL.Graphics — nao precisa de um segundo .ico
  no projeto. Carrega o MAINICON, desenha numa DIB 32-bit ARGB, sobrepoe
  a bolinha vermelha com borda branca, gera HICON.

  Uso:
    Icon := CreateRecordingOverlayIcon(BaseIcon, 32);
    ... use ...
    DestroyIcon(Icon);
*)
unit OBSRecordIcon;

interface

uses
  Winapi.Windows, System.Math;

// Cria um HICON novo: base icon + bolinha vermelha sobreposta.
// Caller e responsavel por chamar DestroyIcon no resultado.
function CreateRecordingOverlayIcon(ABaseIcon: HICON; ASize: Integer): HICON;

implementation

uses
  Vcl.Graphics;

function CreateRecordingOverlayIcon(ABaseIcon: HICON; ASize: Integer): HICON;
const
  DOT_R: Byte = 220;  // #DC2626 — mesmo --danger da UI
  DOT_G: Byte = 38;
  DOT_B: Byte = 38;
var
  Bmp: TBitmap;
  IconInfo: TIconInfo;
  MaskBmp: HBITMAP;
  DotSize, DotX, DotY: Integer;
  Row: PByte;
  iy, ix: Integer;
  CenterX, CenterY, Radius, Dx, Dy, Dist: Double;
  EdgeFade, Alpha: Double;
  A: Byte;
begin
  Result := 0;
  if (ABaseIcon = 0) or (ASize < 16) then Exit;

  Bmp := TBitmap.Create;
  try
    Bmp.PixelFormat := pf32bit;
    Bmp.SetSize(ASize, ASize);
    Bmp.AlphaFormat := afDefined;

    // Zera o bitmap (alpha=0 em tudo) — TBitmap.SetSize nao garante isso
    // em pf32bit.
    for iy := 0 to ASize - 1 do
      FillChar(Bmp.ScanLine[iy]^, ASize * 4, 0);

    // Desenha o icone base com DrawIconEx — preserva alpha (DI_NORMAL).
    DrawIconEx(Bmp.Canvas.Handle, 0, 0, ABaseIcon, ASize, ASize,
      0, 0, DI_NORMAL);

    // Bolinha vermelha: ~45% do tamanho do icone, canto inf. direito.
    // Desenhada pixel-a-pixel pra escrever alpha correto e nao deixar
    // os cantos do bounding rect com preto solido (problema de mexer
    // direto com GDI Brush/Pen — Ellipse nao toca o canal alpha).
    DotSize := (ASize * 9) div 20;  // 45%
    DotX    := ASize - DotSize;
    DotY    := ASize - DotSize;
    CenterX := DotX + (DotSize / 2.0);
    CenterY := DotY + (DotSize / 2.0);
    Radius  := DotSize / 2.0;

    for iy := DotY to ASize - 1 do
    begin
      Row := PByte(Bmp.ScanLine[iy]);
      for ix := DotX to ASize - 1 do
      begin
        // Distancia do centro do pixel (offset 0.5) ate o centro do
        // circulo. Pixels dentro do raio recebem vermelho solido;
        // pixels na borda (ultimo 1px) recebem alpha proporcional pra
        // anti-aliasing simples — sem isso, bolinha pequena (~14px)
        // fica com serrilhado visivel.
        Dx := (ix + 0.5) - CenterX;
        Dy := (iy + 0.5) - CenterY;
        Dist := Sqrt(Dx * Dx + Dy * Dy);
        EdgeFade := Radius - Dist;
        if EdgeFade >= 1.0 then
          Alpha := 1.0
        else if EdgeFade <= 0.0 then
          Continue  // fora do circulo — nao mexe
        else
          Alpha := EdgeFade;  // borda de 1px com fade linear

        A := Byte(Round(Alpha * 255));
        // Pre-multiplied alpha (afDefined): RGB * A / 255.
        Row[ix * 4 + 0] := Byte((DOT_B * A) div 255);
        Row[ix * 4 + 1] := Byte((DOT_G * A) div 255);
        Row[ix * 4 + 2] := Byte((DOT_R * A) div 255);
        Row[ix * 4 + 3] := A;
      end;
    end;

    // Mask monocromatica vazia — o alpha do DIB 32-bit ja determina
    // o que e visivel; mas CreateIconIndirect exige o campo hbmMask
    // preenchido (mesmo que zerado).
    MaskBmp := CreateBitmap(ASize, ASize, 1, 1, nil);
    try
      FillChar(IconInfo, SizeOf(IconInfo), 0);
      IconInfo.fIcon := True;
      IconInfo.hbmMask  := MaskBmp;
      IconInfo.hbmColor := Bmp.Handle;
      Result := CreateIconIndirect(IconInfo);
      // CreateIconIndirect copia internamente — seguro destruir os
      // bitmaps depois.
    finally
      DeleteObject(MaskBmp);
    end;
  finally
    Bmp.Free;
  end;
end;

end.
