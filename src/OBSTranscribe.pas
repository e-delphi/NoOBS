(*
  OBSTranscribe - fila de transcricao contra a Transcritor API
  (https://github.com/e-delphi/transcritor-api), um container local com
  faster-whisper + diarizacao por falante.

  Desenho em tres pontos que nao sao obvios:

  1. MANDA AUDIO, NUNCA O VIDEO. A API tem MAX_UPLOAD_MB=1024 por padrao
     (era 512 ate a v2) e uma gravacao 4K passa disso facil. O audio sai
     em m4a pelo ExtractAudioTracks: 1 hora da ~70 MB.

     E manda as faixas ISOLADAS, uma transcricao por faixa, nao a
     mistura: cada faixa e um dispositivo, com o nome dele escrito no MKV
     — entao a atribuicao de falante sai de graca, com "Microfone (X)" no
     lugar de SPEAKER_00. Custa N vezes mais tempo de servidor, entao so
     vale com 2+ isoladas; com uma so a mistura tem o mesmo conteudo.
     Faixa muda nem e enviada. Ver PlanTracks.

     O preco disso e o ECO: o que sai pelos alto-falantes volta pelo
     microfone e o mesmo trecho e transcrito duas vezes. O
     MergeTranscripts derruba a copia — ver o comentario dele.

  2. MODELO ASSINCRONO, COM PROGRESSO REAL. POST /jobs devolve um id na
     hora; GET /jobs/{id} da estagio, percentual e ETA — e o `result` so
     com ?incluir_resultado=true (mudou na v2; ver STATUS_QUERY). O
     percentual mede o trecho de audio ja coberto pelos segmentos — nao
     ha estimativa nossa. Avanca aos saltos (janelas de 30s do Whisper),
     e o ETA so existe a partir de ~10%.

     Etapas da v2: queued -> decoding -> transcribing -> aligning ->
     diarizing -> done. O `aligning` e novo (WhisperX alinha as palavras
     com wav2vec2 antes de separar falantes).

     O id do job e o SHA-256 do AUDIO combinado com as opcoes, entao
     reenviar a mesma gravacao devolve o resultado pronto na hora, com
     HTTP 200 e `cached: true` em vez de 202. Retentar depois de uma falha
     de rede custa so o upload.

  3. UMA POR VEZ. O proprio container serializa as requisicoes (os
     modelos nao sao thread-safe), entao paralelizar aqui so encheria a
     fila do outro lado. Uma thread, uma fila — e o cancelamento aborta
     ate o item em voo, via DELETE /jobs/{id}.

  4. DOIS MOTORES. 'server' (padrao) e a Transcritor API acima, num
     container ou em outra maquina. 'local' e o OBSLocalAsr: Qwen3 +
     audio.cpp na GPU desta maquina, instalado pela aba de Transcricao,
     sem Docker. A escolha e a chave `transcribeEngine`; tudo o que vem
     depois do RunJob (turnos, eco, cache) nao sabe qual motor rodou —
     os dois devolvem o MESMO JSON. O local nao separa falantes.

  O resultado vai pra dois arquivos no cache:
    <hash>.transcript.json  resposta inteira (turnos, segmentos, palavras)
    <hash>.txt              so o texto puro, pra BUSCA
  A busca le o .txt: carregar o JSON inteiro de cada gravacao pra filtrar
  a lista seria varios MB por item.
*)
unit OBSTranscribe;

interface

uses
  System.SysUtils;

type
  // Callback de mudanca de estado — a UI redesenha a partir daqui.
  // Chamado SEMPRE na main thread (a unit faz o marshalling).
  TTranscribeChanged = procedure;

// Enfileira uma gravacao. Ignora silenciosamente se ja esta na fila ou
// se ja tem transcricao (use Retranscribe pra refazer).
// AManual = o USUARIO pediu (menu da gravacao, "transcrever pendentes"). So
// a transcricao automatica espera o fim da captura; o que foi pedido a mao
// roda mesmo com gravacao ou buffer ligado (ver SetPaused). Pedir a mao um
// item que ja estava na fila como automatico o promove a manual.
procedure Enqueue(const APath: string; AManual: Boolean = False);
// Enfileira varias de uma vez. AManual vale pra todas; AManualPaths marca
// uma a uma (restauracao da fila salva, que guarda quem era manual).
procedure EnqueueMany(const APaths: TArray<string>; AManual: Boolean = False;
  const AManualPaths: TArray<string> = nil);
// Cancela a fila inteira. O item EM CURSO nao e abortado no meio (a API
// nao tem cancelamento); ele termina e o resultado e descartado.
// Segura os itens AUTOMATICOS sem esvaziar a fila: nenhum automatico NOVO
// comeca enquanto pausado (o que ja esta em curso segue ate o fim). Usado
// pelo OBSBridge enquanto grava ou o buffer em memoria esta ligado —
// transcrever le a gravacao inteira do disco e sobe dezenas de MB,
// disputando maquina com o que esta sendo capturado. Itens pedidos A MAO
// (Enqueue com AManual) NAO esperam: ali o usuario escolheu gastar a
// maquina agora. Os itens ficam na fila PERSISTIDA; o Bridge solta quando
// a captura termina.
procedure SetPaused(APaused: Boolean);
function IsPaused: Boolean;

procedure CancelAll;
// Para a thread e limpa — chamada no shutdown.
procedure Shutdown;

procedure SetOnChanged(ACallback: TTranscribeChanged);

// --- estado pra UI (le da main thread) ---
function IsRunning: Boolean;
function QueueLength: Integer;        // inclui o item em curso
function CurrentName: string;
function CurrentElapsedSec: Integer;
// Progresso REAL vindo da API (0..1), medido pelo trecho de audio ja
// coberto pelos segmentos — nao e barra estimada. -1 = ainda nao ha
// numero (extraindo audio, subindo, ou job na fila do servidor).
function CurrentProgress: Double;
// Segundos restantes segundo a API. -1 = ela ainda nao arrisca (so
// aparece a partir de ~10% de progresso).
function CurrentEtaSec: Integer;
// Etapa em curso: 'extracting' | 'uploading' | 'queued' | 'decoding' |
// 'transcribing' | 'diarizing' | 'done'. Vazio = parado.
function CurrentStage: string;
// Faixa em transcricao e quantas ha, quando a gravacao esta sendo mandada
// por faixas isoladas. TrackCount = 1 = faixa unica (a mistura, ou uma
// isolada so) — a UI nao mostra nada nesse caso.
function CurrentTrack: Integer;
function CurrentTrackCount: Integer;
function DoneCount: Integer;
function FailedCount: Integer;
function LastError: string;
// Nome (sem extensao) da gravacao a que o LastError se refere.
function LastErrorName: string;
// Total planejado do lote atual (pra "3 de 7"). Zera quando a fila esvazia.
function BatchTotal: Integer;

// --- fila visivel/editavel (aba Transcricao) ---
// Caminhos que AINDA ESPERAM, na ordem em que serao processados. O item
// em curso NAO entra aqui — ele ja saiu da fila quando a worker o pegou;
// quem o identifica e o CurrentPath.
function QueuedPaths: TArray<string>;
// Caminho do item em curso ('' se parado). O CurrentName so tem o nome,
// e nome nao identifica arquivo (duas pastas podem ter homonimos).
function CurrentPath: string;
// Contador que muda quando a COMPOSICAO da fila muda (entrou, saiu,
// mudou de lugar) — e nao quando so o progresso andou. O Bridge compara
// com o ultimo que empurrou pra nao remandar a lista inteira a cada
// tique de 1s (ver PushTranscribeQueue).
function QueueRevision: Integer;
// Move um item da espera pra posicao ANewIndex (0 = proximo a rodar).
// False se o path nao esta esperando. Nao mexe no item em curso.
function MoveInQueue(const APath: string; ANewIndex: Integer): Boolean;
// Tira um item da espera. False se nao estava la. O item EM CURSO nao
// sai por aqui — pra ele existe o CancelAll.
function RemoveFromQueue(const APath: string): Boolean;

// True se a gravacao ja tem transcricao no cache.
function HasTranscript(const APath: string): Boolean;
// Estado da transcricao em UMA palavra, pros cards da biblioteca:
//   ''      nunca foi transcrita
//   'ok'    transcrita, com fala
//   'empty' transcrita, e o audio nao tinha fala nenhuma
// Distinguir os dois ultimos importa: sem isso o player dizia "ainda nao
// foi transcrita" numa gravacao que FOI transcrita e simplesmente nao
// tem ninguem falando. So faz stat — nada de ler o conteudo, porque isto
// roda uma vez por gravacao a cada listagem da biblioteca.
function TranscriptState(const APath: string): string;
// Caminho do JSON completo (pode nao existir).
function TranscriptPath(const APath: string): string;
// Caminho do texto puro usado pela busca (pode nao existir).
function TranscriptTextPath(const APath: string): string;
// Apaga os dois arquivos de uma gravacao (usado ao excluir/mover).
procedure DeleteTranscript(const APath: string);

// Testa o servidor: GET /health. Devolve '' se ok, senao a mensagem.
function CheckHealth(const AHost: string): string;

// Base do servidor, do config ('transcribeHost'). Sempre sem barra final.
function HostBase: string;

// --- servidor fora do ar ---
// True enquanto a fila esta PARADA esperando o servidor voltar. Servidor
// fora do ar nao e defeito do arquivo: o item volta pra frente da espera
// e a worker tenta de novo a cada SERVER_RETRY_MS. O Bridge usa isto pra
// deixar o app hibernar mesmo com fila (a fila fica salva no disco).
function WaitingForServer: Boolean;
// Acorda a worker que espera o servidor, sem aguardar o proximo ciclo.
// Chamado quando um teste ou diagnostico acabou de ver o servidor de pe.
procedure NudgeQueue;

// --- persistencia ---
// Caminhos salvos na sessao anterior (item em curso primeiro, depois a
// espera). Quem restaura e o Bridge, que filtra o que nao faz mais
// sentido (arquivo sumiu, ja transcrito, so na nuvem) antes do EnqueueMany.
function LoadSavedQueue: TArray<string>;
// Os que, dentre eles, tinham sido pedidos a mao — pra restauracao nao
// rebaixar um pedido do usuario a automatico (que esperaria a captura).
function LoadSavedManualPaths: TArray<string>;
// Item pedido a mao? (a lista da aba de Transcricao marca os que esperam)
function IsManual(const APath: string): Boolean;

// Idioma que vai pra API ('' = deixar a API detectar). Ver o comentario
// na implementacao: detectar e o que fazia a transcricao sair TRADUZIDA.
function ResolveTranscribeLanguage: string;

// --- diagnostico do ambiente (aba Transcricao) ---
type
  TTranscribeSetup = record
    ServerOk: Boolean;
    ServerError: string;
    IsLocal: Boolean;          // o host aponta pra esta maquina
    Port: Integer;
    WslOk: Boolean;
    DockerInstalled: Boolean;
    DockerRunning: Boolean;
    DockerDesktopPath: string; // '' = nao achou o executavel
    ContainerId: string;       // '' = nenhum container publica a porta
    ContainerImage: string;
    ContainerState: string;    // 'running' | 'exited' | 'created' | ...
    ContainerStatus: string;   // texto do docker ("Up 3 minutes")
    RestartPolicy: string;     // 'always' | 'no' | ...
  end;

// Descobre, em etapas, por que o servidor nao responde (WSL -> Docker ->
// container -> servidor). Roda processos wsl/docker e pode levar alguns
// segundos: chame SO de worker thread.
function DiagnoseSetup(const AHost: string): TTranscribeSetup;

// Motor escolhido: True = OBSLocalAsr (GPU desta maquina), False = o
// servidor da Transcritor API (transcribeHost).
function UseLocalEngine: Boolean;

// Roda um programa de console sem janela; codigo de saida (-1 = nao rodou
// ou estourou o tempo) e stdout+stderr em AOutput. Exportada pro
// OBSLocalAsr detectar a GPU com o audiocpp_cli.
function RunHidden(const AExe, AArgs: string; ATimeoutMs: Cardinal;
  out AOutput: string): Integer;

implementation

uses
  Winapi.Windows,
  System.Classes,
  System.JSON,
  System.IOUtils,
  System.SyncObjs,
  System.Net.HttpClient,
  // TNetHTTPClient e do HttpClientComponent (o HttpClient so traz o
  // THTTPClient) e o IHTTPResponse vem do URLClient. Sem os tres, o
  // TNetHTTPClient fica indefinido e o parser descarrila daqui pra baixo.
  System.Net.HttpClientComponent,
  System.Net.URLClient,
  System.Net.Mime,
  System.Generics.Collections,
  System.Generics.Defaults,
  System.Character,
  System.Math,
  System.StrUtils,
  OBSLog,
  OBSLang,
  OBSProbe,
  OBSConfig,
  OBSPlayer,
  OBSLocalAsr,
  FFmpegOps;

const
  // Rotas FIXAS de proposito: o usuario informa host e porta, nao o
  // caminho. O modelo e ASSINCRONO — POST /jobs devolve um id na hora e o
  // andamento sai de GET /jobs/{id}.
  JOBS_PATH   = '/jobs';
  HEALTH_PATH = '/health';
  // A v2 da API separou andamento de resultado: o GET /jobs/{id} devolve
  // so o progresso, e o JSON da transcricao vem neste parametro. (O outro
  // caminho seria o GET /jobs/{id}/download, que existe pra json/txt/srt/
  // vtt — mas ai seriam duas requisicoes pra ter a mesma coisa.)
  STATUS_QUERY = '?incluir_resultado=true';

  DEFAULT_HOST = 'http://localhost:8000';

  HEALTH_TIMEOUT_MS = 4000;
  CONNECT_TIMEOUT_MS = 5000;
  // O POST /jobs so espera o UPLOAD (a API responde ao enfileirar), mas
  // uma gravacao longa vira dezenas de MB — dai a folga.
  SUBMIT_TIMEOUT_MS = 5 * 60 * 1000;
  POLL_TIMEOUT_MS   = 10000;
  // Consulta barata e local. O progresso anda aos saltos (o Whisper
  // processa em janelas de 30s), entao 1s ja mostra cada degrau.
  POLL_INTERVAL_MS  = 1000;

  // --- deduplicacao entre faixas ---
  // Com faixas isoladas, o que sai pelos alto-falantes VOLTA pelo
  // microfone (a menos que o usuario use fone). O mesmo trecho e entao
  // transcrito duas vezes, e a copia precisa cair fora.
  //
  // Dois testes, os DOIS obrigatorios — cada um sozinho tem falso
  // positivo obvio: so tempo derrubaria duas pessoas falando junto (que
  // e exatamente o que faixas isoladas existem pra preservar), e so
  // texto derrubaria uma repeticao legitima minutos depois.
  DEDUP_OVERLAP_MIN = 0.5;   // fracao do turno mais curto em comum
  DEDUP_SIM_MIN     = 0.7;   // fracao das palavras do turno CURTO no longo
  // Turno de uma palavra so nao entra: "sim"/"certo" esta contido em
  // quase qualquer frase mais longa, e derrubar um "sim" simultaneo por
  // isso seria falso positivo garantido.
  DEDUP_MIN_WORDS   = 2;
  // (Os DEDUP_* acima sao do dedup POR TURNO, que sobrou so como rede de
  // seguranca pra faixa sem timestamp por palavra. O caminho normal e o
  // dedup POR PALAVRA abaixo — ver DedupWords.)

  // --- deduplicacao POR PALAVRA ---
  // Casa a MESMA palavra nas duas faixas no MESMO instante, forma corridas
  // tolerando palavras faltando (o eco chega picado) e derruba a copia.
  // Valores escolhidos por varredura sobre 624 palavras REAIS de uma
  // ligacao, em 6 cenarios (dois mics na sala, alto-falante com eco, fone
  // sem eco, gente falando junto, frase repetida depois):
  WDEDUP_TOL       = 0.8;   // s entre a mesma palavra nas duas faixas
  WDEDUP_MAXSKIP   = 3;     // palavras puladas entre dois pares da corrida
  WDEDUP_RUN_GAP   = 1.5;   // s de silencio que quebra a corrida
  WDEDUP_MIN_RUN   = 4;     // corrida LONGA: eco com certeza
  // Corrida CURTA (resposta de 1-3 palavras, o caso mais comum numa
  // ligacao) so conta se for SIMULTANEA — quem repete a fala do outro
  // fala DEPOIS; o eco acontece no mesmo instante do original.
  WDEDUP_MIN_SHORT = 2;     // 1 palavra so coincide por acaso ("oi", "ta")
  WDEDUP_SHORT_TOL = 0.35;  // s: a folga apertada da corrida curta
  WDEDUP_PAD       = 0.5;   // s em volta da corrida curta pra achar "sobra"

  // --- recorte de turnos a partir das PALAVRAS ---
  // O `turns` da API agrupa por MUDANCA DE FALANTE. Na mistura os
  // falantes se alternam e isso da turnos curtos; numa faixa isolada de
  // uma pessoa so NAO HA troca, e a faixa inteira vira um turno unico —
  // medido: 22,5 s num bloco so, com o player destacando esse bloco
  // enquanto as palavras acontecem em algum lugar dentro dele.
  //
  // Por isso os turnos sao remontados a partir do timestamp POR PALAVRA
  // que a resposta ja traz (segments[].words). Corta em: troca de
  // falante, pausa, fim de frase depois de um tempo, e um teto duro pra
  // fala continua que nunca respira.
  TURN_GAP_SEC      = 1.0;   // silencio que ja separa duas falas
  TURN_SOFT_MAX_SEC = 8.0;   // a partir daqui, corta no fim de frase
  TURN_HARD_MAX_SEC = 20.0;  // fala continua: corta de qualquer jeito

  // Faixa cujo pico nunca passa disso nao vai pro servidor: e o
  // microfone que ninguem usou, ou o alto-falante de uma gravacao muda.
  // Mandar assim so gastaria minutos de fila pra receber zero turnos.
  SILENCE_PEAK_MIN = 0.02;
  SILENCE_BUCKETS  = 400;

  // Sentinela de "cancelado pelo usuario". Nao e erro: nao entra na
  // contagem de falhas nem vira mensagem na tela.
  CANCELED_MARK = #1'canceled';

  // Sentinela de "servidor fora do ar". Tambem NAO e falha: o item volta
  // pra frente da espera e a worker tenta de novo (ver Execute). Com a
  // transcricao automatica ao fim da gravacao, gravar com o Docker ainda
  // subindo e o caso COMUM — tratar como falha descartaria o item, e a
  // fila persistida nao serviria pra nada.
  SERVER_DOWN_MARK = #1'serverdown';
  // Entre tentativas com o servidor fora. O teste e um GET /health local
  // (conexao recusada volta na hora; 4 s no pior caso). Era 30 s, e o
  // aviso vermelho ficava na tela meio minuto depois do Docker subir.
  SERVER_RETRY_MS = 10000;

  // Sentinela de "motor LOCAL escolhido mas ainda nao instalado". Mesmo
  // tratamento do servidor fora do ar: o item espera na fila (persistida)
  // e anda sozinho quando a instalacao termina (o Bridge chama NudgeQueue).
  LOCAL_MISSING_MARK = #1'localmissing';

  QUEUE_FILE = 'transcribe-queue.json';

  // Timeouts do diagnostico. O `docker info` e o mais lento: com o Docker
  // Desktop subindo ele pode segurar varios segundos antes de responder.
  // Medido nesta maquina com tudo de pe: wsl --status 84 ms, docker info
  // 313 ms, docker ps 175 ms.
  DIAG_WSL_TIMEOUT_MS    = 10000;
  DIAG_DOCKER_TIMEOUT_MS = 20000;

type
  TTranscribeThread = class(TThread)
  protected
    procedure Execute; override;
  end;

var
  GQueue: TList<string> = nil;
  GLock: TCriticalSection = nil;
  Worker: TTranscribeThread = nil;
  StopEvent: THandle = 0;
  OnChanged: TTranscribeChanged = nil;

  // Estado observado pela UI. Escrito na worker, lido na main — sempre
  // sob GLock, exceto os inteiros simples (leitura atomica em x86-64).
  GRunning: Boolean = False;
  GCurrentName: string = '';
  GCurrentPath: string = '';
  GCurrentStartTick: Cardinal = 0;
  GStage: string = '';
  GProgress: Double = -1;      // -1 = ainda sem numero
  GEta: Integer = -1;          // -1 = a API ainda nao arrisca
  GCancelCurrent: Boolean = False;
  GDone: Integer = 0;
  GFailed: Integer = 0;
  GBatchTotal: Integer = 0;
  GLastError: string = '';
  // Nome da gravacao que falhou. Sem ele, "1 com falha" num lote de 7
  // nao diz QUAL — e a mensagem sozinha raramente identifica o item.
  GLastErrorName: string = '';
  // Sobe a cada mudanca de COMPOSICAO da fila. Ver QueueRevision.
  GQueueRev: Integer = 0;
  GTrack: Integer = 0;
  GTrackCount: Integer = 1;
  GWaitingServer: Boolean = False;
  // Fila segurada pelo Bridge (gravacao ou buffer ligado). Ver SetPaused.
  GPaused: Boolean = False;
  // Caminhos (minusculos) pedidos A MAO — rodam mesmo com a fila pausada.
  // Um conjunto a parte em vez de mudar o tipo da GQueue: ela e lida em
  // muitos lugares como lista de caminhos, e a marca so interessa a quem
  // escolhe o proximo item. Sai daqui quando o item termina de verdade
  // (nao quando a worker o pega: servidor fora do ar devolve o item pra
  // fila, e ele tem que voltar ainda manual).
  GManual: TDictionary<string, Boolean> = nil;
  // O item em curso JA terminou (sucesso, falha ou cancelamento) e so
  // falta a proxima volta do laco tira-lo do estado. Sem esta marca, um
  // fechamento nesse intervalo persistiria o item concluido de novo.
  GCurrentFinished: Boolean = False;
  // Auto-reset: acorda a worker que espera o servidor (NudgeQueue).
  WakeEvent: THandle = 0;
  SaveLock: TCriticalSection = nil;
  GShuttingDown: Boolean = False;

function HostBase: string;
begin
  Result := Trim(GetConfigStr('transcribeHost', DEFAULT_HOST));
  if Result = '' then Result := DEFAULT_HOST;
  while (Result <> '') and (Result[Length(Result)] = '/') do
    Delete(Result, Length(Result), 1);
end;

function UseLocalEngine: Boolean;
begin
  Result := SameText(GetConfigStr('transcribeEngine', 'server'), 'local');
end;

function TranscriptPath(const APath: string): string;
begin
  Result := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath) + '.transcript.json';
end;

function TranscriptTextPath(const APath: string): string;
begin
  Result := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath) + '.txt';
end;

function HasTranscript(const APath: string): Boolean;
begin
  Result := (APath <> '') and TFile.Exists(TranscriptPath(APath));
end;

function TranscriptState(const APath: string): string;
var
  Txt: string;
begin
  Result := '';
  if APath = '' then Exit;
  if not TFile.Exists(TranscriptPath(APath)) then Exit;
  // O .txt e o texto puro. Vazio = a API respondeu, mas nao havia fala.
  // O TAMANHO basta: ler o arquivo so pra saber se e vazio seria I/O a
  // toa em cada item da lista.
  Txt := TranscriptTextPath(APath);
  if not TFile.Exists(Txt) then Exit('empty');
  try
    if TFile.GetSize(Txt) > 0 then Result := 'ok' else Result := 'empty';
  except
    Result := 'ok';
  end;
end;

procedure DeleteTranscript(const APath: string);
begin
  try if TFile.Exists(TranscriptPath(APath)) then TFile.Delete(TranscriptPath(APath)); except end;
  try if TFile.Exists(TranscriptTextPath(APath)) then TFile.Delete(TranscriptTextPath(APath)); except end;
end;

procedure SetOnChanged(ACallback: TTranscribeChanged);
begin
  OnChanged := ACallback;
end;

procedure NotifyChanged;
// Sempre marshalla pra main: a UI so pode ser tocada de la, e este
// callback e chamado da worker em quase todos os pontos.
begin
  TThread.Queue(nil,
    procedure
    begin
      if Assigned(OnChanged) then
        try OnChanged; except end;
    end);
end;

function IsRunning: Boolean;
begin
  Result := GRunning;
end;

function QueueLength: Integer;
begin
  Result := 0;
  if GLock = nil then Exit;
  GLock.Enter;
  try
    if GQueue <> nil then Result := GQueue.Count;
    if GRunning then Inc(Result);
  finally
    GLock.Leave;
  end;
end;

function CurrentName: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GCurrentName; finally GLock.Leave; end;
end;

function CurrentElapsedSec: Integer;
begin
  Result := 0;
  if not GRunning then Exit;
  if GCurrentStartTick = 0 then Exit;
  Result := Integer((GetTickCount - GCurrentStartTick) div 1000);
end;

function CurrentProgress: Double;
begin
  Result := GProgress;
end;

function CurrentEtaSec: Integer;
begin
  Result := GEta;
end;

function CurrentStage: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GStage; finally GLock.Leave; end;
end;

function CancelRequested: Boolean;
begin
  Result := GCancelCurrent;
end;

function ShuttingDown: Boolean;
// O ProcessOne roda na worker mas NAO e metodo dela, entao nao enxerga o
// Terminated. O sinal de fechamento que vale aqui e o StopEvent (manual
// reset, ligado pelo Stop) — o mesmo que tira a espera do laco de polling.
begin
  Result := (StopEvent <> 0) and
    (WaitForSingleObject(StopEvent, 0) = WAIT_OBJECT_0);
end;

function WaitingForServer: Boolean;
begin
  Result := GWaitingServer;
end;

procedure NudgeQueue;
begin
  if WakeEvent <> 0 then SetEvent(WakeEvent);
end;

function QueueFilePath: string;
begin
  // Ao lado do config.json. O OBSConfig so exporta o caminho do arquivo
  // (ConfigDir e privada dele), entao a pasta sai dali.
  Result := ExtractFilePath(OBSConfig.ConfigFilePath) + QUEUE_FILE;
end;

procedure SaveQueue;
// Grava a fila no disco. Chamada FORA do GLock, depois de toda mudanca de
// composicao (entrou, saiu, mudou de lugar, a worker pegou ou soltou).
//
// O item em curso entra PRIMEIRO: se o app fechar no meio da transcricao,
// ele e o primeiro a voltar. So sai do arquivo quando termina de verdade
// (ver GCurrentFinished).
//
// No fechamento NAO grava. O Shutdown para a worker, e a volta final do
// laco trataria o item interrompido como "terminado" e o tiraria do
// arquivo — justo o item que a persistencia existe pra preservar.
//
// SaveLock serializa snapshot + escrita: duas gravacoes cruzadas (main e
// worker) podiam tirar o snapshot numa ordem e escrever na outra, e o
// arquivo terminaria com o estado VELHO.
var
  Items, Manual: TArray<string>;
  Arr, ManArr: TJSONArray;
  Obj: TJSONObject;
  i: Integer;
  Tmp, Dst: string;
begin
  if GShuttingDown or (SaveLock = nil) or (GLock = nil) then Exit;
  SaveLock.Enter;
  try
    SetLength(Items, 0);
    SetLength(Manual, 0);
    GLock.Enter;
    try
      if GRunning and (not GCurrentFinished) and (GCurrentPath <> '') then
        Items := Items + [GCurrentPath];
      if GQueue <> nil then
        for i := 0 to GQueue.Count - 1 do
          Items := Items + [GQueue[i]];
      if GManual <> nil then
        for i := 0 to High(Items) do
          if GManual.ContainsKey(LowerCase(Items[i])) then
            Manual := Manual + [Items[i]];
    finally
      GLock.Leave;
    end;

    Obj := TJSONObject.Create;
    try
      Arr := TJSONArray.Create;
      for i := 0 to High(Items) do Arr.Add(Items[i]);
      ManArr := TJSONArray.Create;
      for i := 0 to High(Manual) do ManArr.Add(Manual[i]);
      Obj.AddPair('version', TJSONNumber.Create(1));
      Obj.AddPair('items', Arr);
      // Chave a mais, mesma versao: um arquivo antigo sem ela so restaura
      // tudo como automatico, que era o comportamento de antes.
      Obj.AddPair('manual', ManArr);
      Dst := QueueFilePath;
      Tmp := Dst + '.tmp';
      try
        ForceDirectories(ExtractFilePath(Dst));
        TFile.WriteAllText(Tmp, Obj.ToJSON, TEncoding.UTF8);
        // Troca atomica: fechar no meio da escrita nunca deixa o arquivo
        // pela metade — o que zeraria a fila na proxima leitura.
        if not MoveFileExW(PChar(Tmp), PChar(Dst), MOVEFILE_REPLACE_EXISTING) then
          Log('Transcribe: falha ao gravar a fila (erro %d).', [GetLastError]);
      except
        on E: Exception do
          Log('Transcribe: falha ao gravar a fila: %s', [E.Message]);
      end;
    finally
      Obj.Free;
    end;
  finally
    SaveLock.Leave;
  end;
end;

function ReadSavedQueueArray(const AKey: string): TArray<string>;
var
  Body: string;
  Root, V: TJSONValue;
  Arr: TJSONArray;
  i: Integer;
begin
  SetLength(Result, 0);
  try
    if not TFile.Exists(QueueFilePath) then Exit;
    Body := TFile.ReadAllText(QueueFilePath);
  except
    on E: Exception do
    begin
      Log('Transcribe: fila salva ilegivel: %s', [E.Message]);
      Exit;
    end;
  end;
  Root := TJSONObject.ParseJSONValue(Body);
  try
    if not (Root is TJSONObject) then Exit;
    V := TJSONObject(Root).GetValue(AKey);
    if not (V is TJSONArray) then Exit;
    Arr := TJSONArray(V);
    for i := 0 to Arr.Count - 1 do
      if (Arr.Items[i] is TJSONString) and (Trim(Arr.Items[i].Value) <> '') then
        Result := Result + [Arr.Items[i].Value];
  finally
    Root.Free;
  end;
end;

function LoadSavedQueue: TArray<string>;
begin
  Result := ReadSavedQueueArray('items');
end;

function LoadSavedManualPaths: TArray<string>;
begin
  Result := ReadSavedQueueArray('manual');
end;

function IsManual(const APath: string): Boolean;
begin
  Result := False;
  if (GLock = nil) or (GManual = nil) then Exit;
  GLock.Enter;
  try
    Result := GManual.ContainsKey(LowerCase(APath));
  finally
    GLock.Leave;
  end;
end;

function ResolveTranscribeLanguage: string;
// Idioma que vai pra API. O padrao e MANDAR, nunca deixar detectar:
// sem `language`, o faster-whisper decide pelos primeiros 30 s do audio,
// e quando erra (silencio, musica, ruido — comum numa faixa isolada) ele
// "transcreve" no idioma errado, que na pratica e TRADUZIR a fala
// inteira. Foi exatamente o sintoma: transcricoes saindo em outra lingua.
//
//   '' / 'app' -> idioma da interface do NoOBS (pt-BR -> pt)
//   'auto'     -> '' (a API detecta; so por escolha explicita)
//   outro      -> o codigo escolhido na tela
var
  Pref, Code: string;
  P: Integer;
begin
  Pref := LowerCase(Trim(GetConfigStr('transcribeLanguage', '')));
  if Pref = 'auto' then Exit('');
  if (Pref <> '') and (Pref <> 'app') then Exit(Pref);
  Code := OBSLang.CurrentLanguage;
  P := Pos('-', Code);
  if P > 0 then Code := Copy(Code, 1, P - 1);
  Result := LowerCase(Trim(Code));
end;

function CurrentTrack: Integer;      begin Result := GTrack; end;
function CurrentTrackCount: Integer; begin Result := GTrackCount; end;

procedure SetTrack(ATrack, ACount: Integer);
// Qual faixa esta sendo transcrita. So notifica quando MUDA: e uma vez
// por faixa, nao a cada tique.
var
  Mudou: Boolean;
begin
  if GLock = nil then Exit;
  GLock.Enter;
  try
    Mudou := (GTrack <> ATrack) or (GTrackCount <> ACount);
    GTrack := ATrack;
    GTrackCount := ACount;
  finally
    GLock.Leave;
  end;
  if Mudou then NotifyChanged;
end;

procedure SetStage(const AStage: string; AProgress: Double; AEta: Integer);
// Publica a etapa/progresso e avisa a UI. Chamado da worker a cada
// consulta ao servidor.
var
  Mudou: Boolean;
begin
  if GLock = nil then Exit;
  GLock.Enter;
  try
    Mudou := (GStage <> AStage) or (Abs(GProgress - AProgress) > 0.0005) or
             (GEta <> AEta);
    GStage := AStage;
    GProgress := AProgress;
    GEta := AEta;
  finally
    GLock.Leave;
  end;
  // So notifica quando algo mudou de fato: o progresso anda aos saltos
  // (janelas de 30s do Whisper), entao a maioria dos tiques e igual ao
  // anterior e empurrar tudo seria ruido no canal.
  if Mudou then NotifyChanged;
end;

function DoneCount: Integer;   begin Result := GDone;   end;
function FailedCount: Integer; begin Result := GFailed; end;
function BatchTotal: Integer;  begin Result := GBatchTotal; end;

function LastError: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GLastError; finally GLock.Leave; end;
end;

function LastErrorName: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GLastErrorName; finally GLock.Leave; end;
end;

function CurrentPath: string;
begin
  if GLock = nil then Exit('');
  GLock.Enter;
  try Result := GCurrentPath; finally GLock.Leave; end;
end;

function QueueRevision: Integer;
begin
  Result := GQueueRev;
end;

function QueuedPaths: TArray<string>;
// Copia sob o lock: a worker tira itens da fila a qualquer momento, e
// devolver a TList crua deixaria o Bridge iterando sobre ela sem lock.
var
  i: Integer;
begin
  Result := nil;
  if GLock = nil then Exit;
  GLock.Enter;
  try
    if GQueue = nil then Exit;
    SetLength(Result, GQueue.Count);
    for i := 0 to GQueue.Count - 1 do Result[i] := GQueue[i];
  finally
    GLock.Leave;
  end;
end;

function IndexInQueue(const APath: string): Integer;
// Caller segura o GLock. Mesma regra do AlreadyQueued: compara por PATH.
var
  i: Integer;
begin
  Result := -1;
  if GQueue = nil then Exit;
  for i := 0 to GQueue.Count - 1 do
    if SameText(GQueue[i], APath) then Exit(i);
end;

function MoveInQueue(const APath: string; ANewIndex: Integer): Boolean;
var
  Old: Integer;
begin
  Result := False;
  if (GLock = nil) or (APath = '') then Exit;
  GLock.Enter;
  try
    Old := IndexInQueue(APath);
    if Old < 0 then Exit;
    // Clamp em vez de recusar: a UI manda um indice calculado de posicao
    // de mouse, e a fila pode ter encolhido entre o gesto e a mensagem
    // (a worker pegou o primeiro item nesse meio-tempo).
    if ANewIndex < 0 then ANewIndex := 0;
    if ANewIndex > GQueue.Count - 1 then ANewIndex := GQueue.Count - 1;
    if ANewIndex = Old then Exit;
    GQueue.Move(Old, ANewIndex);
    Inc(GQueueRev);
    Result := True;
  finally
    GLock.Leave;
  end;
  if Result then
  begin
    Log('Transcribe: "%s" movido pra posicao %d da fila.', [APath, ANewIndex]);
    SaveQueue;
    NotifyChanged;
  end;
end;

function RemoveFromQueue(const APath: string): Boolean;
var
  Idx: Integer;
begin
  Result := False;
  if (GLock = nil) or (APath = '') then Exit;
  GLock.Enter;
  try
    Idx := IndexInQueue(APath);
    if Idx < 0 then Exit;
    GQueue.Delete(Idx);
    if GManual <> nil then GManual.Remove(LowerCase(APath));
    Inc(GQueueRev);
    // O total do lote conta o que VAI rodar. Sem descontar, a linha de
    // progresso ficaria presa em "6 de 7" com a fila vazia.
    if GBatchTotal > 0 then Dec(GBatchTotal);
    Result := True;
  finally
    GLock.Leave;
  end;
  if Result then
  begin
    Log('Transcribe: "%s" tirado da fila.', [APath]);
    SaveQueue;
    NotifyChanged;
  end;
end;

function HttpErrText(const AResp: IHTTPResponse): string;
// "HTTP 422" sozinho nao diz NADA — quem sabe o motivo e o CORPO da
// resposta, e ele estava sendo jogado fora. A Transcritor API e FastAPI,
// entao erro sai como {"detail": ...}: string nos erros de negocio e
// ARRAY nos de validacao (422). Pega o detail quando da, senao o corpo
// cru. Trunca e tira quebras de linha porque isto vira UMA linha na tela.
const
  MAX_LEN = 300;
var
  Body, Detail: string;
  Json, Val: TJSONValue;
begin
  Result := Format('HTTP %d', [AResp.StatusCode]);
  if Trim(AResp.StatusText) <> '' then
    Result := Result + ' ' + Trim(AResp.StatusText);
  Body := '';
  try Body := Trim(AResp.ContentAsString(TEncoding.UTF8)); except end;
  if Body = '' then Exit;
  Detail := Body;
  Json := TJSONObject.ParseJSONValue(Body);
  if Json <> nil then
  try
    if Json is TJSONObject then
    begin
      Val := TJSONObject(Json).GetValue('detail');
      if Val is TJSONString then Detail := TJSONString(Val).Value
      else if Val <> nil then Detail := Val.ToJSON;
    end;
  finally
    Json.Free;
  end;
  Detail := Trim(StringReplace(StringReplace(Detail, #13, ' ', [rfReplaceAll]),
    #10, ' ', [rfReplaceAll]));
  if Detail = '' then Exit;
  if Length(Detail) > MAX_LEN then Detail := Copy(Detail, 1, MAX_LEN) + '...';
  Result := Result + ': ' + Detail;
end;

function NetErrText(const AMsg: string): string;
// Falha de CONEXAO chega aqui como texto cru do WinINet — "Error sending
// data: (12152) O servidor retornou uma resposta invalida" — que descreve
// o sintoma e nao o que fazer. Container parado da justamente isso, e o
// usuario nao tem como ligar uma coisa a outra. Troca pela frase que diz
// o que houve e ONDE; o texto tecnico vai pro log, que e onde ele serve.
begin
  Log('Transcribe: falha de rede contra %s: %s', [HostBase, AMsg]);
  Result := OBSLang.T('error.transcribe.serverDown', ['host', HostBase]);
end;

function CheckHealth(const AHost: string): string;
// Roda na worker (a UI chama por um botao "Testar"). '' = ok.
// Devolve o texto CRU da falha de proposito: aqui o usuario apertou
// "Testar" e quer o diagnostico. Quem transcreve usa o serverDown.
var
  Http: TNetHTTPClient;
  Resp: IHTTPResponse;
  Base: string;
begin
  Base := Trim(AHost);
  while (Base <> '') and (Base[Length(Base)] = '/') do
    Delete(Base, Length(Base), 1);
  if Base = '' then Exit(OBSLang.T('error.transcribe.emptyHost'));
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := HEALTH_TIMEOUT_MS;
    Http.ResponseTimeout := HEALTH_TIMEOUT_MS;
    try
      Resp := Http.Get(Base + HEALTH_PATH);
    except
      on E: Exception do Exit(E.Message);
    end;
    if Resp = nil then Exit(OBSLang.T('error.transcribe.noResponse'));
    if Resp.StatusCode <> 200 then
      Exit(HttpErrText(Resp));
    Result := '';
  finally
    Http.Free;
  end;
end;

function ExtractPlainText(AObj: TJSONObject): string;
// Texto puro pra BUSCA. Prefere o campo 'text' da resposta; se faltar,
// remonta a partir dos turnos. Nunca falha — busca com texto vazio so
// nao acha nada, o que e melhor que abortar a transcricao inteira.
var
  Arr: TJSONValue;
  i: Integer;
  Turn: TJSONValue;
  S: string;
  SB: TStringBuilder;
begin
  Result := '';
  if AObj = nil then Exit;
  if AObj.TryGetValue<string>('text', S) and (Trim(S) <> '') then Exit(S);
  Arr := AObj.GetValue('turns');
  if not (Arr is TJSONArray) then Exit;
  SB := TStringBuilder.Create;
  try
    for i := 0 to TJSONArray(Arr).Count - 1 do
    begin
      Turn := TJSONArray(Arr).Items[i];
      if not (Turn is TJSONObject) then Continue;
      if TJSONObject(Turn).TryGetValue<string>('text', S) then
      begin
        if SB.Length > 0 then SB.Append(' ');
        SB.Append(Trim(S));
      end;
    end;
    Result := SB.ToString;
  finally
    SB.Free;
  end;
end;

function RunHidden(const AExe, AArgs: string; ATimeoutMs: Cardinal;
  out AOutput: string): Integer;
// Roda um programa de console SEM janela e devolve o codigo de saida
// (-1 = nao rodou ou estourou o tempo). stdout + stderr voltam em AOutput.
// Existe SO pro diagnostico do ambiente — toda midia continua in-process.
//
// O pipe e lido DURANTE a espera, nao so no fim: se a saida encher o
// buffer do pipe (4 KB), o filho trava escrevendo e a espera acabaria em
// timeout com o processo vivo.
var
  SA: TSecurityAttributes;
  ReadPipe, WritePipe: THandle;
  SI: TStartupInfo;
  PI: TProcessInformation;
  Cmd: string;
  Buf: array[0..4095] of Byte;
  Avail, Got, Code, W: DWORD;
  Raw: TBytes;
  Start: UInt64;

  procedure Drain;
  var
    n: Integer;
  begin
    while PeekNamedPipe(ReadPipe, nil, 0, nil, @Avail, nil) and (Avail > 0) do
    begin
      if (not ReadFile(ReadPipe, Buf, SizeOf(Buf), Got, nil)) or (Got = 0) then
        Break;
      n := Length(Raw);
      SetLength(Raw, n + Integer(Got));
      Move(Buf[0], Raw[n], Got);
    end;
  end;

begin
  Result := -1;
  AOutput := '';
  SetLength(Raw, 0);
  FillChar(SA, SizeOf(SA), 0);
  SA.nLength := SizeOf(SA);
  SA.bInheritHandle := True;
  if not CreatePipe(ReadPipe, WritePipe, @SA, 0) then Exit;
  try
    // So a ponta de ESCRITA vai pro filho. Herdada, a de leitura manteria
    // o pipe aberto do lado dele e o ReadFile nunca veria o fim.
    SetHandleInformation(ReadPipe, HANDLE_FLAG_INHERIT, 0);
    FillChar(SI, SizeOf(SI), 0);
    SI.cb := SizeOf(SI);
    SI.dwFlags := STARTF_USESTDHANDLES or STARTF_USESHOWWINDOW;
    SI.wShowWindow := SW_HIDE;
    SI.hStdOutput := WritePipe;
    SI.hStdError := WritePipe;
    Cmd := '"' + AExe + '" ' + AArgs;
    UniqueString(Cmd);   // o CreateProcessW pode escrever na linha de comando
    FillChar(PI, SizeOf(PI), 0);
    if not CreateProcessW(nil, PChar(Cmd), nil, nil, True, CREATE_NO_WINDOW,
      nil, nil, SI, PI) then Exit;
    // O filho ja tem a copia dele; fechar a nossa e o que faz o pipe
    // terminar quando ele sai.
    CloseHandle(WritePipe);
    WritePipe := 0;
    try
      Start := GetTickCount64;
      repeat
        W := WaitForSingleObject(PI.hProcess, 50);
        Drain;
        if (W <> WAIT_OBJECT_0) and (GetTickCount64 - Start > ATimeoutMs) then
        begin
          TerminateProcess(PI.hProcess, 1);
          Exit;   // Result segue -1
        end;
      until W = WAIT_OBJECT_0;
      Drain;
      if GetExitCodeProcess(PI.hProcess, Code) then Result := Integer(Code);
    finally
      CloseHandle(PI.hThread);
      CloseHandle(PI.hProcess);
    end;
  finally
    if WritePipe <> 0 then CloseHandle(WritePipe);
    CloseHandle(ReadPipe);
    if Length(Raw) > 0 then AOutput := Trim(TEncoding.UTF8.GetString(Raw));
  end;
end;

procedure ParseHost(const AHost: string; out AName: string; out APort: Integer);
// 'http://localhost:8000/' -> ('localhost', 8000). Aceita IPv6 entre
// colchetes. Sem porta, vale a do esquema.
var
  S: string;
  P: Integer;
begin
  S := Trim(AHost);
  if StartsText('https://', S) then APort := 443 else APort := 80;
  P := Pos('://', S);
  if P > 0 then Delete(S, 1, P + 2);
  P := Pos('/', S);
  if P > 0 then S := Copy(S, 1, P - 1);
  AName := S;
  if (S <> '') and (S[1] = '[') then
  begin
    P := Pos(']', S);
    if P > 0 then
    begin
      AName := Copy(S, 2, P - 2);
      if (P < Length(S)) and (S[P + 1] = ':') then
        APort := StrToIntDef(Copy(S, P + 2, MaxInt), APort);
    end;
  end
  else
  begin
    P := LastDelimiter(':', S);
    if P > 0 then
    begin
      AName := Copy(S, 1, P - 1);
      APort := StrToIntDef(Copy(S, P + 1, MaxInt), APort);
    end;
  end;
  AName := LowerCase(Trim(AName));
end;

function DiagnoseSetup(const AHost: string): TTranscribeSetup;
// Em ETAPAS, e cada uma so roda se a anterior passou — a tela mostra o
// primeiro degrau que falta, com a instrucao dele, e nao uma lista de
// coisas que talvez estejam erradas.
//
//   1. O servidor responde?        sim -> pronto, nada mais importa
//   2. O host e desta maquina?     nao -> servidor remoto: Docker local
//                                         nao tem nada a ver
//   3. WSL instalado?              (o Docker Desktop precisa dele)
//   4. Docker instalado?
//   5. Docker em execucao?
//   6. Ha container publicando a porta? parado, rodando, de outra imagem?
//
// O passo 6 existe pra NAO mandar o usuario rodar `docker run` de novo
// quando o container ja existe e so esta parado: o segundo brigaria pela
// mesma porta. Visto na pratica — container criado sem --restart, morto
// por um reinicio, `Exited (255)`.
//
// O container e achado pela PORTA publicada, nao pelo nome da imagem:
// quem fez build local tem `transcritor-api:latest`, nao a do Docker Hub.
var
  HostName, Wsl, Docker, ProgFiles, Outp: string;
  Lines, Parts: TArray<string>;
  Buf: array[0..MAX_PATH] of Char;
  FilePart: PChar;
  i: Integer;
begin
  Result := Default(TTranscribeSetup);

  Result.ServerError := CheckHealth(AHost);
  Result.ServerOk := Result.ServerError = '';
  ParseHost(AHost, HostName, Result.Port);
  Result.IsLocal := (HostName = 'localhost') or (HostName = '127.0.0.1') or
    (HostName = '::1') or (HostName = '0.0.0.0') or
    SameText(HostName, GetEnvironmentVariable('COMPUTERNAME'));
  if Result.ServerOk or not Result.IsLocal then Exit;

  // WSL: em versoes novas do Windows o wsl.exe existe como "stub" mesmo
  // sem WSL instalado, entao a existencia do arquivo nao basta — o que
  // decide e o `--status` sair com 0.
  Wsl := IncludeTrailingPathDelimiter(GetEnvironmentVariable('WINDIR')) +
    'System32\wsl.exe';
  Result.WslOk := FileExists(Wsl) and
    (RunHidden(Wsl, '--status', DIAG_WSL_TIMEOUT_MS, Outp) = 0);

  // Docker: primeiro pelo PATH, depois no lugar padrao do Docker Desktop.
  // O PATH do NoOBS e o do momento em que ele abriu — quem instalou o
  // Docker com o app aberto nao o teria ali.
  Docker := '';
  FilePart := nil;
  if SearchPath(nil, 'docker.exe', nil, Length(Buf), @Buf[0], FilePart) > 0 then
    Docker := Buf;
  ProgFiles := IncludeTrailingPathDelimiter(GetEnvironmentVariable('ProgramFiles'));
  if (Docker = '') and FileExists(ProgFiles + 'Docker\Docker\resources\bin\docker.exe') then
    Docker := ProgFiles + 'Docker\Docker\resources\bin\docker.exe';
  if FileExists(ProgFiles + 'Docker\Docker\Docker Desktop.exe') then
    Result.DockerDesktopPath := ProgFiles + 'Docker\Docker\Docker Desktop.exe';
  Result.DockerInstalled := (Docker <> '') or (Result.DockerDesktopPath <> '');
  if Docker = '' then Exit;

  Result.DockerRunning :=
    RunHidden(Docker, 'info --format "{{.ServerVersion}}"',
      DIAG_DOCKER_TIMEOUT_MS, Outp) = 0;
  if not Result.DockerRunning then Exit;

  if RunHidden(Docker,
       Format('ps -a --filter "publish=%d" --format "{{.ID}}|{{.Image}}|{{.State}}|{{.Status}}"',
         [Result.Port]), DIAG_DOCKER_TIMEOUT_MS, Outp) = 0 then
  begin
    Lines := Outp.Split([#13, #10], TStringSplitOptions.ExcludeEmpty);
    // Varios parados podem publicar a mesma porta; so um roda. Prefere o
    // que esta rodando, senao o primeiro listado (o mais recente).
    for i := 0 to High(Lines) do
    begin
      Parts := Lines[i].Split(['|']);
      if Length(Parts) < 4 then Continue;
      if (Result.ContainerId = '') or SameText(Trim(Parts[2]), 'running') then
      begin
        Result.ContainerId := Trim(Parts[0]);
        Result.ContainerImage := Trim(Parts[1]);
        Result.ContainerState := LowerCase(Trim(Parts[2]));
        Result.ContainerStatus := Trim(Parts[3]);
        if Result.ContainerState = 'running' then Break;
      end;
    end;
  end;

  if (Result.ContainerId <> '') and
     (RunHidden(Docker, 'inspect ' + Result.ContainerId +
        ' --format "{{.HostConfig.RestartPolicy.Name}}"',
        DIAG_DOCKER_TIMEOUT_MS, Outp) = 0) then
    Result.RestartPolicy := LowerCase(Trim(Outp));
end;

function ServerDiarizes: Boolean;
// A Transcritor API tem duas versoes: o container (WhisperX), que separa
// falantes, e a nativa do Windows (Qwen3 + audio.cpp, na GPU), que nao
// separa e RECUSA diarization=true com HTTP 400. O /health da nativa diz o
// motor ("engine": "audiocpp"); o container 2.0 nao manda o campo. Entao:
// sem "engine", ou "whisperx", pede a separacao; qualquer outro, nao.
// Na duvida (health sem resposta) pede, que e o comportamento de sempre —
// se o servidor recusar, o motivo aparece no envio.
var
  Http: TNetHTTPClient;
  Resp: IHTTPResponse;
  Json: TJSONValue;
  Engine: string;
begin
  Result := True;
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := HEALTH_TIMEOUT_MS;
    Http.ResponseTimeout := HEALTH_TIMEOUT_MS;
    try
      Resp := Http.Get(HostBase + HEALTH_PATH);
    except
      Exit;
    end;
    if (Resp = nil) or (Resp.StatusCode <> 200) then Exit;
    Json := TJSONObject.ParseJSONValue(Resp.ContentAsString(TEncoding.UTF8));
    try
      if (Json is TJSONObject) and
         TJSONObject(Json).TryGetValue<string>('engine', Engine) then
        Result := SameText(Engine, 'whisperx');
    finally
      Json.Free;
    end;
  finally
    Http.Free;
  end;
end;

function SubmitJob(const AAudioPath: string; out AJobId: string): string;
// POST /jobs — devolve na hora um identificador (HTTP 202), sem esperar a
// transcricao. O upload e a unica parte demorada aqui, e ele e rapido
// porque so vai audio (ver cabecalho da unit).
var
  Http: TNetHTTPClient;
  Data: TMultipartFormData;
  Resp: IHTTPResponse;
  Lang, Body: string;
  Json: TJSONValue;
begin
  AJobId := '';
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    // So o upload: a API responde assim que enfileira.
    Http.ResponseTimeout := SUBMIT_TIMEOUT_MS;
    Http.UserAgent := 'NoOBS';
    Data := TMultipartFormData.Create;
    try
      Data.AddFile('file', AAudioPath);
      // Idioma SEMPRE explicito, salvo escolha de "detectar" — deixar a
      // API adivinhar era o que traduzia a transcricao. Ver
      // ResolveTranscribeLanguage.
      Lang := ResolveTranscribeLanguage;
      if Lang <> '' then Data.AddField('language', Lang);
      if Lang = '' then Log('Transcribe: idioma: deteccao automatica.')
      else Log('Transcribe: idioma: %s', [Lang]);
      // Separacao por falante — e o que torna o painel do player util.
      // So quando o servidor sabe fazer: a versao nativa recusa com 400
      // (ver ServerDiarizes). Sem ela, faixas isoladas ainda levam o nome
      // do dispositivo como falante (LabelTrackSpeakers).
      if ServerDiarizes then
        Data.AddField('diarization', 'true')
      else
        Log('Transcribe: servidor sem separacao por falante; enviando sem diarizacao.');
      // Alinhamento por palavra. O padrao da API ja e true, mas vai
      // EXPLICITO porque o TurnsFromWords depende dele: sem os
      // `segments[].words` o recorte de turnos cai no `turns` cru da API,
      // que so quebra em troca de falante (pegadinha #60k).
      Data.AddField('alignment', 'true');
      try
        Resp := Http.Post(HostBase + JOBS_PATH, Data);
      except
        on E: Exception do Exit(NetErrText(E.Message));
      end;
      if Resp = nil then Exit(OBSLang.T('error.transcribe.noResponse'));
      // 202 e o esperado; aceita 200 tambem pra nao quebrar se a API
      // mudar o codigo de sucesso.
      if (Resp.StatusCode <> 202) and (Resp.StatusCode <> 200) then
        Exit(HttpErrText(Resp));
      Body := Resp.ContentAsString(TEncoding.UTF8);
      Json := TJSONObject.ParseJSONValue(Body);
      if not (Json is TJSONObject) then
      begin
        if Json <> nil then Json.Free;
        Exit(OBSLang.T('error.transcribe.jobsNotJson'));
      end;
      try
        TJSONObject(Json).TryGetValue<string>('job_id', AJobId);
      finally
        Json.Free;
      end;
      if Trim(AJobId) = '' then Exit(OBSLang.T('error.transcribe.noJobId'));
      Result := '';
    finally
      Data.Free;
    end;
  finally
    Http.Free;
  end;
end;

procedure DeleteJob(const AJobId: string);
// Melhor esforco: se o cancelamento nao chegar, o job termina sozinho e
// expira pelo JOB_TTL_SECONDS da API. Nao vale falhar por causa disso.
var
  Http: TNetHTTPClient;
begin
  if AJobId = '' then Exit;
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    Http.ResponseTimeout := POLL_TIMEOUT_MS;
    try Http.Delete(HostBase + JOBS_PATH + '/' + AJobId); except end;
    Log('Transcribe: job %s cancelado no servidor.', [AJobId]);
  finally
    Http.Free;
  end;
end;

function PollJob(const AJobId: string; out AStatus, AStage, AError: string;
  out AProgress: Double; out AEta: Integer; out AResult: string): string;
// GET /jobs/{id}. Devolve '' se a consulta em si funcionou (mesmo com
// status "error" do job — isso vem em AStatus/AError); senao a falha de
// rede.
var
  Http: TNetHTTPClient;
  Resp: IHTTPResponse;
  Body: string;
  Json: TJSONValue;
  Obj: TJSONObject;
  ResVal: TJSONValue;
  Eta: Double;
begin
  AStatus := '';
  AStage := '';
  AError := '';
  AResult := '';
  AProgress := 0;
  AEta := -1;
  Http := TNetHTTPClient.Create(nil);
  try
    Http.ConnectionTimeout := CONNECT_TIMEOUT_MS;
    Http.ResponseTimeout := POLL_TIMEOUT_MS;
    Http.UserAgent := 'NoOBS';
    try
      Resp := Http.Get(HostBase + JOBS_PATH + '/' + AJobId + STATUS_QUERY);
    except
      on E: Exception do Exit(NetErrText(E.Message));
    end;
    if Resp = nil then Exit(OBSLang.T('error.transcribe.noResponse'));
    if Resp.StatusCode = 404 then Exit(OBSLang.T('error.transcribe.jobGone'));
    if Resp.StatusCode <> 200 then Exit(HttpErrText(Resp));
    Body := Resp.ContentAsString(TEncoding.UTF8);
    Json := TJSONObject.ParseJSONValue(Body);
    if not (Json is TJSONObject) then
    begin
      if Json <> nil then Json.Free;
      Exit(OBSLang.T('error.transcribe.statusNotJson'));
    end;
    try
      Obj := TJSONObject(Json);
      Obj.TryGetValue<string>('status', AStatus);
      Obj.TryGetValue<string>('stage', AStage);
      Obj.TryGetValue<string>('error', AError);
      Obj.TryGetValue<Double>('progress', AProgress);
      // eta_seconds so aparece a partir de ~10% (antes disso extrapolar
      // nao faz sentido). -1 = ainda nao da pra dizer.
      if Obj.TryGetValue<Double>('eta_seconds', Eta) then AEta := Round(Eta);
      // O `result` so vem quando status = done, e e o MESMO JSON que o
      // /transcribe devolve. E so vem porque pedimos: desde a v2 da API o
      // GET /jobs/{id} traz SO o andamento por padrao, e o resultado
      // depende do ?incluir_resultado=true (ver STATUS_QUERY). Sem ele a
      // transcricao terminava e o job "concluia sem resultado".
      ResVal := Obj.GetValue('result');
      if ResVal is TJSONObject then AResult := ResVal.ToJSON;
    finally
      Json.Free;
    end;
    Result := '';
  finally
    Http.Free;
  end;
end;

type
  TTurn = record
    Speaker: string;      // rotulo final (nome da faixa, + falante se houver)
    StartS, EndS: Double;
    Text: string;
    Track: Integer;       // de qual faixa veio — o dedup so cruza faixas
    Drop: Boolean;
  end;

function NormalizeForCompare(const S: string): string;
// Minusculas, so letras/digitos, espaco unico. A pontuacao do Whisper
// varia entre duas passagens do MESMO trecho (o eco chega mais fraco),
// entao compara-la geraria falso negativo no dedup.
var
  i: Integer;
  Ch: Char;
  SB: TStringBuilder;
  LastSpace: Boolean;
begin
  SB := TStringBuilder.Create;
  try
    LastSpace := True;
    for i := 1 to Length(S) do
    begin
      Ch := S[i];
      if Ch.IsLetterOrDigit then
      begin
        SB.Append(Ch.ToLower);
        LastSpace := False;
      end
      else if not LastSpace then
      begin
        SB.Append(' ');
        LastSpace := True;
      end;
    end;
    Result := Trim(SB.ToString);
  finally
    SB.Free;
  end;
end;

function WordSimilarity(const A, B: string): Double;
// CONTENCAO: quantas das palavras do turno CURTO aparecem no longo.
//
// Nao e Dice (2*comuns/(total+total)), e a diferenca decide o caso que
// mais importa: o eco PARCIAL. O microfone costuma pegar so um pedaco do
// que saiu no alto-falante, e Dice pune diferenca de tamanho — "e adiar
// tudo" dentro de "entao eu acho que a melhor saida aqui e adiar tudo
// pra semana que vem" da 0,33 no Dice (nao dedupa) e 1,00 aqui.
//
// Medido sobre 10 pares (5 ecos reais, 5 falas legitimas): Dice pegou
// 4/5 ecos, contencao pegou 5/5 — os dois com zero falso positivo,
// desde que o turno curto tenha 2+ palavras.
//
// Palavra e nao caractere de proposito: o eco troca uma palavra aqui e
// ali, mas as demais coincidem; distancia de edicao seria mais cara e
// mais sensivel justamente a essas trocas.
var
  WA, WB: TArray<string>;
  UsedB: TArray<Boolean>;
  i, j, Common, Shortest: Integer;
begin
  Result := 0;
  WA := A.Split([' '], TStringSplitOptions.ExcludeEmpty);
  WB := B.Split([' '], TStringSplitOptions.ExcludeEmpty);
  if (Length(WA) = 0) or (Length(WB) = 0) then Exit;
  Shortest := Min(Length(WA), Length(WB));
  if Shortest < DEDUP_MIN_WORDS then Exit;
  SetLength(UsedB, Length(WB));
  Common := 0;
  for i := 0 to High(WA) do
    for j := 0 to High(WB) do
      if (not UsedB[j]) and (WA[i] = WB[j]) then
      begin
        UsedB[j] := True;
        Inc(Common);
        Break;
      end;
  Result := Common / Shortest;
end;

function TimeOverlapRatio(const A, B: TTurn): Double;
// Fracao do turno mais CURTO coberta pela intersecao. Dividir pelo mais
// curto e deliberado: o eco costuma sair picado em turnos menores, e
// dividir pela uniao faria um turno curto dentro de um longo pontuar
// baixo justamente no caso que interessa.
var
  Inter, Shortest: Double;
begin
  Result := 0;
  Inter := Min(A.EndS, B.EndS) - Max(A.StartS, B.StartS);
  if Inter <= 0 then Exit;
  Shortest := Min(A.EndS - A.StartS, B.EndS - B.StartS);
  if Shortest <= 0 then Exit;
  Result := Inter / Shortest;
end;

function PlanTracks(const APath: string; out ANames: TArray<string>;
  out AStreams: TArray<Integer>): Boolean;
// Decide QUAIS streams de audio vao pra transcricao.
//
// Stream 0 e a MISTURA; 1..N-1 sao as ISOLADAS por dispositivo. Mandar
// as isoladas separadas da a atribuicao de falante DE GRACA — cada faixa
// e um dispositivo, e o nome dele foi escrito como titulo da stream no
// MKV pelo OBSEngine. Diarizacao dentro da faixa vira detalhe, nao a
// unica pista.
//
// O custo e uma transcricao POR FAIXA, e o container roda uma de cada
// vez. Dai o corte: so vale com DUAS ou mais isoladas. Com uma so, a
// mistura tem exatamente o mesmo conteudo e sairia o mesmo texto pelo
// dobro do tempo.
var
  Rep: TProbeReport;
  Auds: TStreamArray;
  i: Integer;
begin
  SetLength(ANames, 0);
  SetLength(AStreams, 0);
  Result := False;

  if not GetConfigBool('transcribePerTrack', True) then Exit;
  if not Probe(APath, Rep) then Exit;
  Auds := Rep.AudioStreams;
  if Length(Auds) < 3 then Exit;   // mistura + no minimo 2 isoladas

  SetLength(ANames, Length(Auds) - 1);
  SetLength(AStreams, Length(Auds) - 1);
  for i := 1 to High(Auds) do
  begin
    AStreams[i - 1] := i;
    // O titulo vem do BuildTrackNames, gravado como metadata da stream.
    // Sem ele (gravacao de outra ferramenta) cai num rotulo generico.
    ANames[i - 1] := Trim(Auds[i].Title);
    if ANames[i - 1] = '' then
      ANames[i - 1] := OBSLang.T('transcript.trackN', ['n', IntToStr(i + 1)]);
  end;
  Result := True;
end;

function TrackHasSound(const AAudioPath: string): Boolean;
// Decoda a faixa e olha o pico. Faixa muda (microfone que ninguem usou,
// alto-falante de uma gravacao silenciosa) nao vai pro servidor: gastaria
// minutos de fila pra voltar com zero turnos. Decodar custa segundos; a
// transcricao custaria minutos.
var
  Peaks: TArray<Single>;
  i: Integer;
begin
  Result := True;   // na duvida, MANDA: perder fala e pior que gastar tempo
  if not FFmpegOps.ComputeAudioPeaks(AAudioPath, SILENCE_BUCKETS, Peaks) then Exit;
  if Length(Peaks) = 0 then Exit;
  for i := 0 to High(Peaks) do
    if Peaks[i] >= SILENCE_PEAK_MIN then Exit;
  Result := False;
end;

function RunLocal(const AAudioPath: string; ATrack, ATrackCount: Integer;
  AStartTick: Cardinal; var AEstPerTrackSec: Integer; out ABody: string): string;
// O RunJob do motor LOCAL (OBSLocalAsr). Mesmas regras de progresso do
// caminho da API: fracao do CONJUNTO de faixas e "faltam" somando as
// faixas que nem comecaram, pela regua da primeira que terminou.
var
  Canceled: Boolean;
  Est: Integer;
begin
  Est := AEstPerTrackSec;
  Result := OBSLocalAsr.Transcribe(AAudioPath, ResolveTranscribeLanguage,
    procedure(const AStage: string; AFraction: Double)
    var
      ElapsedSec, TrackEta: Integer;
      Total: Double;
    begin
      ElapsedSec := Integer((GetTickCount - AStartTick) div 1000);
      TrackEta := -1;
      // Extrapolar antes de 10% chuta demais (a decodificacao e rapida e
      // distorce a conta) — mesma regra da API.
      if AFraction >= 0.1 then
        TrackEta := Round(ElapsedSec * (1 - AFraction) / AFraction);
      if (TrackEta >= 0) and (Est > 0) then
        Inc(TrackEta, (ATrackCount - ATrack - 1) * Est);
      if ATrackCount <= 1 then Total := AFraction
      else Total := (ATrack + AFraction) / ATrackCount;
      SetStage(AStage, Total, TrackEta);
    end,
    function: Boolean
    begin
      Result := CancelRequested or ShuttingDown;
    end,
    ABody, Canceled);
  if Canceled then
  begin
    if ShuttingDown then Exit(OBSLang.T('error.transcribe.shuttingDown'));
    Exit(CANCELED_MARK);
  end;
  if Result <> '' then Exit;
  if AEstPerTrackSec <= 0 then
    AEstPerTrackSec := Integer((GetTickCount - AStartTick) div 1000);
  if ATrackCount <= 1 then SetStage('done', 1, -1)
  else SetStage('done', (ATrack + 1) / ATrackCount, -1);
end;

function RunJob(const AAudioPath: string; ATrack, ATrackCount: Integer;
  var AEstPerTrackSec: Integer; out ABody: string): string;
// Sobe UM arquivo de audio e acompanha ate o fim. '' = sucesso (JSON em
// ABody), CANCELED_MARK se o usuario cancelou, senao a mensagem de erro.
//
// O progresso publicado e o do CONJUNTO de faixas, nao o da faixa: com 3
// faixas, a segunda a 50% vale 50% do total, senao a barra voltaria pra
// zero tres vezes. E o "faltam" soma as faixas que ainda nem comecaram,
// usando o tempo medido da primeira que terminou — antes disso nao ha
// base pra estimar e sai so o da faixa atual.
var
  JobId, Status, Stage, JobErr, ResJson: string;
  Prog: Double;
  Eta, TotalEta: Integer;
  StartTick: Cardinal;

  function Overall(AInner: Double): Double;
  begin
    if ATrackCount <= 1 then Exit(AInner);
    if AInner < 0 then AInner := 0;
    Result := (ATrack + AInner) / ATrackCount;
  end;

begin
  ABody := '';
  StartTick := GetTickCount;
  SetTrack(ATrack, ATrackCount);

  if UseLocalEngine then
  begin
    // Motor local: mesmo contrato (JSON da API em ABody); progresso e ETA
    // calculados aqui — nao ha servidor que os mande.
    Result := RunLocal(AAudioPath, ATrack, ATrackCount, StartTick,
      AEstPerTrackSec, ABody);
    Exit;
  end;

  SetStage('uploading', Overall(0), -1);
  Result := SubmitJob(AAudioPath, JobId);
  if Result <> '' then Exit;
  Log('Transcribe: job %s aceito (faixa %d de %d).',
    [JobId, ATrack + 1, ATrackCount]);

  while True do
  begin
    if ShuttingDown then
    begin
      DeleteJob(JobId);
      Exit(OBSLang.T('error.transcribe.shuttingDown'));
    end;
    if CancelRequested then
    begin
      DeleteJob(JobId);
      Exit(CANCELED_MARK);
    end;

    Result := PollJob(JobId, Status, Stage, JobErr, Prog, Eta, ResJson);
    if Result <> '' then Exit;

    if Status = 'done' then
    begin
      ABody := ResJson;
      if Trim(ABody) = '' then Exit(OBSLang.T('error.transcribe.emptyResult'));
      // A primeira faixa concluida vira a regua pras que faltam.
      if AEstPerTrackSec <= 0 then
        AEstPerTrackSec := Integer((GetTickCount - StartTick) div 1000);
      SetStage('done', Overall(1), -1);
      Result := '';
      Exit;
    end;
    if Status = 'error' then
    begin
      if JobErr = '' then JobErr := OBSLang.T('error.transcribe.serverSilent');
      Exit(JobErr);
    end;

    TotalEta := Eta;
    if (TotalEta >= 0) and (AEstPerTrackSec > 0) then
      Inc(TotalEta, (ATrackCount - ATrack - 1) * AEstPerTrackSec);
    SetStage(Stage, Overall(Prog), TotalEta);

    // Espera interrompivel: o StopEvent tira a thread daqui na hora, em
    // vez de deixar o fechamento do app esperando um Sleep.
    if WaitForSingleObject(StopEvent, POLL_INTERVAL_MS) = WAIT_OBJECT_0 then
    begin
      DeleteJob(JobId);
      Exit(OBSLang.T('error.transcribe.shuttingDown'));
    end;
  end;
end;

function EndsSentence(const S: string): Boolean;
// Ultima letra util e ponto final / interrogacao / exclamacao / reticencia.
var
  T: string;
begin
  T := TrimRight(S);
  Result := (T <> '') and CharInSet(T[Length(T)], ['.', '?', '!']);
end;

procedure AppendWord(var ADest: string; const AWord: string);
// Junta a palavra ao texto do turno inserindo o espaco SO quando falta.
//
// O formato mudou com o WhisperX: o faster-whisper devolvia a palavra
// com o espaco na frente (" Bom"), o alinhador devolve ela pelada
// ("Bom") — e e por isso que o _build_turns da propria API junta com
// " ".join(). Somar direto emendava tudo ("Bomdiapessoal"); somar sempre
// um espaco dobraria no formato antigo, que ainda aparece nos segmentos
// que nao passaram pelo alinhamento (idioma sem alinhador embutido).
begin
  if AWord = '' then Exit;
  if (ADest <> '') and (ADest[Length(ADest)] <> ' ') and (AWord[1] <> ' ') then
    ADest := ADest + ' ';
  ADest := ADest + AWord;
end;

type
  // Uma palavra alinhada de UMA faixa. E a unidade do dedup: comparar
  // turnos inteiros apagava fala legitima junto com o eco (medido: 4,8%
  // das palavras legitimas perdidas contra 0,3% por palavra).
  TTimedWord = record
    Text: string;        // como vai pro turno (inclui o texto de palavras
                         // sem timestamp que vieram logo antes dela)
    Norm: string;        // SO a propria palavra normalizada; '' = fora do
                         // casamento (pontuacao solta, segmento inteiro)
    Speaker: string;
    StartS, EndS: Double;
    Score: Double;       // confianca do alinhador: o eco chega mais fraco
    IsSegment: Boolean;  // segmento sem NENHUMA palavra alinhada, inteiro
    Drop: Boolean;
  end;
  TTimedWords = TArray<TTimedWord>;

procedure CollectWords(AObj: TJSONObject; var AWords: TTimedWords);
// Le segments[].words de UMA resposta da API, na ordem do tempo.
//
// Dois casos da v2 que moldam isto (pegadinha #60a):
//   - palavra SEM start/end (numero, simbolo que o alinhador nao
//     reconheceu): o texto dela entra na PROXIMA palavra alinhada, sem
//     mexer em tempo nenhum — "Custou 1500 reais" nao vira "Custou reais";
//   - segmento sem nenhuma palavra alinhada: vira uma pseudo-palavra com
//     o tempo e o texto do proprio segmento (o fallback do _build_turns da
//     API), fora do dedup.
var
  SegsVal, WordsVal: TJSONValue;
  Segs, Words: TJSONArray;
  Seg, W: TJSONObject;
  i, j, n, SegFirst: Integer;
  WStart, WEnd, Sc: Double;
  WText, WSpk, Pending, SegText, SegSpk: string;
  HasTs: Boolean;
begin
  SegsVal := AObj.GetValue('segments');
  if not (SegsVal is TJSONArray) then Exit;
  Segs := TJSONArray(SegsVal);
  for i := 0 to Segs.Count - 1 do
  begin
    if not (Segs.Items[i] is TJSONObject) then Continue;
    Seg := TJSONObject(Segs.Items[i]);
    SegFirst := Length(AWords);
    Pending := '';
    WordsVal := Seg.GetValue('words');
    if WordsVal is TJSONArray then
    begin
      Words := TJSONArray(WordsVal);
      for j := 0 to Words.Count - 1 do
      begin
        if not (Words.Items[j] is TJSONObject) then Continue;
        W := TJSONObject(Words.Items[j]);
        WText := ''; WSpk := ''; WStart := 0; WEnd := 0;
        W.TryGetValue<string>('word', WText);
        if Trim(WText) = '' then Continue;
        HasTs := W.TryGetValue<Double>('start', WStart);
        if not W.TryGetValue<Double>('end', WEnd) then WEnd := WStart;
        if not HasTs then
        begin
          AppendWord(Pending, WText);
          Continue;
        end;
        W.TryGetValue<string>('speaker', WSpk);
        // A API nao poe speaker na palavra em toda configuracao.
        if WSpk = '' then Seg.TryGetValue<string>('speaker', WSpk);
        // `score` na v2; `probability` nas respostas antigas guardadas.
        if not W.TryGetValue<Double>('score', Sc) then
          if not W.TryGetValue<Double>('probability', Sc) then Sc := 0;
        n := Length(AWords);
        SetLength(AWords, n + 1);
        AWords[n] := Default(TTimedWord);
        AWords[n].Text := '';
        AppendWord(AWords[n].Text, Pending);
        AppendWord(AWords[n].Text, WText);
        Pending := '';
        AWords[n].Norm := NormalizeForCompare(WText);
        AWords[n].Speaker := WSpk;
        AWords[n].StartS := WStart;
        AWords[n].EndS := WEnd;
        AWords[n].Score := Sc;
      end;
    end;

    if Length(AWords) > SegFirst then
    begin
      // Sobrou pendente no fim do segmento: vai pra ultima palavra dele.
      if Pending <> '' then AppendWord(AWords[High(AWords)].Text, Pending);
    end
    else
    begin
      SegText := ''; SegSpk := ''; WStart := 0; WEnd := 0;
      Seg.TryGetValue<string>('text', SegText);
      Seg.TryGetValue<string>('speaker', SegSpk);
      Seg.TryGetValue<Double>('start', WStart);
      Seg.TryGetValue<Double>('end', WEnd);
      if Trim(SegText) <> '' then
      begin
        n := Length(AWords);
        SetLength(AWords, n + 1);
        AWords[n] := Default(TTimedWord);
        AWords[n].Text := Trim(SegText);
        AWords[n].Speaker := SegSpk;
        AWords[n].StartS := WStart;
        AWords[n].EndS := WEnd;
        AWords[n].IsSegment := True;
      end;
    end;
  end;
end;

procedure TurnsFromWordList(const AWords: TTimedWords; ATrack: Integer;
  var ATurns: TArray<TTurn>);
// Remonta os turnos a partir das palavras que SOBRARAM do dedup. Ver o
// bloco TURN_* nas constantes pra o porque de cortar aqui e nao usar o
// `turns` da API (que so quebra em troca de falante).
var
  i, n: Integer;
  Cur: TTurn;
  Have: Boolean;

  procedure Flush;
  begin
    if not Have then Exit;
    Cur.Text := Trim(Cur.Text);
    if Cur.Text <> '' then
    begin
      n := Length(ATurns);
      SetLength(ATurns, n + 1);
      ATurns[n] := Cur;
    end;
    Have := False;
  end;

begin
  Have := False;
  Cur := Default(TTurn);
  for i := 0 to High(AWords) do
  begin
    if AWords[i].Drop then Continue;
    if AWords[i].IsSegment then
    begin
      // Segmento sem alinhamento: turno proprio, com o tempo dele.
      Flush;
      Cur := Default(TTurn);
      Cur.Speaker := AWords[i].Speaker;
      Cur.StartS := AWords[i].StartS;
      Cur.EndS := AWords[i].EndS;
      Cur.Text := AWords[i].Text;
      Cur.Track := ATrack;
      Have := True;
      Flush;
      Continue;
    end;
    if Have and ((AWords[i].Speaker <> Cur.Speaker)
        or (AWords[i].StartS - Cur.EndS > TURN_GAP_SEC)
        or (AWords[i].EndS - Cur.StartS > TURN_HARD_MAX_SEC)
        or ((AWords[i].EndS - Cur.StartS > TURN_SOFT_MAX_SEC) and
            EndsSentence(Cur.Text))) then
      Flush;
    if not Have then
    begin
      Cur := Default(TTurn);
      Cur.Speaker := AWords[i].Speaker;
      Cur.StartS := AWords[i].StartS;
      Cur.Text := '';
      Cur.Track := ATrack;
      Have := True;
    end;
    AppendWord(Cur.Text, AWords[i].Text);
    Cur.EndS := AWords[i].EndS;
  end;
  Flush;
end;

procedure TurnsFromWords(AObj: TJSONObject; ATrack: Integer;
  var ATurns: TArray<TTurn>);
// Caminho de UMA faixa (mistura): palavras -> turnos, sem dedup.
var
  Words: TTimedWords;
begin
  SetLength(Words, 0);
  CollectWords(AObj, Words);
  TurnsFromWordList(Words, ATrack, ATurns);
end;

function DedupWords(var ATracks: TArray<TTimedWords>): Integer;
// DEDUPLICACAO POR PALAVRA entre faixas. Devolve quantas caíram.
//
// O que sai pelos alto-falantes volta pelo microfone, e dois microfones
// na mesma sala captam a mesma fala. Comparar TURNOS nao funcionava: cada
// faixa corta os turnos em pontos diferentes, entao a copia quase nunca
// se sobrepunha o bastante (31 falas duplicadas sobreviveram numa
// palestra real), e quando casava o turno inteiro caia — levando junto
// as palavras legitimas dele.
//
// Aqui, por par de faixas:
//   1. CASA a mesma palavra nas duas, dentro de WDEDUP_TOL, em ordem
//      (guloso e monotono: cada palavra de A pega a primeira igual em B
//      depois do ultimo par);
//   2. forma CORRIDAS de pares seguidos, tolerando ate WDEDUP_MAXSKIP
//      palavras puladas de cada lado — o eco chega picado;
//   3. corrida LONGA (>= 4 pares) e eco: cai o trecho da faixa que
//      capturou MENOS palavras (empate: a de menor confianca);
//   4. corrida CURTA (2-3 pares) so e eco se TODOS os pares forem
//      simultaneos (WDEDUP_SHORT_TOL) e se ao menos um lado nao tiver
//      falado mais nada em volta; cai a copia de MENOR confianca.
//
// Por que "menor confianca" na curta e nao "o lado sem sobra": a resposta
// do falante local logo depois poe sobra justamente no lado do ECO, e a
// regra apagava o original. Medido e corrigido na varredura.
//
// Varredura (624 palavras reais, 6 cenarios, 3 sementes): eco que sobra
// 13%, fala legitima perdida 0,3%. O dedup por turno dava 28% e 4,8%.
// Conversa real SEM eco (fone): zero palavras perdidas.
var
  a, b, pa, pb, i, j, k, low, last, RunStart, Dropped, NM: Integer;
  IA, IB, MI, MJ: TArray<Integer>;

  procedure Filter(const W: TTimedWords; var Idx: TArray<Integer>);
  var
    x, c: Integer;
  begin
    SetLength(Idx, Length(W));
    c := 0;
    for x := 0 to High(W) do
      if (W[x].Norm <> '') and not W[x].IsSegment then
      begin
        Idx[c] := x;
        Inc(c);
      end;
    SetLength(Idx, c);
  end;

  function Continues(p, q: Integer): Boolean;
  begin
    Result := (MI[q] - MI[p] <= WDEDUP_MAXSKIP + 1) and
              (MJ[q] - MJ[p] <= WDEDUP_MAXSKIP + 1) and
              (ATracks[a][IA[MI[q]]].StartS - ATracks[a][IA[MI[p]]].EndS <= WDEDUP_RUN_GAP);
  end;

  procedure HandleRun(r0, r1: Integer);
  var
    Len, iA0, iA1, jB0, jB1, CountA, CountB, x: Integer;
    ScoreA, ScoreB, T0, T1: Double;
    ExtraA, ExtraB, LoseB: Boolean;
  begin
    Len := r1 - r0 + 1;
    iA0 := MI[r0]; iA1 := MI[r1];
    jB0 := MJ[r0]; jB1 := MJ[r1];
    CountA := iA1 - iA0 + 1;
    CountB := jB1 - jB0 + 1;
    ScoreA := 0;
    for x := iA0 to iA1 do ScoreA := ScoreA + ATracks[a][IA[x]].Score;
    ScoreA := ScoreA / CountA;
    ScoreB := 0;
    for x := jB0 to jB1 do ScoreB := ScoreB + ATracks[b][IB[x]].Score;
    ScoreB := ScoreB / CountB;

    if Len >= WDEDUP_MIN_RUN then
      LoseB := (CountA > CountB) or ((CountA = CountB) and (ScoreA >= ScoreB))
    else
    begin
      if Len < WDEDUP_MIN_SHORT then Exit;
      for x := r0 to r1 do
        if Abs(ATracks[a][IA[MI[x]]].StartS - ATracks[b][IB[MJ[x]]].StartS) > WDEDUP_SHORT_TOL then
          Exit;
      T0 := Min(ATracks[a][IA[iA0]].StartS, ATracks[b][IB[jB0]].StartS) - WDEDUP_PAD;
      T1 := Max(ATracks[a][IA[iA1]].EndS, ATracks[b][IB[jB1]].EndS) + WDEDUP_PAD;
      // "Sobra" = palavra do lado que nao entrou na corrida: no meio dela,
      // ou colada antes/depois (as palavras vem em ordem de tempo).
      ExtraA := (CountA > Len) or
        ((iA0 > 0) and (ATracks[a][IA[iA0 - 1]].StartS >= T0)) or
        ((iA1 < High(IA)) and (ATracks[a][IA[iA1 + 1]].StartS <= T1));
      ExtraB := (CountB > Len) or
        ((jB0 > 0) and (ATracks[b][IB[jB0 - 1]].StartS >= T0)) or
        ((jB1 < High(IB)) and (ATracks[b][IB[jB1 + 1]].StartS <= T1));
      // Os dois lados falaram mais coisa ali: e conversa, nao eco.
      if ExtraA and ExtraB then Exit;
      LoseB := ScoreA >= ScoreB;
    end;

    if LoseB then
    begin
      for x := jB0 to jB1 do
        if not ATracks[b][IB[x]].Drop then
        begin
          ATracks[b][IB[x]].Drop := True;
          Inc(Dropped);
        end;
    end
    else
      for x := iA0 to iA1 do
        if not ATracks[a][IA[x]].Drop then
        begin
          ATracks[a][IA[x]].Drop := True;
          Inc(Dropped);
        end;
  end;

begin
  Dropped := 0;
  // a/b NAO sao as variaveis do for: as rotinas aninhadas leem as duas, e
  // variavel de controle de for tem que ser local simples.
  for pa := 0 to High(ATracks) do
    for pb := pa + 1 to High(ATracks) do
    begin
      a := pa;
      b := pb;
      Filter(ATracks[a], IA);
      Filter(ATracks[b], IB);
      if (Length(IA) = 0) or (Length(IB) = 0) then Continue;

      SetLength(MI, Length(IA));
      SetLength(MJ, Length(IA));
      NM := 0;
      low := 0;
      last := -1;
      for i := 0 to High(IA) do
      begin
        while (low <= High(IB)) and
              (ATracks[b][IB[low]].StartS < ATracks[a][IA[i]].StartS - WDEDUP_TOL) do
          Inc(low);
        j := Max(low, last + 1);
        while (j <= High(IB)) and
              (ATracks[b][IB[j]].StartS <= ATracks[a][IA[i]].StartS + WDEDUP_TOL) do
        begin
          if ATracks[b][IB[j]].Norm = ATracks[a][IA[i]].Norm then
          begin
            MI[NM] := i;
            MJ[NM] := j;
            Inc(NM);
            last := j;
            Break;
          end;
          Inc(j);
        end;
      end;

      RunStart := 0;
      if NM = 0 then Continue;
      for k := 1 to NM do
        if (k = NM) or not Continues(k - 1, k) then
        begin
          HandleRun(RunStart, k - 1);
          RunStart := k;
        end;
    end;
  Result := Dropped;
end;

function RechunkTurns(const ABody: string; out ANewBody: string): Boolean;
// Troca o `turns` da resposta pelos turnos remontados das PALAVRAS,
// preservando todo o resto (segments, words, language, by_speaker...).
//
// Vale pro caminho da MISTURA, que guarda o JSON da API como veio. O
// defeito e o mesmo do caminho por faixa: o `turns` da API so quebra em
// troca de falante, entao uma fala corrida de uma pessoa vira um bloco
// unico. Medido numa gravacao real: um turno de 10,04 s com SETE
// segundos de silencio no meio; remontado, virou dois de ~2 s.
var
  Root: TJSONValue;
  Obj: TJSONObject;
  Turns: TArray<TTurn>;
  Arr: TJSONArray;
  Item: TJSONObject;
  Old: TJSONPair;
  i: Integer;
begin
  Result := False;
  ANewBody := ABody;
  Root := TJSONObject.ParseJSONValue(ABody);
  if not (Root is TJSONObject) then
  begin
    if Root <> nil then Root.Free;
    Exit;
  end;
  try
    Obj := TJSONObject(Root);
    SetLength(Turns, 0);
    TurnsFromWords(Obj, 0, Turns);
    // Sem word timestamps nao ha o que remontar — deixa como veio.
    if Length(Turns) = 0 then Exit;

    Arr := TJSONArray.Create;
    for i := 0 to High(Turns) do
    begin
      Item := TJSONObject.Create;
      Item.AddPair('speaker', Turns[i].Speaker);
      Item.AddPair('start', TJSONNumber.Create(Turns[i].StartS));
      Item.AddPair('end', TJSONNumber.Create(Turns[i].EndS));
      Item.AddPair('text', Turns[i].Text);
      Arr.AddElement(Item);
    end;
    Old := Obj.RemovePair('turns');
    if Old <> nil then Old.Free;
    Obj.AddPair('turns', Arr);
    ANewBody := Obj.ToJSON;
    Result := True;
  finally
    Root.Free;
  end;
end;

procedure FallbackTurns(AObj: TJSONObject; ATrack: Integer;
  var ATurns: TArray<TTurn>);
// Resposta SEM palavra nenhuma (API sem alinhamento): usa o `turns` como
// veio. Fica grosso, mas e melhor que nao ter turno nenhum.
var
  Arr: TJSONValue;
  A: TJSONArray;
  T: TJSONObject;
  i, n: Integer;
  Spk, Txt: string;
  St, En: Double;
begin
  Arr := AObj.GetValue('turns');
  if not (Arr is TJSONArray) then Exit;
  A := TJSONArray(Arr);
  for i := 0 to A.Count - 1 do
  begin
    if not (A.Items[i] is TJSONObject) then Continue;
    T := TJSONObject(A.Items[i]);
    Spk := ''; Txt := ''; St := 0; En := 0;
    T.TryGetValue<string>('speaker', Spk);
    T.TryGetValue<string>('text', Txt);
    T.TryGetValue<Double>('start', St);
    T.TryGetValue<Double>('end', En);
    if Trim(Txt) = '' then Continue;
    n := Length(ATurns);
    SetLength(ATurns, n + 1);
    ATurns[n] := Default(TTurn);
    ATurns[n].Speaker := Trim(Spk);
    ATurns[n].StartS := St;
    ATurns[n].EndS := En;
    ATurns[n].Text := Trim(Txt);
    ATurns[n].Track := ATrack;
  end;
end;

procedure LabelTrackSpeakers(var ATurns: TArray<TTurn>; const ATrackName: string);
// A regra do rotulo: se a faixa tem um falante so (o caso do microfone),
// o nome do DISPOSITIVO ja diz tudo e o SPEAKER_00 seria ruido. Se tem
// mais de um (o caso do alto-falante numa reuniao), o nome da faixa vira
// prefixo e a diarizacao continua distinguindo quem e quem dentro dela.
var
  Distinct: TStringList;
  i: Integer;
begin
  Distinct := TStringList.Create;
  try
    Distinct.Sorted := True;
    Distinct.Duplicates := dupIgnore;
    for i := 0 to High(ATurns) do
      if ATurns[i].Speaker <> '' then Distinct.Add(ATurns[i].Speaker);
    for i := 0 to High(ATurns) do
      if (Distinct.Count > 1) and (ATurns[i].Speaker <> '') then
        ATurns[i].Speaker := ATrackName + ' · ' + ATurns[i].Speaker
      else
        ATurns[i].Speaker := ATrackName;
  finally
    Distinct.Free;
  end;
end;

function MergeTranscripts(const ABodies, ANames: TArray<string>;
  out AMerged, AText: string): Boolean;
// Junta as faixas numa transcricao so, no MESMO formato que a API
// devolve pra uma faixa unica ({turns:[{speaker,start,end,text}], text}).
// Manter o formato e o que faz o painel do player e a busca continuarem
// funcionando sem saber que isto existe.
//
// A ordem importa: PALAVRAS de todas as faixas -> dedup POR PALAVRA ->
// turnos so com as palavras que sobraram. Montar os turnos antes e
// deduplicar depois era o que falhava (ver DedupWords).
//
// O dedup por TURNO sobrou como rede de seguranca so pra faixa sem
// timestamp por palavra (idioma sem alinhador): la nao ha palavra pra
// casar. Entre duas faixas COM palavras ele nao roda — ja foi resolvido
// no nivel certo, e por turno ele apagaria fala legitima.
var
  Roots: TArray<TJSONValue>;
  Words: TArray<TTimedWords>;
  HasWords: TArray<Boolean>;
  Turns, Tmp: TArray<TTurn>;
  i, j, x, Kept, WordDrops, TurnDrops: Integer;
  Sim, Ov: Double;
  Norm: TArray<string>;
  Obj: TJSONObject;
  Arr, TracksArr: TJSONArray;
  Item: TJSONObject;
  SB: TStringBuilder;
begin
  Result := False;
  AMerged := '';
  AText := '';
  SetLength(Turns, 0);
  SetLength(Roots, Length(ABodies));
  SetLength(Words, Length(ABodies));
  SetLength(HasWords, Length(ABodies));
  for i := 0 to High(Roots) do Roots[i] := nil;
  try
    for i := 0 to High(ABodies) do
    begin
      if Trim(ABodies[i]) = '' then Exit;
      Roots[i] := TJSONObject.ParseJSONValue(ABodies[i]);
      if not (Roots[i] is TJSONObject) then Exit;
      SetLength(Words[i], 0);
      CollectWords(TJSONObject(Roots[i]), Words[i]);
      HasWords[i] := False;
      for x := 0 to High(Words[i]) do
        if (Words[i][x].Norm <> '') and not Words[i][x].IsSegment then
        begin
          HasWords[i] := True;
          Break;
        end;
    end;

    WordDrops := DedupWords(Words);

    for i := 0 to High(ABodies) do
    begin
      SetLength(Tmp, 0);
      TurnsFromWordList(Words[i], i, Tmp);
      if Length(Tmp) = 0 then FallbackTurns(TJSONObject(Roots[i]), i, Tmp);
      LabelTrackSpeakers(Tmp, ANames[i]);
      Turns := Turns + Tmp;
    end;
  finally
    for i := 0 to High(Roots) do
      if Roots[i] <> nil then Roots[i].Free;
  end;

  // Ordena por inicio: o player acompanha o destaque nessa ordem, e o dedup
  // por turno abaixo so olha a janela vizinha.
  TArray.Sort<TTurn>(Turns, TComparer<TTurn>.Construct(
    function(const A, B: TTurn): Integer
    begin
      Result := CompareValue(A.StartS, B.StartS);
      if Result = 0 then Result := CompareValue(A.Track, B.Track);
    end));

  SetLength(Norm, Length(Turns));
  for i := 0 to High(Turns) do Norm[i] := NormalizeForCompare(Turns[i].Text);

  TurnDrops := 0;
  for i := 0 to High(Turns) do
  begin
    if Turns[i].Drop then Continue;
    j := i + 1;
    while (j <= High(Turns)) and (Turns[j].StartS < Turns[i].EndS) do
    begin
      // So a rede de seguranca: entre duas faixas com palavras o dedup ja
      // foi feito por palavra.
      if Turns[j].Drop or (Turns[j].Track = Turns[i].Track) or
         (HasWords[Turns[i].Track] and HasWords[Turns[j].Track]) then
      begin
        Inc(j);
        Continue;
      end;
      Ov := TimeOverlapRatio(Turns[i], Turns[j]);
      if Ov >= DEDUP_OVERLAP_MIN then
      begin
        Sim := WordSimilarity(Norm[i], Norm[j]);
        if Sim >= DEDUP_SIM_MIN then
        begin
          if Length(Norm[j]) > Length(Norm[i]) then Turns[i].Drop := True
          else Turns[j].Drop := True;
          Inc(TurnDrops);
          if Turns[i].Drop then Break;
        end;
      end;
      Inc(j);
    end;
  end;

  Obj := TJSONObject.Create;
  SB := TStringBuilder.Create;
  try
    Arr := TJSONArray.Create;
    Kept := 0;
    for i := 0 to High(Turns) do
    begin
      if Turns[i].Drop then Continue;
      Item := TJSONObject.Create;
      Item.AddPair('speaker', Turns[i].Speaker);
      Item.AddPair('start', TJSONNumber.Create(Turns[i].StartS));
      Item.AddPair('end', TJSONNumber.Create(Turns[i].EndS));
      Item.AddPair('text', Turns[i].Text);
      Arr.AddElement(Item);
      if SB.Length > 0 then SB.Append(' ');
      SB.Append(Turns[i].Text);
      Inc(Kept);
    end;
    AText := SB.ToString;
    Obj.AddPair('turns', Arr);
    Obj.AddPair('text', AText);
    // Proveniencia: quais faixas entraram. Nao e lido por ninguem hoje,
    // mas e a unica pista de que este arquivo veio de varias faixas.
    TracksArr := TJSONArray.Create;
    for i := 0 to High(ANames) do TracksArr.Add(ANames[i]);
    Obj.AddPair('tracks', TracksArr);
    AMerged := Obj.ToJSON;
  finally
    SB.Free;
    Obj.Free;
  end;
  Log('Transcribe: %d faixas -> %d turnos (eco: %d palavra(s) por palavra, %d turno(s) por turno).',
    [Length(ABodies), Kept, WordDrops, TurnDrops]);
  Result := True;
end;

procedure ServerBackUp;
// O /health acabou de responder. Se a fila estava esperando o servidor,
// tira a frase de espera da tela AGORA — antes ela so saia quando o item
// terminava de transcrever, minutos depois, e parecia que o erro tinha
// ficado preso. So a frase de espera (nome vazio), nunca a falha real de
// uma gravacao.
var
  Changed: Boolean;
begin
  Changed := False;
  GLock.Enter;
  try
    if GWaitingServer then
    begin
      GWaitingServer := False;
      if GLastErrorName = '' then GLastError := '';
      Changed := True;
    end;
  finally
    GLock.Leave;
  end;
  if Changed then
  begin
    Log('Transcribe: servidor voltou (%s) — fila retomada.', [HostBase]);
    NotifyChanged;
  end;
end;

function ProcessOne(const APath: string): string;
// Transcreve UMA gravacao. Devolve '' em sucesso, senao a mensagem.
// Roda inteiro na worker thread.
//
// Duas formas, decididas pelo PlanTracks:
//   1 faixa  — a MISTURA (stream 0), como sempre foi.
//   N faixas — as ISOLADAS por dispositivo, uma transcricao cada, o
//              resultado juntado e desduplicado pelo MergeTranscripts.
// A segunda custa N vezes mais tempo de servidor e entrega atribuicao de
// falante de verdade, com o nome do dispositivo em vez de SPEAKER_00.
var
  Body, Txt, Merged: string;
  Files, Send, Names, Bodies, UsedNames: TArray<string>;
  Streams: TArray<Integer>;
  CacheBase: string;
  HealthErr: string;
  PerTrack: Boolean;
  Est, i, n: Integer;

  procedure CleanupFiles;
  var
    k: Integer;
  begin
    // Os m4a sao so veiculo do upload — regeneram em segundos, e uma
    // hora de audio ocupa ~70 MB que nao vale guardar.
    for k := 0 to High(Files) do
      try if TFile.Exists(Files[k]) then TFile.Delete(Files[k]); except end;
  end;

begin
  if not TFile.Exists(APath) then Exit(OBSLang.T('error.transcribe.fileMissing'));

  // PING ANTES DE QUALQUER TRABALHO. Duas razoes, e a segunda e a que
  // pesa: (1) servidor fora do ar so aparecia depois de extrair o audio
  // — dezenas de MB, segundos a minutos jogados fora por item do lote;
  // (2) o erro que chegava era o do WinINet no meio do upload ("Error
  // sending data: (12152)..."), que nao tem como ser lido como
  // "o container nao esta rodando". O /health e barato e local.
  // Motor LOCAL: nao ha servidor pra pingar; o que falta, se falta, e a
  // instalacao. O servidor local sobe sozinho dentro do RunLocal.
  if UseLocalEngine then
  begin
    if not OBSLocalAsr.IsInstalled then Exit(LOCAL_MISSING_MARK);
    HealthErr := '';
  end
  else
  begin
    SetStage('checking', 0, -1);
    HealthErr := CheckHealth(HostBase);
  end;
  if HealthErr <> '' then
  begin
    // Uma linha por queda, nao uma a cada tentativa.
    if not WaitingForServer then
      Log('Transcribe: /health nao respondeu (%s): %s', [HostBase, HealthErr]);
    // Sentinela, nao mensagem: o Execute poe o item de volta na frente da
    // espera e tenta de novo depois. A frase pra tela sai de la.
    Exit(SERVER_DOWN_MARK);
  end;
  if not UseLocalEngine then ServerBackUp;

  // GRAVACAO SEM AUDIO nenhum (so monitores, nenhum microfone marcado):
  // nao ha o que transcrever. Sem este atalho o ExtractAudioTracks
  // falhava e, com a transcricao automatica ao fim da gravacao, quem
  // grava sem audio levaria um aviso de erro depois de CADA gravacao.
  // Sai como "sem fala", que e o que a gravacao e.
  var Rep: TProbeReport;
  if Probe(APath, Rep) and (Length(Rep.AudioStreams) = 0) then
  begin
    Log('Transcribe: "%s" nao tem faixa de audio — gravada como sem fala.', [APath]);
    try
      TFile.WriteAllText(TranscriptPath(APath), '{"turns":[],"text":""}', TEncoding.UTF8);
      TFile.WriteAllText(TranscriptTextPath(APath), '', TEncoding.UTF8);
    except
      on E: Exception do Exit(OBSLang.T('error.transcribe.writeFailed', ['error', E.Message]));
    end;
    Exit('');
  end;

  CacheBase := IncludeTrailingPathDelimiter(OBSPlayer.CacheRootDir) +
    OBSPlayer.HashName(APath);

  PerTrack := PlanTracks(APath, Names, Streams);
  if not PerTrack then
  begin
    // Caminho antigo: SO O AUDIO da mistura. O video passaria do
    // MAX_UPLOAD_MB da API (512 MB) numa gravacao 4K de poucos minutos.
    SetLength(Names, 1);
    SetLength(Streams, 1);
    Names[0] := '';
    Streams[0] := 0;
  end;

  SetLength(Files, Length(Streams));
  for i := 0 to High(Files) do
    Files[i] := Format('%s_tr%d.m4a', [CacheBase, Streams[i]]);

  try
    CleanupFiles;
    SetStage('extracting', 0, -1);
    // UMA passada de demux pra todas as faixas. Streams[0] e o indice do
    // primeiro audio pedido; como PlanTracks devolve sempre um intervalo
    // contiguo comecando em Streams[0], o start index cobre o resto.
    if not FFmpegOps.ExtractAudioTracks(APath, Files, Streams[0]) then
      Exit(OBSLang.T('error.transcribe.extractFailed'));

    // Faixa muda nao vai pro servidor. Com varias faixas isto e o que
    // impede o microfone que ninguem usou de gastar uma fila inteira.
    SetLength(Bodies, 0);
    SetLength(UsedNames, 0);
    SetLength(Send, 0);
    n := 0;
    for i := 0 to High(Files) do
    begin
      if not TFile.Exists(Files[i]) then
        Exit(OBSLang.T('error.transcribe.extractMissing'));
      if PerTrack and not TrackHasSound(Files[i]) then
      begin
        Log('Transcribe: faixa "%s" sem som — nao vai pro servidor.', [Names[i]]);
        Continue;
      end;
      SetLength(UsedNames, n + 1);
      SetLength(Send, n + 1);
      UsedNames[n] := Names[i];
      // Lista SEPARADA: compactar o Files em si mesmo perderia os paths
      // das faixas puladas, e o CleanupFiles deixaria .m4a orfaos no cache.
      Send[n] := Files[i];
      Inc(n);
    end;

    // Todas mudas: e uma gravacao sem fala, nao uma falha. Escreve a
    // transcricao vazia — o app distingue "sem fala" de "nao transcrita"
    // (transcribed=True, has=False) e o player diz a coisa certa.
    if n = 0 then
    begin
      Log('Transcribe: nenhuma faixa com som em "%s".', [APath]);
      Merged := '{"turns":[],"text":""}';
      Txt := '';
    end
    else
    begin
      Est := 0;
      SetLength(Bodies, n);
      for i := 0 to n - 1 do
      begin
        Result := RunJob(Send[i], i, n, Est, Body);
        if Result <> '' then
        begin
          // O container pode cair NO MEIO do job (Docker reiniciado,
          // computador suspenso). Isso chega como erro de rede, mas e o
          // mesmo caso do servidor fora antes de comecar: se o /health
          // agora nao responde, o item espera em vez de ser descartado.
          // Voltando, a API reaproveita o job se o audio reenviado for
          // identico (o id e o hash dele); se nao for, refaz do zero.
          if (Result <> CANCELED_MARK) and (not ShuttingDown) and
             (not UseLocalEngine) and (CheckHealth(HostBase) <> '') then
            Result := SERVER_DOWN_MARK;
          Exit;
        end;
        Bodies[i] := Body;
      end;

      // So a MISTURA passa direto: o JSON da API ja esta no formato
      // final, e reescreve-lo jogaria fora os segmentos com timestamp por
      // palavra que o merge nao carrega.
      //
      // Por faixa, mesmo com UMA sobrando (as outras mudas), o merge
      // roda: e ele quem troca o SPEAKER_00 pelo nome do dispositivo, e
      // sair com rotulo generico justo na faixa que tem a fala anularia
      // o motivo de mandar por faixa.
      if not PerTrack then
      begin
        Merged := Bodies[0];
        // Mesmo aqui os turnos sao remontados: o `turns` da API quebra so
        // em troca de falante, e o player destaca o bloco inteiro.
        // O resto da resposta (segments, words) e preservado.
        var Rechunked: string;
        if RechunkTurns(Merged, Rechunked) then Merged := Rechunked;
        var Json := TJSONObject.ParseJSONValue(Merged);
        if not (Json is TJSONObject) then
        begin
          if Json <> nil then Json.Free;
          Exit(OBSLang.T('error.transcribe.resultNotJson'));
        end;
        try
          Txt := ExtractPlainText(TJSONObject(Json));
        finally
          Json.Free;
        end;
      end
      else if not MergeTranscripts(Bodies, UsedNames, Merged, Txt) then
        Exit(OBSLang.T('error.transcribe.resultNotJson'));
    end;

    // Dois arquivos: o JSON pro player, o texto puro pra busca.
    // Ver o cabecalho da unit.
    try
      TFile.WriteAllText(TranscriptPath(APath), Merged, TEncoding.UTF8);
      TFile.WriteAllText(TranscriptTextPath(APath), Txt, TEncoding.UTF8);
    except
      on E: Exception do Exit(OBSLang.T('error.transcribe.writeFailed', ['error', E.Message]));
    end;
    Result := '';
  finally
    CleanupFiles;
    SetTrack(0, 1);
  end;
end;

procedure SetPaused(APaused: Boolean);
begin
  if GPaused = APaused then Exit;
  GPaused := APaused;
  if APaused then
    Log('Transcribe: automaticas PAUSADAS (captura em andamento; pedidas a mao seguem).')
  else Log('Transcribe: fila liberada.');
  // A tela precisa saber: pausado com itens na fila vira a etapa "paused".
  NotifyChanged;
end;

function IsPaused: Boolean;
begin
  Result := GPaused;
end;

procedure TTranscribeThread.Execute;

  // Proximo item a rodar, ou -1. Pausado, pula os automaticos e pega o
  // primeiro pedido a mao, na ordem da fila. Caller segura o GLock.
  function NextIndex: Integer;
  var
    k: Integer;
  begin
    Result := -1;
    if (GQueue = nil) or (GQueue.Count = 0) then Exit;
    if not GPaused then Exit(0);
    if GManual = nil then Exit;
    for k := 0 to GQueue.Count - 1 do
      if GManual.ContainsKey(LowerCase(GQueue[k])) then Exit(k);
  end;

var
  Path, Err: string;
  Has: Boolean;
  Idx: Integer;
  Handles: array[0..1] of THandle;
begin
  while not Terminated do
  begin
    Path := '';
    if GLock <> nil then
    begin
      GLock.Enter;
      try
        // Pausado: NAO pega item AUTOMATICO novo, mas a fila fica como
        // esta; pedido a mao passa na frente. So com automaticos parados a
        // etapa vira 'paused', pra a aba de Transcricao explicar a espera —
        // fila parada sem motivo aparente se le como defeito.
        Idx := NextIndex;
        Has := Idx >= 0;
        if (not Has) and GPaused and (GQueue <> nil) and (GQueue.Count > 0) then
          GStage := 'paused';
        if Has then
        begin
          Path := GQueue[Idx];
          GQueue.Delete(Idx);
          Inc(GQueueRev);
          GRunning := True;
          GCurrentFinished := False;
          GCurrentPath := Path;
          GCurrentName := ChangeFileExt(ExtractFileName(Path), '');
          GCurrentStartTick := GetTickCount;
          GStage := 'extracting';
          GProgress := -1;
          GEta := -1;
          GTrack := 0;
          GTrackCount := 1;
          // O cancelamento vale pro item que estava em curso quando o
          // usuario clicou; um item NOVO comeca limpo.
          GCancelCurrent := False;
        end
        else
        begin
          if GRunning then Inc(GQueueRev);   // a linha "em curso" sai da lista
          GRunning := False;
          GCurrentPath := '';
          GCurrentName := '';
          GCurrentStartTick := 0;
          GStage := '';
          GProgress := -1;
          GEta := -1;
          GCancelCurrent := False;
          // Fila vazia: nao ha mais o que esperar — nem o aviso de espera
          // (o item que esperava foi removido da fila).
          if GWaitingServer and (GLastErrorName = '') then GLastError := '';
          GWaitingServer := False;
          // Os contadores do lote NAO sao zerados aqui: a UI precisa
          // continuar mostrando "7 de 7 concluidas" depois que a fila
          // esvazia. Quem zera e o proximo lote (ver ResetBatchIfIdle).
        end;
      finally
        GLock.Leave;
      end;
    end;

    if Path = '' then
    begin
      NotifyChanged;
      // Fila vazia: o servidor local, se de pe, cai depois de ocioso por
      // alguns minutos e devolve a memoria de video (ver OBSLocalAsr).
      OBSLocalAsr.StopIfIdle;
      if WaitForSingleObject(StopEvent, 400) = WAIT_OBJECT_0 then Break;
      Continue;
    end;

    // O item saiu da espera e virou "em curso": continua no arquivo, agora
    // na primeira posicao.
    SaveQueue;
    NotifyChanged;
    Log('Transcribe: iniciando "%s"', [Path]);
    Err := '';
    try
      Err := ProcessOne(Path);
    except
      on E: Exception do Err := E.Message;
    end;

    // SERVIDOR FORA DO AR: nao e defeito do arquivo. Devolve o item pra
    // FRENTE da espera, sem contar como falha nem soltar aviso de erro por
    // item, e tenta de novo daqui a SERVER_RETRY_MS — ou antes, se alguem
    // chamar NudgeQueue (um teste de servidor que deu certo).
    if ((Err = SERVER_DOWN_MARK) or (Err = LOCAL_MISSING_MARK)) and not Terminated then
    begin
      GLock.Enter;
      try
        GQueue.Insert(0, Path);
        Inc(GQueueRev);
        GRunning := False;
        GCurrentFinished := True;
        GCurrentPath := '';
        GCurrentName := '';
        GCurrentStartTick := 0;
        if Err = LOCAL_MISSING_MARK then GStage := 'waitingInstall'
        else GStage := 'waiting';
        GProgress := -1;
        GEta := -1;
        if not GWaitingServer then
        begin
          GWaitingServer := True;
          // Nome vazio de proposito: nao e a gravacao que falhou, e o
          // servidor. E e por ele que a limpeza abaixo reconhece a frase.
          if Err = LOCAL_MISSING_MARK then
          begin
            GLastError := OBSLang.T('error.localAsr.notInstalled');
            Log('Transcribe: motor local nao instalado — fila em espera.');
          end
          else
          begin
            GLastError := OBSLang.T('error.transcribe.serverDown', ['host', HostBase]);
            Log('Transcribe: servidor fora do ar — fila em espera.');
          end;
          GLastErrorName := '';
        end;
      finally
        GLock.Leave;
      end;
      SaveQueue;
      NotifyChanged;
      Handles[0] := StopEvent;
      Handles[1] := WakeEvent;
      if WaitForMultipleObjects(2, @Handles[0], False, SERVER_RETRY_MS) = WAIT_OBJECT_0 then
        Break;
      Continue;
    end;

    GLock.Enter;
    try
      GCurrentFinished := True;
      // Terminou de verdade (sucesso, falha ou cancelamento): a marca de
      // manual morre aqui, nao quando a worker pegou o item.
      if GManual <> nil then GManual.Remove(LowerCase(Path));
      // Passou do /health: o servidor voltou. Tira a frase de espera da
      // tela — so ela (nome vazio), nunca a falha real de uma gravacao.
      if GWaitingServer then
      begin
        GWaitingServer := False;
        if GLastErrorName = '' then GLastError := '';
      end;
      if Err = CANCELED_MARK then
        // Cancelado pelo usuario: nao conta como concluido nem como
        // falha, e nao vira mensagem de erro na tela.
        Log('Transcribe: cancelado "%s"', [Path])
      else if Err = '' then Inc(GDone)
      else
      begin
        Inc(GFailed);
        GLastError := Err;
        GLastErrorName := ChangeFileExt(ExtractFileName(Path), '');
      end;
    finally
      GLock.Leave;
    end;
    // Terminou de verdade: sai do arquivo. (No fechamento o SaveQueue nao
    // grava, entao o item interrompido fica la pra voltar na proxima vez.)
    SaveQueue;
    if Err = '' then Log('Transcribe: concluida "%s"', [Path])
    else Log('Transcribe: FALHOU "%s": %s', [Path, Err]);
    NotifyChanged;
  end;
end;

procedure EnsureStarted;
// Cria fila/lock/thread na primeira necessidade. Chamado sempre da main.
begin
  if GLock = nil then GLock := TCriticalSection.Create;
  if GQueue = nil then GQueue := TList<string>.Create;
  if GManual = nil then GManual := TDictionary<string, Boolean>.Create;
  if StopEvent = 0 then StopEvent := CreateEvent(nil, True, False, nil);
  if WakeEvent = 0 then WakeEvent := CreateEvent(nil, False, False, nil);
  if SaveLock = nil then SaveLock := TCriticalSection.Create;
  if Worker = nil then
  begin
    // Nao-suspensa e sem Start explicito — pegadinha #45: criar suspensa
    // e chamar Start no proprio construtor dispara EThread por
    // double-resume.
    Worker := TTranscribeThread.Create(False);
    Worker.FreeOnTerminate := False;
  end;
end;

function AlreadyQueued(const APath: string): Boolean;
// Compara por PATH, nunca pelo nome de exibicao: duas pastas podem ter
// gravacoes homonimas, e enfileirar a mesma duas vezes gastaria minutos
// do servidor pra reescrever o mesmo arquivo.
var
  i: Integer;
begin
  Result := False;
  if GQueue = nil then Exit;
  for i := 0 to GQueue.Count - 1 do
    if SameText(GQueue[i], APath) then Exit(True);
  Result := GRunning and SameText(GCurrentPath, APath);
end;

procedure ResetBatchIfIdle;
// Zera os contadores do lote quando um NOVO lote comeca. Nao da pra
// zerar ao esvaziar a fila: a UI tem que continuar mostrando o
// resultado ("7 de 7") depois que tudo termina. Caller segura o GLock.
begin
  if GRunning then Exit;
  if (GQueue <> nil) and (GQueue.Count > 0) then Exit;
  GBatchTotal := 0;
  GDone := 0;
  GFailed := 0;
  GLastError := '';
  GLastErrorName := '';
end;

procedure Enqueue(const APath: string; AManual: Boolean);
var
  Promoted: Boolean;
begin
  if APath = '' then Exit;
  EnsureStarted;
  Promoted := False;
  GLock.Enter;
  try
    if AlreadyQueued(APath) then
    begin
      // Ja estava esperando como automatico e o usuario pediu a mao: vira
      // manual, e com a captura em andamento passa a rodar.
      if AManual and not GManual.ContainsKey(LowerCase(APath)) then
      begin
        GManual.AddOrSetValue(LowerCase(APath), True);
        Inc(GQueueRev);
        Promoted := True;
      end;
    end
    else
    begin
      ResetBatchIfIdle;
      GQueue.Add(APath);
      if AManual then GManual.AddOrSetValue(LowerCase(APath), True);
      Inc(GBatchTotal);
      Inc(GQueueRev);
      Promoted := True;
    end;
  finally
    GLock.Leave;
  end;
  if not Promoted then Exit;
  Log('Transcribe: enfileirado "%s"%s (fila=%d)',
    [APath, IfThen(AManual, ' a pedido do usuario', ''), QueueLength]);
  SaveQueue;
  NotifyChanged;
end;

procedure EnqueueMany(const APaths: TArray<string>; AManual: Boolean;
  const AManualPaths: TArray<string>);
var
  i, j, Added: Integer;
  IsMan: Boolean;
begin
  if Length(APaths) = 0 then Exit;
  EnsureStarted;
  Added := 0;
  GLock.Enter;
  try
    ResetBatchIfIdle;
    for i := 0 to High(APaths) do
    begin
      if APaths[i] = '' then Continue;
      IsMan := AManual;
      if not IsMan then
        for j := 0 to High(AManualPaths) do
          if SameText(AManualPaths[j], APaths[i]) then
          begin
            IsMan := True;
            Break;
          end;
      if AlreadyQueued(APaths[i]) then
      begin
        if IsMan and not GManual.ContainsKey(LowerCase(APaths[i])) then
        begin
          GManual.AddOrSetValue(LowerCase(APaths[i]), True);
          Inc(GQueueRev);
          Inc(Added);
        end;
        Continue;
      end;
      GQueue.Add(APaths[i]);
      if IsMan then GManual.AddOrSetValue(LowerCase(APaths[i]), True);
      Inc(GBatchTotal);
      Inc(GQueueRev);
      Inc(Added);
    end;
  finally
    GLock.Leave;
  end;
  Log('Transcribe: %d item(ns) enfileirado(s)%s.',
    [Added, IfThen(AManual, ' a pedido do usuario', '')]);
  if Added > 0 then SaveQueue;
  NotifyChanged;
end;

procedure CancelAll;
begin
  if GLock = nil then Exit;
  GLock.Enter;
  try
    if GQueue <> nil then GQueue.Clear;
    if GManual <> nil then GManual.Clear;
    Inc(GQueueRev);
    // Agora o item EM CURSO tambem para: a API ganhou DELETE /jobs/{id}.
    // A worker ve esta flag na proxima consulta (no maximo 1s), manda o
    // DELETE e desiste. Antes, sem rota de cancelamento, ele tinha que
    // ir ate o fim.
    GCancelCurrent := True;
    GBatchTotal := 0;
    GDone := 0;
    GFailed := 0;
    GLastError := '';
    GLastErrorName := '';
    GWaitingServer := False;
  finally
    GLock.Leave;
  end;
  Log('Transcribe: fila cancelada.');
  SaveQueue;
  // Se a worker esperava o servidor, acorda: com a fila vazia ela sai do
  // modo de espera na hora, em vez de a tela seguir "aguardando" por 30 s.
  NudgeQueue;
  NotifyChanged;
end;

procedure Shutdown;
begin
  // ANTES de parar a worker: a volta final do laco dela trataria o item
  // interrompido como terminado e o tiraria do arquivo da fila.
  GShuttingDown := True;
  OnChanged := nil;
  // Antes de esperar a worker: matar o servidor local derruba o POST em
  // voo na hora, e a worker sai sem precisar ser abandonada.
  OBSLocalAsr.Shutdown;
  if Worker <> nil then
  begin
    Worker.Terminate;
    if StopEvent <> 0 then SetEvent(StopEvent);
    // Espera curta: se o POST em curso ainda esta em voo, nao da pra
    // aborta-lo — deixa o OS limpar no fim do processo em vez de segurar
    // o fechamento do app.
    if WaitForSingleObject(Worker.Handle, 2000) = WAIT_TIMEOUT then
    begin
      // POST em voo nao da pra abortar (a API nao tem cancelamento).
      // Abandona a thread e VAZA fila/lock de proposito: libera-los aqui
      // seria uso-apos-liberacao no proximo tique dela. O OS limpa tudo
      // no fim do processo — mesma escolha do OBSRecordWatch.
      Log('Transcribe: worker nao parou em 2s — abandonando (cleanup pelo OS).');
      Worker := nil;
      Exit;
    end;
    FreeAndNil(Worker);
  end;
  if StopEvent <> 0 then
  begin
    CloseHandle(StopEvent);
    StopEvent := 0;
  end;
  if WakeEvent <> 0 then
  begin
    CloseHandle(WakeEvent);
    WakeEvent := 0;
  end;
  if GQueue <> nil then FreeAndNil(GQueue);
  if GManual <> nil then FreeAndNil(GManual);
  if GLock <> nil then FreeAndNil(GLock);
  if SaveLock <> nil then FreeAndNil(SaveLock);
end;

end.
