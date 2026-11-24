// Simula o backend no navegador comum (`pnpm dev` fora do Tauri) para
// desenvolver a interface sem o app nativo. Nunca entra no app empacotado.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { ServiceView, Snapshot } from "./api";

export function installDevMock() {
  const services: ServiceView[] = [
    mk("svc-a", "whatsapp", "WhatsApp", "https://web.whatsapp.com/", true, 3),
    mk("svc-b", "telegram", "Telegram", "https://web.telegram.org/a/", false, 0),
  ];
  let active: string | null = null;

  const snap = (): Snapshot => ({
    services: structuredClone(services),
    active,
    totalUnread: services.reduce((n, s) => n + s.unread, 0),
  });

  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    const find = () => services.find((s) => s.id === args.id);
    switch (cmd) {
      case "get_state":
        return snap();
      case "activate_service":
        active = (args.id as string | null) ?? null;
        if (active) find()!.loaded = true;
        return snap();
      case "add_service": {
        const kind = args.kind as ServiceView["kind"];
        if (kind === "custom" && !args.url) throw "Informe a URL do serviço.";
        const id = `svc-${Math.random().toString(16).slice(2, 8)}`;
        const base = (args.name as string) || (kind === "custom" ? String(args.url) : kind === "whatsapp" ? "WhatsApp" : "Telegram");
        let name = base;
        for (let n = 2; services.some((s) => s.name === name); n++) name = `${base} ${n}`;
        services.push(mk(id, kind, name, String(args.url ?? ""), true, 0));
        active = id;
        return snap();
      }
      case "remove_service":
        services.splice(services.indexOf(find()!), 1);
        if (active === args.id) active = null;
        return snap();
      case "rename_service":
        find()!.name = String(args.name);
        return snap();
      case "set_notifications":
        find()!.notifications = Boolean(args.enabled);
        return snap();
      case "set_hibernation":
        find()!.hibernateAfterMinutes = Number(args.minutes);
        return snap();
      case "hibernate_service":
        find()!.loaded = false;
        if (active === args.id) active = null;
        return snap();
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
  return { id, kind, name, url, session: id, hibernateAfterMinutes: 0, notifications: true, loaded, unread };
}
