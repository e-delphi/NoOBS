(*
  OBSSingleInstance - identificadores compartilhados entre o modo full
  (OBSUI) e o modo hibernate (OBSHibernate) pra protecao de instancia
  unica funcionar ENTRE os dois modos.

  Sem essa unit, cada modo tinha a sua copia local desses literais e
  ficaram fora de sincronia depois de um rename — o mutex e a window
  message tinham nomes diferentes, entao um processo full e um hibernate
  podiam rodar simultaneamente sem detectar um ao outro.

  Mantemos uma unit propria (em vez de juntar com NoOBSTypes ou OBSConfig)
  pra reforcar o ponto: TODA mudanca aqui afeta os DOIS modos, e o
  hibernate tem que ser minimo em dependencias (e essa unit nao traz
  nenhuma alem de SysUtils).

  Pegadinha relacionada: RegisterWindowMessage com a mesma string em
  processos diferentes retorna o MESMO UINT (escopo de sistema). E como
  o "canal" entre os dois processos pra promover hibernate -> full
  funciona sem precisar de DDE ou pipe.
*)
unit OBSSingleInstance;

interface

const
  // Mutex nomeado verificado por CreateMutex+GetLastError no inicio dos
  // dois modos. Se ja existe, a 2a instancia sai (com ressalvas: o full
  // pode promover um hibernate via WM_SHOW_INSTANCE).
  MUTEX_NAME    = 'NoOBS.SingleInstance.TNoOBS';

  // String registrada via RegisterWindowMessage. O resultado e o mesmo
  // UINT nos dois processos — usamos pra um modo acordar o outro.
  SHOW_MSG_NAME = 'NoOBS.ShowInstance.TNoOBS';

implementation

end.
