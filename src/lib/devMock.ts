// Simula o backend no navegador comum (`pnpm dev` fora do Tauri) para
// desenvolver a interface sem o app nativo. Nunca entra no app empacotado.
// Mantém os mesmos padrões do backend (URLs, nomes numerados, validações).
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import notes020 from "../../release-notes/0.2.0.md?raw";
import {
  EMPTY_SNAPSHOT,
  type Contact,
  type DownloadEntry,
  type Mode,
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
  gmail: "https://mail.google.com/mail/u/0/",
  outlook: "https://outlook.live.com/mail/",
  slack: "https://app.slack.com/client",
  teams: "https://teams.microsoft.com/v2/",
};
const DEFAULT_NAME: Record<string, string> = {
  whatsapp: "WhatsApp",
  telegram: "Telegram",
  discord: "Discord",
  gmail: "Gmail",
  outlook: "Outlook",
  slack: "Slack",
  teams: "Teams",
  nextcloud: "Nextcloud Talk",
};
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
      mk("svc-d", "gmail", "Gmail", DEFAULT_URL.gmail, true, 7),
      mk("svc-e", "email", "E-mail da loja", "https://webmail.exemplo.com.br/", true, 2),
    );
    Object.assign(services[0], { emoji: "🏠", color: "#16a34a", idleMinutes: 24 });
    Object.assign(services[3], { chat: "Proposta de orçamento — novembro", idleMinutes: 5 });
    Object.assign(services[1], { emoji: "💼", color: "#2563eb", idleMinutes: 3 });
    // `?modos`: contas marcadas como trabalho/pessoal.
    if (new URLSearchParams(location.search).has("modos")) {
      services[0].context = "personal";
      services[1].context = "work";
      services[3].context = "work";
      services[4].context = "work";
    }
    settings.updates.noticeSeen = true;
    // `?diagnostico`: o campo de texto do WhatsApp sumiu (modo ligado).
    if (new URLSearchParams(location.search).has("diagnostico")) {
      settings.diagnostics = true;
      services[0].probes = [
        { probe: "list", state: "found" },
        { probe: "search", state: "found" },
        { probe: "header", state: "found" },
        { probe: "composer", state: "broken" },
      ];
      services[3].probes = [
        { probe: "inbox", state: "found" },
        { probe: "subject", state: "unknown" },
      ];
    }
    // `?alertas`: uma conta fora do ar e outra travada.
    if (new URLSearchParams(location.search).has("alertas")) {
      services[0].status = "loggedOut";
      services[1].status = "unresponsive";
      services[4].url = "http://webmail.exemplo.com.br/";
    }
  }
  let autostart = false;
  let locked = false;
  // `?semhello`: computador sem autenticação do sistema (usa o PIN do Papeo).
  const systemAuth = !new URLSearchParams(location.search).has("semhello");
  let pin: string | null = null;
  let orphans = 2;
  // `?update`: versão nova disponível; `?v=0.2.0`: simula o app já atualizado.
  const params = new URLSearchParams(location.search);
  const appVersion = params.get("v") ?? "0.1.0";
  // A versão "nova" é sempre a seguinte à instalada (com `?v=0.9.0`, a 0.10.0),
  // para o histórico do "O que mudou" ficar coerente; as notas são as da 0.2.0.
  const [maj, min] = appVersion.split(".").map(Number);
  const nextVersion = `${maj}.${min + 1}.0`;
  let update: Snapshot["update"] = params.has("update")
    ? { state: "available", version: nextVersion, notes: notes020 }
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
  // Versão do estado (como `Snapshot::rev` no app).
  let rev = 0;
  // Modos (mesmas regras de `papeo_core::Settings::current_mode`).
  let manual: { mode: Mode; scheduled: Mode } | null = null;
  const scheduledMode = (): Mode => {
    if (settings.mode !== "auto") return settings.mode;
    const { days, start, end } = settings.workHours;
    const d = new Date();
    const minute = d.getHours() * 60 + d.getMinutes();
    const day = d.getDay();
    const work =
      start === end
        ? false
        : start < end
          ? days.includes(day) && minute >= start && minute < end
          : (days.includes(day) && minute >= start) || (days.includes((day + 6) % 7) && minute < end);
    return work ? "work" : "personal";
  };
  const currentMode = (): Mode => {
    const scheduled = scheduledMode();
    return settings.mode === "auto" && manual?.scheduled === scheduled ? manual.mode : scheduled;
  };
  const inMode = (s: ServiceView, mode: Mode) => mode === "all" || !s.context || s.context === mode;
  // Leitor de e-mail simulado: contas que pediram o site e a caixa de exemplo.
  const siteView = new Set<string>();
  const mailSetup = new Set<string>();
  const mailbox = mockMailbox();
  const snap = (): Snapshot => {
    const mode = currentMode();
    const list = structuredClone(services).map((s) => ({
      ...s,
      inMode: inMode(s, mode),
      reader: (!!s.mail || mailSetup.has(s.id)) && !siteView.has(s.id),
    }));
    const hidden = new Set(list.filter((s) => !s.inMode).map((s) => s.id));
    return {
      rev: ++rev,
      mode,
      modeManual: settings.mode === "auto" && manual?.scheduled === scheduledMode(),
      tasks: locked ? { overdue: 0, today: 0 } : counts(),
      services: list,
      active,
      totalUnread: list.filter((s) => s.inMode).reduce((n, s) => n + s.unread, 0),
      settings: structuredClone(settings),
      dndActive: (settings.dndUntil ?? 0) > now(),
      autostart,
      downloads: structuredClone(downloads),
      downloadsDir: settings.downloadsDir ?? "/Users/voce/Downloads",
      locked,
      lockAvailable: pin !== null || systemAuth,
      lockMethod: pin !== null ? "pin" : systemAuth ? "system" : "none",
      systemAuth,
      update: structuredClone(update),
      appVersion,
      recent:
        locked || settings.notificationPreview === "hidden"
          ? []
          : structuredClone(recent).filter((c) => !hidden.has(c.serviceId)),
      silenceSummary:
        locked || !silence
          ? null
          : settings.notificationPreview === "hidden"
            ? { ...silence, accounts: silence.accounts.map((a) => ({ ...a, chats: [] })) }
            : structuredClone(silence),
      // Idioma (spec 20): `?en` força inglês; "system" segue o navegador.
      lang:
        settings.language === "system"
          ? /^pt([-_]|$)/i.test(navigator.language) && !new URLSearchParams(location.search).has("en")
            ? "pt"
            : "en"
          : settings.language,
    };
  };

  // `?silencio`: resumo de um não perturbe que acabou de terminar.
  let silence: import("./api").SilenceSummary | null = new URLSearchParams(location.search).has("silencio")
    ? {
        since: Math.floor(Date.now() / 1000) - 3 * 3600,
        until: Math.floor(Date.now() / 1000) - 120,
        total: 14,
        accounts: [
          { serviceId: services[0].id, count: 9, chats: ["Ana", "Família", "Grupo da obra", "Carlos", "Bia"] },
          { serviceId: services[1].id, count: 5, chats: ["Suporte"] },
        ],
      }
    : null;
  const held = new Map<string, ReturnType<typeof setTimeout>>();
  // Assinaturas formatadas (spec 22): fora do estado, como no backend.
  const signatureHtml = new Map<string, string>();
  const drafts = new Map<string, import("./api").MailDraft>();
  let unsentTaken = false;

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
          // `?alertas`: mantém o aviso visível na barra de cima ao abrir a conta.
          if (!params.has("alertas")) s.status = "ok";
          s.idleMinutes = 0;
        }
        return snap();
      case "add_service": {
        const kind = args.kind as ServiceView["kind"];
        if ((kind === "custom" || kind === "email") && !args.url) throw "Informe a URL do serviço.";
        const id = `svc-${Math.random().toString(16).slice(2, 8)}`;
        const base = (args.name as string) || DEFAULT_NAME[kind] || String(args.url);
        let name = base;
        for (let n = 2; !args.name && services.some((s) => s.name === name); n++) name = `${base} ${n}`;
        services.push(mk(id, kind, name, (args.url as string) || DEFAULT_URL[kind] || "", true, 0));
        if (args.reader && (kind === "gmail" || kind === "email" || kind === "outlook")) mailSetup.add(id);
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
        s.context = (args.context as ServiceView["context"] | null) ?? undefined;
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
      case "mail_folders":
        return mailbox.folders;
      case "mail_list": {
        const q = String(args.query ?? "").toLowerCase();
        const all = mailbox.messages(String(args.folder)).filter(
          (m) => !q || `${m.subject} ${m.from.name} ${m.preview}`.toLowerCase().includes(q),
        );
        const page = Number(args.page ?? 0);
        return { messages: all.slice(page * 50, page * 50 + 50), total: all.length };
      }
      case "mail_open":
        return mailbox.open(String(args.folder), Number(args.uid));
      case "mail_set_flag":
        mailbox.flag(String(args.folder), args.uids as number[], String(args.flag), Boolean(args.on));
        return undefined;
      case "mail_move":
      case "mail_delete":
        mailbox.remove(String(args.folder), args.uids as number[]);
        return undefined;
      case "mail_save_attachment":
        return "/Users/voce/Downloads/anexo.pdf";
      case "mail_contacts": {
        const q = String(args.query).trim().toLowerCase();
        const known = [
          { name: "Ana Souza", email: "ana.souza@exemplo.com" },
          { name: "Carlos Lima", email: "carlos@empresa.com.br" },
          { name: "Dani Rocha", email: "dani.rocha@exemplo.com" },
          { name: "", email: "financeiro@empresa.com.br" },
        ];
        return q ? known.filter((a) => a.name.toLowerCase().includes(q) || a.email.includes(q)) : [];
      }
      case "mail_save_attachments":
        return `/Users/voce/Downloads/${String(args.subject || "Anexos")}`;
      case "mail_preview_attachment":
        // Sem a janela nativa no navegador: finge que abriu.
        return true;
      case "mail_reveal_file":
        return undefined;
      case "mail_send": {
        const draft = args.draft as import("./api").MailDraft;
        if (!draft.to.length && !draft.cc.length && !draft.bcc.length) throw "Informe para quem enviar.";
        return undefined;
      }
      case "mail_save_draft": {
        // `?semrascunhos`: conta sem a pasta Rascunhos.
        if (new URLSearchParams(location.search).has("semrascunhos")) throw "Esta conta não tem pasta Rascunhos no servidor.";
        const draft = args.draft as import("./api").MailDraft;
        return draft.draftToken ?? Math.random().toString(16).slice(2).padEnd(24, "0").slice(0, 24);
      }
      case "mail_discard_draft":
        return undefined;
      case "mail_open_draft":
        return {
          to: ["ana@exemplo.com"],
          cc: [],
          bcc: ["chefe@exemplo.com"],
          subject: "Proposta (rascunho)",
          body: "Oi, Ana!\n\nSegue a proposta",
          inReplyTo: null,
          references: [],
          attachments: [],
          draftToken: null,
          replaces: { folder: String(args.folder), uid: Number(args.uid) },
        };
      case "mail_send_later": {
        // Simula a espera; assunto com "falha" volta com erro.
        const draft = args.draft as import("./api").MailDraft;
        if (!draft.to.length && !draft.cc.length && !draft.bcc.length) throw "Informe para quem enviar.";
        const holdId = Math.random().toString(16).slice(2).padEnd(24, "0").slice(0, 24);
        const id = String(args.id);
        const secs = settings.mailUndoSecs;
        held.set(
          holdId,
          setTimeout(() => {
            held.delete(holdId);
            const fail = /falha/i.test(draft.subject);
            void emit("papeo://mail-send", {
              holdId,
              id,
              error: fail ? "O servidor recusou o envio (simulado)." : null,
              draft: fail ? draft : null,
            });
          }, secs * 1000),
        );
        drafts.set(holdId, draft);
        return { holdId, secs };
      }
      case "mail_send_cancel": {
        const holdId = String(args.holdId);
        const timer = held.get(holdId);
        if (timer === undefined) return null;
        clearTimeout(timer);
        held.delete(holdId);
        return drafts.get(holdId) ?? null;
      }
      case "mail_unsent":
        // `?naoenviado`: um e-mail que sobrou da sessão anterior.
        if (new URLSearchParams(location.search).has("naoenviado") && !unsentTaken) {
          unsentTaken = true;
          return [
            {
              holdId: "0".repeat(24),
              draft: { to: ["ana@exemplo.com"], cc: [], bcc: [], subject: "Proposta", body: "Segue a proposta.", attachments: [] },
              maybeSent: false,
              error: null,
            },
          ];
        }
        return [];
      case "dismiss_silence_summary":
        silence = null;
        return snap();
      case "mail_unsent_taken":
        return undefined;
      case "set_mail_signature": {
        const s = find();
        if (!s.mail) throw "Esta conta não tem o e-mail por IMAP configurado.";
        const text = String(args.signature).trim().slice(0, 2000);
        s.mail = { ...s.mail, signature: text || undefined };
        const html = text && typeof args.html === "string" ? args.html : "";
        if (html) signatureHtml.set(s.id, html);
        else signatureHtml.delete(s.id);
        s.signatureRich = Boolean(html);
        return snap();
      }
      case "mail_signature": {
        const s = find();
        if (!s.mail) throw "Esta conta não tem o e-mail por IMAP configurado.";
        return { text: s.mail.signature ?? "", html: signatureHtml.get(s.id) ?? "" };
      }
      case "mail_show_site": {
        const s = find();
        if (args.site) siteView.add(s.id);
        else siteView.delete(s.id);
        active = s.id;
        s.loaded = Boolean(args.site);
        return snap();
      }
      case "mail_microsoft_cancel":
        return undefined;
      case "mail_microsoft_login": {
        // Simula o navegador: entra como a conta de exemplo.
        const s = find();
        s.mail = {
          host: "outlook.office365.com",
          port: 993,
          user: "voce@outlook.com",
          sleepSite: Boolean(args.sleepSite),
          auth: "microsoft",
        };
        s.mailStatus = { state: "ok" };
        return snap();
      }
      case "set_mail_account": {
        // Simula: senha "errada" é recusada; o resto conecta na hora.
        const s = find();
        const input = args.account as import("./api").MailInput | null;
        if (!input) {
          delete s.mail;
          delete s.mailStatus;
          return snap();
        }
        if (s.mail?.auth === "microsoft") {
          s.mail = { ...s.mail, sleepSite: input.sleepSite };
          return snap();
        }
        if (!input.password && !s.mail) throw "Digite a senha: ela vale só para o servidor e o usuário em que foi informada.";
        s.mail = {
          host: input.host,
          port: input.port || (input.starttls ? 143 : 993),
          user: input.user,
          sleepSite: input.sleepSite,
          starttls: input.starttls || undefined,
          wholeInbox: (s.kind === "gmail" && input.wholeInbox) || undefined,
        };
        s.mailStatus = input.password === "errada" ? { state: "auth" } : { state: "ok" };
        return snap();
      }
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
      case "set_mode": {
        const mode = args.mode as Mode;
        if (settings.mode === "auto") {
          const scheduled = scheduledMode();
          manual = mode === scheduled ? null : { mode, scheduled };
        } else {
          settings.mode = mode;
          manual = null;
        }
        return snap();
      }
      case "set_mode_schedule":
        settings.mode = args.mode as Settings["mode"];
        settings.workHours = args.workHours as Settings["workHours"];
        manual = null;
        return snap();
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
      case "attachment_state":
        // `#attachment?windows`: Excel sem visualização (como no Windows).
        return {
          fileName: "Orçamento 2026.xlsx",
          view: new URLSearchParams(location.search).has("windows") ? "external" : "quickLook",
          saved: false,
        };
      case "attachment_save":
      case "attachment_open_app":
      case "attachment_reveal":
      case "attachment_close":
        return null;
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
        if (args.enabled && pin === null && !systemAuth) throw "Crie um PIN do Papeo.";
        settings.lock = { enabled: Boolean(args.enabled), afterMinutes: Number(args.afterMinutes) };
        return snap();
      case "set_appearance":
        settings.appearance = args.appearance as Settings["appearance"];
        return snap();
      case "lock_now":
        if (settings.lock.enabled) locked = true;
        return snap();
      case "unlock":
        if (pin !== null && args.pin !== pin) throw "PIN incorreto.";
        locked = false;
        return snap();
      case "set_lock_pin":
        if (locked) throw "Desbloqueie o Papeo primeiro.";
        if (pin !== null && args.current !== pin) throw "PIN incorreto.";
        if (args.pin !== null && String(args.pin).length < 4) throw "Use pelo menos 4 caracteres.";
        if (pin === null && args.pin !== null) settings.lock.enabled = true;
        pin = (args.pin as string | null) ?? null;
        if (pin === null && !systemAuth) settings.lock.enabled = false;
        return snap();
      case "forget_lock_pin":
        pin = null;
        settings.lock.enabled = false;
        locked = false;
        return snap();
      case "open_signin_settings":
        return null;
      case "check_update":
        update = { state: "available", version: nextVersion, notes: notes020 };
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
      case "reply_later": {
        const serviceId = String(args.serviceId);
        const title = String(args.chat);
        const c = recent.find((x) => x.serviceId === serviceId && x.chat === title);
        const last = c && [...c.messages].reverse().find((m) => !m.mine);
        addTask({
          text: last ? `Responder: ${last.body}` : `Responder ${title}`,
          remindAt: Number(args.at),
          contact: contactFor(serviceId, title),
        });
        return visible();
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
  }, { shouldMockEvents: true });
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
    inMode: true,
  };
}

/** Caixa de e-mail de exemplo para o leitor no navegador comum. */
function mockMailbox() {
  type Summary = import("./api").MailSummary;
  const folders: import("./api").MailFolder[] = [
    { path: "INBOX", name: "Caixa de Entrada", depth: 0, role: "inbox" },
    { path: "Rascunhos", name: "Rascunhos", depth: 0, role: "drafts" },
    { path: "Enviados", name: "Enviados", depth: 0, role: "sent" },
    { path: "Arquivo", name: "Arquivo", depth: 0, role: "archive" },
    { path: "Spam", name: "Spam", depth: 0, role: "junk" },
    { path: "Lixeira", name: "Lixeira", depth: 0, role: "trash" },
    { path: "Projetos", name: "Projetos", depth: 0, role: null },
  ];
  const nowSecs = Math.floor(Date.now() / 1000);
  const people = ["Ana Souza", "Banco Exemplo", "Carlos Lima", "Equipe Papeo", "Dani Rocha"];
  const subjects = [
    "Reunião de quinta",
    "Seu extrato chegou",
    "Proposta revisada",
    "Novidades da semana",
    "Fotos da viagem",
  ];
  const boxes = new Map<string, Summary[]>();
  boxes.set(
    "INBOX",
    Array.from({ length: 64 }, (_, i) => ({
      uid: 1000 - i,
      from: { name: people[i % 5], email: `contato${i % 5}@exemplo.com` },
      to: [{ name: "Você", email: "voce@exemplo.com" }],
      subject: `${subjects[i % 5]}${i >= 5 ? ` (${i})` : ""}`,
      date: nowSecs - i * 5400,
      seen: i > 3,
      flagged: i === 2,
      answered: i === 6,
      size: 4000 + i * 100,
      preview: "Olá! Segue o que combinamos. Qualquer dúvida, me avise que eu explico melhor.",
      hasAttachments: i % 4 === 1,
    })),
  );
  const list = (folder: string) => boxes.get(folder) ?? [];
  return {
    folders,
    messages: list,
    open(folder: string, uid: number): import("./api").MailMessage {
      const m = list(folder).find((x) => x.uid === uid);
      if (!m) throw "Mensagem não encontrada.";
      m.seen = true;
      return {
        uid,
        subject: m.subject,
        from: [m.from],
        to: m.to,
        cc: [],
        replyTo: [],
        date: m.date,
        messageId: `m${uid}@exemplo.com`,
        references: [],
        html:
          uid % 2 === 0
            ? `<div style="font-family:Georgia,serif"><h2 style="color:#2563eb">${m.subject}</h2><p>Olá!</p><p>Segue o que combinamos. <a href="https://example.com">Veja aqui</a>.</p><img src="https://example.com/logo.png" alt="logo"></div>`
            : null,
        hasRemote: uid % 2 === 0,
        text: `Olá!\n\nSegue o que combinamos.\n\n${m.from.name}`,
        attachments: m.hasAttachments ? [{ index: 0, name: "proposta.pdf", mime: "application/pdf", size: 182_000 }] : [],
        seen: true,
        flagged: m.flagged,
      };
    },
    flag(folder: string, uids: number[], flag: string, on: boolean) {
      for (const m of list(folder)) {
        if (!uids.includes(m.uid)) continue;
        if (flag === "seen") m.seen = on;
        if (flag === "flagged") m.flagged = on;
      }
    },
    remove(folder: string, uids: number[]) {
      boxes.set(
        folder,
        list(folder).filter((m) => !uids.includes(m.uid)),
      );
    },
  };
}
