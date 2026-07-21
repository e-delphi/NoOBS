@echo off
REM ---------------------------------------------------------------------
REM  gen-version.bat - gera src\Version.inc a partir da tag do git.
REM
REM  Chamado pelo PreBuildEvent do NoOBS.dproj:
REM      call "$(PROJECTDIR)\gen-version.bat" "$(OUTPUTFILENAME)"
REM
REM  O 1o parametro (opcional) e o nome do exe a encerrar antes de compilar,
REM  senao o link falha com o app rodando. Precisa vir como PARAMETRO: as
REM  macros $(...) sao expandidas pelo IDE na linha do build event, nao
REM  dentro do .bat. Sem parametro, o encerramento e apenas pulado.
REM
REM  O arquivo gerado tem UMA linha, ASCII puro (sem BOM - nao ha acento):
REM      const APP_VERSION = 'v0.15.0-beta';
REM
REM  Por que derivar da tag: criar a tag E o ato de lancar, entao a versao
REM  nunca fica dessincronizada por esquecimento.
REM
REM  git describe devolve sufixo quando o build NAO esta exatamente numa tag:
REM      v0.15.0-beta              -> build de release (limpo, na tag)
REM      v0.15.0-beta-1-gd7e4f73   -> 1 commit depois da tag
REM      ...-dirty                 -> ha alteracoes nao commitadas
REM  O OBSUpdate usa isso: versao com sufixo = build de desenvolvimento,
REM  entao NAO checa atualizacao (evita "sempre tem versao nova").
REM
REM  Salvaguardas:
REM   - git ausente/sem tags: PRESERVA o Version.inc existente (nao zera).
REM   - arquivo inexistente nesse caso: escreve 'dev' pra compilar mesmo assim.
REM   - so reescreve se o valor MUDOU, pra nao forcar recompilacao a cada build.
REM ---------------------------------------------------------------------
setlocal EnableDelayedExpansion
cd /d "%~dp0"

REM --- 1) Encerra o app, se estiver rodando (libera o exe pro linker) ------
REM  TASKKILL devolve errorlevel != 0 quando o processo NAO existe - o caso
REM  normal. Por isso a saida e engolida e o errorlevel e descartado: sem
REM  isso, "processo nao encontrado" faria o build inteiro falhar.
if not "%~1"=="" (
  taskkill /F /IM "%~1" /T >nul 2>&1
  REM !errorlevel! (expansao adiada) e nao %errorlevel%: dentro de um bloco
  REM ( ) o %errorlevel% e resolvido na LEITURA do bloco, antes do taskkill
  REM rodar - sempre daria a mesma resposta.
  if !errorlevel! equ 0 (
    echo [gen-version] %~1 encerrado.
  ) else (
    echo [gen-version] %~1 nao estava rodando.
  )
)

REM --- 2) Gera a versao a partir da tag do git -----------------------------
set "OUT=src\Version.inc"
set "VER="

for /f "delims=" %%v in ('git describe --tags --always --dirty 2^>nul') do set "VER=%%v"

if not defined VER (
  if exist "%OUT%" (
    echo [gen-version] git indisponivel - mantendo %OUT% atual.
  ) else (
    echo const APP_VERSION = 'dev';> "%OUT%"
    echo [gen-version] git indisponivel - %OUT% criado como 'dev'.
  )
  exit /b 0
)

set "LINE=const APP_VERSION = '%VER%';"

REM So reescreve se mudou: reescrever sempre atualizaria o timestamp e
REM faria o Delphi recompilar tudo que depende do .inc em todo build.
set "CUR="
if exist "%OUT%" set /p CUR=<"%OUT%"
if "!CUR!"=="!LINE!" (
  echo [gen-version] %OUT% ja esta em %VER%.
  exit /b 0
)

> "%OUT%" echo !LINE!
echo [gen-version] %OUT% = %VER%
endlocal
exit /b 0
