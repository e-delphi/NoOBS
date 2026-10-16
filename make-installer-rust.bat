@echo off
REM ---------------------------------------------------------------------
REM  make-installer-rust.bat - instalador da versao RUST do NoOBS.
REM
REM  Uso:
REM      make-installer-rust.bat                  versao = tag do git
REM      make-installer-rust.bat v0.22.0.0-beta   versao explicita
REM
REM  Saida: NoOBS-<versao>-rust.exe na raiz do projeto.
REM
REM  Compila rust\ em release (cargo build --release), monta rust\exe\ com
REM  o rust\sync-runtime.bat (copia do runtime do Delphi, com o exe do Rust
REM  e sem o WebView2Loader.dll) e chama o mesmo make-installer.bat do
REM  Delphi apontando pra essa pasta. O exe\ do Delphi nao e tocado.
REM
REM  Os dois instaladores instalam no MESMO lugar e com o mesmo nome: um
REM  substitui o outro.
REM ---------------------------------------------------------------------
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
  echo [make-installer-rust] ERRO: cargo nao encontrado. Instale o Rust ^(rustup.rs^).
  exit /b 1
)

echo [make-installer-rust] compilando rust\ em release ...
pushd rust
cargo build --release
if errorlevel 1 (
  popd
  echo [make-installer-rust] ERRO: cargo build falhou.
  exit /b 1
)
popd

call "%~dp0rust\sync-runtime.bat"
if errorlevel 1 exit /b 1

set "NOOBS_RUNTIME=rust\exe"
set "NOOBS_FLAVOR=rust"
call "%~dp0make-installer.bat" %*
exit /b %errorlevel%
