// Estado do Início compartilhado entre a página e o resumo da barra do topo:
// memória medida (uma única consulta a cada 10 s) e contas recarregando.
import { api, type MemoryReport, type Snapshot } from "./api";

/** Acima disso, um serviço em segundo plano vale ser recarregado. */
export const HEAVY_BYTES = 300 * 1024 ** 2;

export const home = $state({
  memory: null as MemoryReport | null,
  /** Contas recarregando para liberar memória (o site leva alguns segundos). */
  reloading: {} as Record<string, boolean>,
});

let watchers = 0;
let timer: ReturnType<typeof setInterval> | undefined;

function refresh() {
  api
    .getMemory()
    .then((m) => (home.memory = m))
    .catch(() => {});
}

/** Mede a memória enquanto alguém mostra; devolve a função para parar. */
export function watchMemory(): () => void {
  if (watchers++ === 0) {
    refresh();
    timer = setInterval(refresh, 10_000);
  }
  return () => {
    if (--watchers === 0) clearInterval(timer);
  };
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
