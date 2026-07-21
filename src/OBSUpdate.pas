{
  OBSUpdate — verifica se há versão nova no GitHub Releases.

  Consulta a API pública do repositório (sem autenticação):
    GET https://api.github.com/repos/e-delphi/NoOBS/releases/latest

  Usa TNetHTTPClient, que no Windows fala com winhttp.dll — TLS do próprio
  sistema, sem DLL extra. NÃO usar Indy aqui: o TIdHTTP depende das DLLs do
  OpenSSL pra HTTPS, e o projeto não as distribui.

  Roda SEMPRE em worker thread (chamada de rede pode pendurar por segundos)
  e devolve o resultado na main via TThread.Queue, como o resto do projeto.

  Falha é silenciosa por design: sem internet, offline, rate limit ou API
  fora do ar apenas registra no log e não incomoda o usuário. A única coisa
  pior que não avisar de uma atualização é avisar errado.

  Rate limit: 60 req/hora por IP sem token — folgado pra 1 checagem/dia.
}
unit OBSUpdate;

interface

type
  TUpdateResult = record
    Ok: Boolean;          // a consulta em si funcionou (200 + JSON válido)
    HasUpdate: Boolean;   // há versão mais nova que a atual
    LatestTag: string;    // ex.: 'v0.16.0-beta'
    ReleaseUrl: string;   // página do release pra abrir no navegador
    Error: string;        // motivo quando Ok = False (só pro log)
    // Código HTTP devolvido pelo GitHub. 0 = NÃO houve resposta (falha de
    // rede, DNS, timeout, offline). A distinção importa: se a API respondeu
    // qualquer coisa — inclusive 403 de rate limit ou 500 — o servidor foi
    // alcançado e a checagem "aconteceu"; sem resposta, não aconteceu e
    // precisa ser tentada de novo em vez de queimar o intervalo de 24h.
    StatusCode: Integer;
  end;

  TUpdateCheckProc = procedure(const AResult: TUpdateResult);

// Dispara a checagem em worker thread. O callback roda na MAIN thread.
// No-op se já houver uma checagem em andamento.
procedure CheckForUpdates(ACallback: TUpdateCheckProc);
function IsChecking: Boolean;

implementation

uses
  Winapi.Windows, System.SysUtils, System.Classes, System.JSON,
  System.Net.HttpClient, System.Net.HttpClientComponent, System.Net.URLClient,
  OBSLog, OBSVersion;

const
  API_URL = 'https://api.github.com/repos/e-delphi/NoOBS/releases/latest';
  // Timeouts curtos: isso roda em background e não vale segurar recurso.
  CONNECT_TIMEOUT_MS = 5000;
  RESPONSE_TIMEOUT_MS = 8000;

var
  Checking: Integer = 0;   // usado com InterlockedExchange (thread-safe)

function IsChecking: Boolean;
begin
  Result := Checking <> 0;
end;

// Faz a requisição e interpreta o JSON. Roda NA WORKER THREAD.
function FetchLatest: TUpdateResult;
var
  Http: TNetHTTPClient;
  Resp: IHTTPResponse;
  Json: TJSONValue;
  Obj: TJSONObject;
  Body: string;
begin
  Result := Default(TUpdateResult);
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    Http.ResponseTimeout := RESPONSE_TIMEOUT_MS;
    // A API do GitHub RECUSA (403) requisições sem User-Agent. Não é
    // opcional — sem esse header a checagem falha sempre.
    Http.UserAgent := 'NoOBS-UpdateCheck';
    Http.CustomHeaders['Accept'] := 'application/vnd.github+json';
    Http.CustomHeaders['X-GitHub-Api-Version'] := '2022-11-28';

    Resp := Http.Get(API_URL);
    if Resp = nil then
    begin
      Result.Error := 'sem resposta';
      Exit;
    end;
    // A partir daqui o servidor respondeu — registra o código ANTES de
    // qualquer Exit, pra o chamador saber que a checagem de fato ocorreu.
    Result.StatusCode := Resp.StatusCode;
    if Resp.StatusCode <> 200 then
    begin
      // 404 = ainda não há release publicado; 403 = rate limit.
      Result.Error := Format('HTTP %d', [Resp.StatusCode]);
      Exit;
    end;

    Body := Resp.ContentAsString(TEncoding.UTF8);
    Json := TJSONObject.ParseJSONValue(Body);
    if not (Json is TJSONObject) then
    begin
      Json.Free;
      Result.Error := 'JSON inválido';
      Exit;
    end;
    try
      Obj := TJSONObject(Json);
      Obj.TryGetValue<string>('tag_name', Result.LatestTag);
      Obj.TryGetValue<string>('html_url', Result.ReleaseUrl);
    finally
      Json.Free;
    end;

    if Trim(Result.LatestTag) = '' then
    begin
      Result.Error := 'release sem tag_name';
      Exit;
    end;

    Result.Ok := True;
    Result.HasUpdate := OBSVersion.IsRemoteNewer(Result.LatestTag);
  finally
    Http.Free;
  end;
end;

procedure CheckForUpdates(ACallback: TUpdateCheckProc);
begin
  // Guarda contra checagens simultâneas (ex.: clique repetido no botão).
  if InterlockedExchange(Checking, 1) <> 0 then
  begin
    Log('Update: checagem já em andamento — ignorando.');
    Exit;
  end;

  TThread.CreateAnonymousThread(
    procedure
    var
      R: TUpdateResult;
    begin
      try
        try
          R := FetchLatest;
        except
          on E: Exception do
          begin
            R := Default(TUpdateResult);
            R.Error := E.Message;
          end;
        end;

        if R.Ok then
          Log('Update: atual=%s remoto=%s novidade=%s',
            [OBSVersion.CurrentVersion, R.LatestTag, BoolToStr(R.HasUpdate, True)])
        else
          Log('Update: falha na checagem (%s).', [R.Error]);

        if Assigned(ACallback) then
          TThread.Queue(nil,
            procedure
            begin
              try ACallback(R); except end;
            end);
      finally
        InterlockedExchange(Checking, 0);
      end;
    end).Start;
end;

end.
