// Simula o backend no navegador comum (`pnpm dev` fora do Tauri) para
// desenvolver a interface sem o app nativo. Nunca entra no app empacotado.
// Mantém os mesmos padrões do backend (URLs, nomes numerados, validações).
import { mockIPC } from "@tauri-apps/api/mocks";
import notes020 from "../../release-notes/0.2.0.md?raw";
import {
  EMPTY_SNAPSHOT,
  type Contact,
  type DownloadEntry,
  type Organizer,
  type ServiceView,
  type Settings,
  type Snapshot,
  type Task,
} from "./api";
import { addDays, dateKey, groupOf, startOfDay } from "./tasks";

const DEFAULT_URL: Record<string, string> = {
  whatsapp: "https://web.whatsapp.com/",
  telegram: "https://web.telegram.org/a/",
  discord: "https://discord.com/app",
};
const DEFAULT_NAME: Record<string, string> = { whatsapp: "WhatsApp", telegram: "Telegram", discord: "Discord" };
const MB = 1024 ** 2;

export function installDevMock() {
  const services: ServiceView[] = [
    mk("svc-a", "whatsapp", "WhatsApp", DEFAULT_URL.whatsapp, true, 3),
    mk("svc-b", "telegram", "Telegram", DEFAULT_URL.telegram, false, 0),
  ];
  services[0].idleMinutes = 24;
  let settings: Settings = structuredClone(EMPTY_SNAPSHOT.settings);
  // `?demo`: dados fictícios para as capturas de tela do README.
  if (new URLSearchParams(location.search).has("demo")) {
    services.splice(
      0,
      services.length,
      mk("svc-a", "whatsapp", "Pessoal", DEFAULT_URL.whatsapp, true, 4),
      mk("svc-c", "whatsapp", "Trabalho", DEFAULT_URL.whatsapp, true, 12),
      mk("svc-b", "telegram", "Telegram", DEFAULT_URL.telegram, false, 0),
    );
    Object.assign(services[0], { emoji: "🏠", color: "#16a34a", idleMinutes: 24 });
    Object.assign(services[1], { emoji: "💼", color: "#2563eb", idleMinutes: 3 });
    settings.updates.noticeSeen = true;
    // `?alertas`: uma conta fora do ar e outra travada.
    if (new URLSearchParams(location.search).has("alertas")) {
      services[0].status = "loggedOut";
      services[1].status = "unresponsive";
    }
  }
  let autostart = false;
  let locked = false;
  let orphans = 2;
  // `?update`: versão nova disponível; `?v=0.2.0`: simula o app já atualizado.
  const params = new URLSearchParams(location.search);
  const appVersion = params.get("v") ?? "0.1.0";
  let update: Snapshot["update"] = params.has("update")
    ? { state: "available", version: "0.2.0", notes: notes020 }
    : { state: "idle" };
  let active: string | null = null;
  const minutesAgo = (m: number) => Math.round(Date.now() / 1000 - m * 60);
  const msg = (body: string, ago: number, mine = false) => ({ body, at: minutesAgo(ago), mine });
  const chat = (serviceId: string, title: string, messages: Snapshot["recent"][number]["messages"]) => {
    const lastIn = [...messages].reverse().find((m) => !m.mine);
    return {
      serviceId,
      title,
      chat: title,
      body: lastIn && !messages[messages.length - 1].mine ? lastIn.body : "",
      at: messages[messages.length - 1].at,
      messages,
      canReply: !serviceId.endsWith("x"),
      unread: !messages[messages.length - 1].mine && title !== "Grupo da família",
    };
  };
  const recent: Snapshot["recent"] = [
    chat("svc-a", "Ana Souza", [
      msg("Oi! Tudo certo pra hoje?", 9),
      msg("Tudo sim 😊", 8, true),
      msg("Chego às 19h, beleza?", 2),
    ]),
    chat("svc-c", "Equipe Vendas", [
      msg("Carlos: Enviei a proposta revisada", 12),
      msg("Marina: Proposta aprovada! 🎉", 6),
    ]),
    chat("svc-b", "Grupo da família", [msg("Mãe: Foto", 15)]),
  ];
  const sendReply = (id: string, title: string, text: string) => {
    const i = recent.findIndex((c) => c.serviceId === id && c.chat === title);
    if (i < 0) throw "Conversa não encontrada.";
    if (/erro/i.test(text)) throw "Não encontrei essa conversa. Abra a conversa para responder.";
    const [c] = recent.splice(i, 1);
    c.messages = [...c.messages, msg(text, 0, true)].slice(-6);
    c.at = minutesAgo(0);
    c.body = "";
    c.unread = false;
    recent.unshift(c);
  };
  const downloads: DownloadEntry[] = [
    { id: 1, serviceId: "svc-a", fileName: "orçamento-2026.pdf", state: "done", at: 0 },
    { id: 2, serviceId: "svc-a", fileName: "foto-praia.jpg", state: "downloading", at: 0 },
  ];

  // Tarefas e anotações (`?demo` traz exemplos).
  const org: Organizer = { tasks: [], contacts: [] };
  const hex = () => Array.from({ length: 32 }, () => Math.floor(Math.random() * 16).toString(16)).join("");
  const ms = () => Date.now();
  const contactFor = (serviceId: string, chat: string): string => {
    const s = services.find((x) => x.id === serviceId);
    if (!s) throw "Serviço não encontrado.";
    let c = org.contacts.find((x) => x.serviceId === serviceId && x.chat === chat);
    if (!c) {
      c = { id: hex(), serviceId, serviceKind: s.kind, serviceName: s.name, chat, note: "", updatedAt: ms() };
      org.contacts.push(c);
    }
    return c.id;
  };
  const addTask = (extra: Partial<Task> & { text: string }) => {
    const t: Task = { id: hex(), done: false, createdAt: ms(), updatedAt: ms(), ...extra };
    org.tasks.push(t);
    return t;
  };
  if (new URLSearchParams(location.search).has("demo")) {
    const today = new Date();
    const at = (d: Date, h: number, m = 0) => {
      const x = new Date(d);
      x.setHours(h, m, 0, 0);
      return Math.floor(x.getTime() / 1000);
    };
    const ana = contactFor("svc-a", "Ana Souza");
    const vendas = contactFor("svc-c", "Equipe Vendas");
    (org.contacts.find((c) => c.id === ana) as Contact).note =
      "Prefere falar à tarde.\nPediu orçamento do jardim em novembro.";
    addTask({ text: "Retornar orçamento do jardim", contact: ana, remindAt: at(today, 17, 30) });
    addTask({ text: "Mandar contrato assinado", contact: vendas, due: dateKey(addDays(startOfDay(today), -1)) });
    addTask({ text: "Pagar o contador", due: dateKey(addDays(startOfDay(today), 2)) });
    addTask({ text: "Ligar para a Ana sobre a entrega", contact: ana, remindAt: at(addDays(today, 1), 9) });
    addTask({ text: "Comprar presente da Marina" });
    addTask({ text: "Enviar relatório semanal", remindAt: at(addDays(today, 2), 10), repeat: "weekly" });
    addTask({ text: "Enviar fotos da obra", contact: ana, done: true, doneAt: Math.floor(ms() / 1000) });
    services[0].chat = "Ana Souza";
  }
  const visible = (): Organizer => structuredClone(org);
  // Sincronização (`?semsync`: versão sem o acesso ao Google).
  const sync = {
    available: !new URLSearchParams(location.search).has("semsync"),
    signedIn: false,
    connected: false,
    needsPassphrase: null as "create" | "enter" | null,
    running: false,
    lastSync: null as number | null,
    error: null as string | null,
    deviceName: "MacBook de Ana",
    devices: 0,
  };
  const syncState = () => ({ ...sync });
  const taskById = (id: unknown) => {
    const t = org.tasks.find((x) => x.id === id);
    if (!t) throw "Tarefa não encontrada.";
    return t;
  };
  const counts = () => {
    const d = new Date();
    let overdue = 0;
    let today = 0;
    for (const t of org.tasks) {
      const g = groupOf(t, d);
      if (g === "overdue") overdue++;
      if (g === "today") today++;
    }
    return { overdue, today };
  };

  const now = () => Date.now() / 1000;
  const snap = (): Snapshot => ({
    tasks: locked ? { overdue: 0, today: 0 } : counts(),
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
    update: structuredClone(update),
    appVersion,
    recent: locked || settings.notificationPreview === "hidden" ? [] : structuredClone(recent),
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
      case "open_chat": {
        const s = find();
        active = s.id;
        s.loaded = true;
        s.status = "ok";
        return snap();
      }
      case "reload_service": {
        const s = find();
        s.status = "ok";
        return null;
      }
      case "send_reply":
        return new Promise((resolve, reject) =>
          setTimeout(() => {
            try {
              sendReply(String(args.id), String(args.chat), String(args.text));
              resolve(snap());
            } catch (e) {
              reject(e);
            }
          }, 900),
        );
      case "show_app":
        if (args.id) active = String(args.id);
        return snap();
      case "hide_popover":
        return null;
      case "plugin:opener|open_url":
        window.open(String(args.url), "_blank", "noopener");
        return null;
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
      case "set_quick_replies":
        settings = { ...settings, quickReplies: args.replies as Settings["quickReplies"] };
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
      case "preview_state":
        return { url: "https://example.com/artigo", title: "Um artigo de exemplo" };
      case "get_memory":
        return {
          services: Object.fromEntries(
            services.filter((s) => s.loaded).map((s, i) => [s.id, (i === 0 ? 1150 : 920) * MB]),
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
      case "set_appearance":
        settings.appearance = args.appearance as Settings["appearance"];
        return snap();
      case "lock_now":
        if (settings.lock.enabled) locked = true;
        return snap();
      case "unlock":
        locked = false;
        return snap();
      case "check_update":
        update = { state: "available", version: "0.2.0", notes: notes020 };
        return snap();
      case "install_update":
        if (locked) throw "Desbloqueie o Papeo primeiro.";
        update = { state: "installing", percent: 40 };
        return snap();
      case "set_updates":
        if (locked) throw "Desbloqueie o Papeo primeiro.";
        settings.updates = { check: Boolean(args.check), noticeSeen: true };
        return snap();
      case "preview_backup":
        return { services: services.map((s) => s.name), tasks: org.tasks.filter((t) => !t.done).length, notes: 1 };
      case "sync_status":
        return syncState();
      case "sync_connect":
        return new Promise((resolve) =>
          setTimeout(() => {
            sync.signedIn = true;
            sync.needsPassphrase = new URLSearchParams(location.search).has("outro") ? "enter" : "create";
            resolve(syncState());
          }, 700),
        );
      case "sync_passphrase":
      case "sync_reset":
        if (String(args.passphrase).length < 10) throw "Use uma frase-senha com pelo menos 10 caracteres.";
        if (cmd === "sync_passphrase" && sync.needsPassphrase === "enter" && args.passphrase !== "frase de teste 123")
          throw "Frase-senha incorreta.";
        Object.assign(sync, { connected: true, needsPassphrase: null, lastSync: Math.floor(ms() / 1000), devices: 1 });
        return syncState();
      case "sync_now":
        sync.lastSync = Math.floor(ms() / 1000);
        return syncState();
      case "sync_disconnect":
        Object.assign(sync, { signedIn: false, connected: false, needsPassphrase: null, lastSync: null, devices: 0 });
        return syncState();
      case "get_organizer":
        if (locked) throw "Desbloqueie o Papeo primeiro.";
        return visible();
      case "save_task": {
        const f = args.task as {
          text: string;
          due?: string | null;
          remindAt?: number | null;
          link?: { serviceId: string; chat: string } | null;
          contactId?: string | null;
          repeat?: Task["repeat"] | null;
        };
        if (!f.text.trim()) throw "Escreva a tarefa.";
        const contact = f.link ? contactFor(f.link.serviceId, f.link.chat) : (f.contactId ?? undefined);
        const fields = {
          text: f.text.trim(),
          due: f.due ?? undefined,
          remindAt: f.remindAt ?? undefined,
          contact,
          repeat: (f.due || f.remindAt) && f.repeat ? f.repeat : undefined,
          updatedAt: ms(),
        };
        if (args.id) Object.assign(taskById(args.id), fields);
        else addTask(fields);
        return visible();
      }
      case "set_task_done": {
        const t = taskById(args.id);
        // Repete: vai para a próxima vez (aproximado no mock).
        const days = { daily: 1, weekdays: 1, weekly: 7, monthly: 30 }[t.repeat ?? "daily"];
        if (args.done && t.repeat) {
          if (t.remindAt) t.remindAt += days * 86_400;
          if (t.due) t.due = dateKey(addDays(new Date(`${t.due}T00:00`), days));
          t.doneAt = Math.floor(ms() / 1000);
          return visible();
        }
        t.done = Boolean(args.done);
        t.doneAt = t.done ? Math.floor(ms() / 1000) : undefined;
        return visible();
      }
      case "snooze_task":
        Object.assign(taskById(args.id), { remindAt: Number(args.until), done: false });
        return visible();
      case "show_task": {
        const t = taskById(args.id);
        const c = org.contacts.find((x) => x.id === t.contact);
        if (c) active = c.serviceId;
        return null;
      }
      case "delete_task":
        org.tasks.splice(org.tasks.indexOf(taskById(args.id)), 1);
        return visible();
      case "save_note": {
        const link = args.link as { serviceId: string; chat: string } | null;
        const id = link ? contactFor(link.serviceId, link.chat) : String(args.contactId);
        const c = org.contacts.find((x) => x.id === id);
        if (c) c.note = String(args.note);
        return visible();
      }
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
