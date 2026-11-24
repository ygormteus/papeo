// Simula o backend no navegador comum (`pnpm dev` fora do Tauri) para
// desenvolver a interface sem o app nativo. Nunca entra no app empacotado.
// Mantém os mesmos padrões do backend (URLs, nomes numerados, validações).
import { mockIPC } from "@tauri-apps/api/mocks";
import { EMPTY_SNAPSHOT, type DownloadEntry, type ServiceView, type Settings, type Snapshot } from "./api";

const DEFAULT_URL: Record<string, string> = {
  whatsapp: "https://web.whatsapp.com/",
  telegram: "https://web.telegram.org/a/",
};
const DEFAULT_NAME: Record<string, string> = { whatsapp: "WhatsApp", telegram: "Telegram" };
const MB = 1024 ** 2;

export function installDevMock() {
  const services: ServiceView[] = [
    mk("svc-a", "whatsapp", "WhatsApp", DEFAULT_URL.whatsapp, true, 3),
    mk("svc-b", "telegram", "Telegram", DEFAULT_URL.telegram, false, 0),
  ];
  services[0].idleMinutes = 24;
  let settings: Settings = structuredClone(EMPTY_SNAPSHOT.settings);
  let autostart = false;
  let locked = false;
  let orphans = 2;
  let active: string | null = null;
  const downloads: DownloadEntry[] = [
    { id: 1, serviceId: "svc-a", fileName: "orçamento-2026.pdf", state: "done", at: 0 },
    { id: 2, serviceId: "svc-a", fileName: "foto-praia.jpg", state: "downloading", at: 0 },
  ];

  const now = () => Date.now() / 1000;
  const snap = (): Snapshot => ({
    services: structuredClone(services),
    active,
    totalUnread: services.reduce((n, s) => n + s.unread, 0),
    settings: structuredClone(settings),
    dndActive: (settings.dndUntil ?? 0) > now(),
    autostart,
    downloads: structuredClone(downloads),
    downloadsDir: settings.downloadsDir ?? "/Users/voce/Downloads",
    locked,
    lockAvailable: true,
  });

  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    const find = () => {
      const s = services.find((x) => x.id === args.id);
      if (!s) throw "Serviço não encontrado.";
      return s;
    };
    switch (cmd) {
      case "get_state":
        return snap();
      case "activate_service":
        active = (args.id as string | null) ?? null;
        if (active) {
          const s = find();
          s.loaded = true;
          s.status = "ok";
          s.idleMinutes = 0;
        }
        return snap();
      case "add_service": {
        const kind = args.kind as ServiceView["kind"];
        if (kind === "custom" && !args.url) throw "Informe a URL do serviço.";
        const id = `svc-${Math.random().toString(16).slice(2, 8)}`;
        const base = (args.name as string) || DEFAULT_NAME[kind] || String(args.url);
        let name = base;
        for (let n = 2; !args.name && services.some((s) => s.name === name); n++) name = `${base} ${n}`;
        services.push(mk(id, kind, name, DEFAULT_URL[kind] ?? String(args.url), true, 0));
        active = id;
        return snap();
      }
      case "remove_service":
        services.splice(services.indexOf(find()), 1);
        if (active === args.id) active = null;
        return snap();
      case "customize_service": {
        const s = find();
        s.name = String(args.name);
        s.color = (args.color as string | null) || undefined;
        s.emoji = (args.emoji as string | null) || undefined;
        return snap();
      }
      case "move_service": {
        const s = find();
        services.splice(services.indexOf(s), 1);
        services.splice(Math.min(Number(args.to), services.length), 0, s);
        return snap();
      }
      case "set_notifications":
        find().notifications = Boolean(args.enabled);
        if (args.enabled) find().mutedUntil = undefined;
        return snap();
      case "mute_service":
        find().mutedUntil = Number(args.minutes) > 0 ? now() + Number(args.minutes) * 60 : undefined;
        return snap();
      case "set_media_policy":
        find().media = args.policy as ServiceView["media"];
        return snap();
      case "set_zoom": {
        const s = find();
        const target = (args.zoom as number | null) ?? s.zoom + Number(args.delta ?? 0);
        s.zoom = Math.round(Math.min(2, Math.max(0.5, target)) * 10) / 10;
        return snap();
      }
      case "set_hibernation":
        find().hibernateAfterMinutes = Number(args.minutes);
        return snap();
      case "hibernate_service": {
        const s = find();
        s.loaded = false;
        s.status = "sleeping";
        if (active === args.id) active = null;
        return snap();
      }
      case "set_dnd": {
        const m = args.minutes as number | null;
        settings.dndUntil = m === null ? null : m === 0 ? Number.MAX_SAFE_INTEGER : now() + m * 60;
        return snap();
      }
      case "set_quiet_hours":
        settings.quietHours = args.quietHours as Settings["quietHours"];
        return snap();
      case "set_preferences":
        settings = { ...settings, ...(args.prefs as Partial<Settings>) };
        return snap();
      case "set_autostart":
        autostart = Boolean(args.enabled);
        return snap();
      case "set_downloads_dir":
        settings.downloadsDir = (args.path as string | null) ?? null;
        return snap();
      case "clear_downloads":
        downloads.splice(0, downloads.length, ...downloads.filter((d) => d.state === "downloading"));
        return snap();
      case "get_memory":
        return {
          services: Object.fromEntries(
            services.filter((s) => s.loaded).map((s, i) => [s.id, (i === 0 ? 420 : 180) * MB]),
          ),
          app: 48 * MB,
        };
      case "clear_service_data": {
        const s = find();
        s.loaded = false;
        s.status = "sleeping";
        return snap();
      }
      case "get_orphan_sessions":
        return orphans;
      case "clean_orphan_sessions": {
        const n = orphans;
        orphans = 0;
        return n;
      }
      case "set_lock":
        if (locked) throw "Desbloqueie o Papeo primeiro.";
        settings.lock = { enabled: Boolean(args.enabled), afterMinutes: Number(args.afterMinutes) };
        return snap();
      case "lock_now":
        if (settings.lock.enabled) locked = true;
        return snap();
      case "unlock":
        locked = false;
        return snap();
      case "preview_backup":
        return { services: services.map((s) => s.name) };
      default:
        return null;
    }
  });
}

function mk(
  id: string,
  kind: ServiceView["kind"],
  name: string,
  url: string,
  loaded: boolean,
  unread: number,
): ServiceView {
  return {
    id,
    kind,
    name,
    url,
    session: id,
    hibernateAfterMinutes: 0,
    notifications: true,
    zoom: 1,
    loaded,
    unread,
    status: loaded ? "ok" : "sleeping",
    idleMinutes: null,
  };
}
