// Estado do Início compartilhado entre a página e o resumo da barra do topo:
// memória medida (uma única consulta a cada 10 s) e contas recarregando.
import { api, type MemoryReport, type Snapshot } from "./api";

/** Acima disso, um serviço em segundo plano vale ser recarregado. */
export const HEAVY_BYTES = 300 * 1024 ** 2;

const CONFIRM_FREE_KEY = "papeo.confirmFreeMemory";

function readConfirmFree(): boolean {
  try {
    return localStorage.getItem(CONFIRM_FREE_KEY) !== "off";
  } catch {
    return true;
  }
}

export const home = $state({
  /** Perguntar antes de liberar memória pelo resumo do topo (preferência local). */
  confirmFree: readConfirmFree(),
  memory: null as MemoryReport | null,
  /** Contas recarregando para liberar memória (o site leva alguns segundos). */
  reloading: {} as Record<string, boolean>,
  /** Modal "Serviços" (tabela das contas), aberto pela barra do topo no Início. */
  servicesOpen: false,
});

let watchers = 0;
let timer: ReturnType<typeof setInterval> | undefined;

function refresh() {
  // Janela na bandeja ou minimizada: ninguém vê a medição.
  if (document.hidden) return;
  api
    .getMemory()
    .then((m) => {
      home.memory = m;
    })
    .catch(() => {});
}

/** Mede a memória enquanto alguém mostra; devolve a função para parar. */
export function watchMemory(): () => void {
  if (watchers++ === 0) {
    refresh();
    timer = setInterval(refresh, 10_000);
    document.addEventListener("visibilitychange", refresh);
  }
  return () => {
    if (--watchers === 0) {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", refresh);
    }
  };
}

export function setConfirmFree(value: boolean) {
  home.confirmFree = value;
  try {
    if (value) localStorage.removeItem(CONFIRM_FREE_KEY);
    else localStorage.setItem(CONFIRM_FREE_KEY, "off");
  } catch {
    /* preferência local apenas */
  }
}

/** Recarregar zera a memória acumulada pelo site; as notificações continuam. */
export function freeMemory(ids: string[], run: (action: Promise<Snapshot | void>) => Promise<void>) {
  for (const id of ids) {
    home.reloading[id] = true;
    run(api.reload(id));
    setTimeout(() => {
      home.reloading[id] = false;
    }, 15_000);
  }
}

/** Contas em segundo plano pesadas o bastante para recarregar. */
export function heavyIds(snap: Snapshot): string[] {
  return snap.services
    .filter(
      (s) =>
        s.loaded &&
        s.id !== snap.active &&
        (home.memory?.services[s.id] ?? 0) >= HEAVY_BYTES &&
        !home.reloading[s.id],
    )
    .map((s) => s.id);
}
