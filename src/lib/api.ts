import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Component } from "svelte";
import { locale, t, tk } from "./i18n.svelte";
import { setTranslator } from "./tasks";

// `tasks.ts` também roda no Node (testes): recebe o tradutor daqui.
setTranslator(t, locale);

export type ServiceKind =
  | "whatsapp"
  | "telegram"
  | "discord"
  | "gmail"
  | "outlook"
  | "email"
  | "custom"
  | "slack"
  | "teams"
  | "nextcloud";
export type MediaPolicy = "ask" | "allow" | "block";
export type LockMethod = "none" | "system" | "pin";
export type Appearance = "system" | "light" | "dark" | "oled";
/** Conteúdo nas notificações: completo · só remetente · oculto. */
export type NotificationPreview = "full" | "sender" | "hidden";
/** ok · dormindo (hibernado) · desconectado (tela de login) · não responde */
export type ServiceStatus = "ok" | "sleeping" | "loggedOut" | "unresponsive";
/** Mundo da conta. Sem valor: aparece em todos os modos. */
export type ServiceContext = "work" | "personal";
/** Modo valendo agora. */
export type Mode = "all" | "work" | "personal";
/** Modo escolhido nas configurações (`auto` segue o expediente). */
export type ModeChoice = Mode | "auto";

export interface ServiceView {
  id: string;
  kind: ServiceKind;
  name: string;
  url: string;
  session: string;
  hibernateAfterMinutes: number;
  /** Repassa as notificações do site como notificações nativas. */
  notifications: boolean;
  /** Silenciado até este instante (segundos Unix). */
  mutedUntil?: number;
  /** Cor própria (`#rrggbb`); sem ela, a cor do tipo de serviço. */
  color?: string;
  /** Emoji ou até 2 letras no lugar do símbolo do serviço. */
  emoji?: string;
  /** Sem valor: padrão do tipo (mensageiros permitem; sites perguntam). */
  media?: MediaPolicy;
  zoom: number;
  userAgent?: string;
  /** A webview do serviço está em memória (não hibernado). */
  loaded: boolean;
  unread: number;
  status: ServiceStatus;
  /** Minutos desde o último uso nesta sessão (0 = em uso). */
  idleMinutes?: number | null;
  /** Conversa aberta no site (nome no cabeçalho; nas caixas de e-mail, o
   *  assunto do e-mail aberto). Só WhatsApp, Telegram, Gmail, Outlook e Roundcube. */
  chat?: string | null;
  /** Trabalho ou pessoal; sem valor, aparece em todos os modos. */
  context?: ServiceContext;
  /** Faz parte do modo atual (aparece na barra, conta e notifica). */
  inMode: boolean;
  /** Avisos por IMAP (servidor e usuário; a senha nunca vem para cá). */
  mail?: MailAccount;
  /** Situação do IMAP ou do Talk pela API (só contas que os usam). */
  mailStatus?: MailStatus;
  /** Leitor do Talk pela API (spec 29): servidor e usuário; a senha de app nunca vem. */
  talk?: { server: string; user: string; uid: string } | null;
  /** Abre no leitor de e-mail do Papeo (o site só quando pedido). */
  reader?: boolean;
  /** Servidor de envio (SMTP) usado pelo leitor, `host:porta`. */
  mailSmtp?: string;
  /** Pontos de apoio do site que a página relatou (diagnóstico, spec 14). */
  probes?: ProbeView[];
  /** Tem assinatura formatada (spec 22); o HTML vem por `mailSignature`. */
  signatureRich?: boolean;
}

/** Ponto de apoio procurado na página do site (spec 14). */
export type Probe = "list" | "search" | "header" | "composer" | "inbox" | "subject";
/** `broken`: ausente há 10 min ou mais; `unknown`: ainda não avaliado. */
export type ProbeState = "found" | "missing" | "broken" | "unknown";
export interface ProbeView {
  probe: Probe;
  state: ProbeState;
}

export const PROBE_LABEL: Record<Probe, string> = {
  list: tk("Lista de conversas"),
  search: tk("Busca"),
  header: tk("Nome da conversa aberta"),
  composer: tk("Campo de texto"),
  inbox: tk("Caixa de Entrada"),
  subject: tk("Assunto do e-mail aberto"),
};

export const PROBE_STATE_LABEL: Record<ProbeState, string> = {
  found: tk("Encontrado"),
  missing: tk("Não encontrado"),
  broken: tk("Não encontrado há 10 min"),
  unknown: tk("Não avaliado"),
};

/** A página da conta perdeu um ponto de apoio há tempo demais. */
export const hasBrokenProbe = (s: ServiceView) => !!s.probes?.some((p) => p.state === "broken");

/** Relatório do diagnóstico sem nomes de contas, conversas ou endereços. */
export function probeReport(version: string, services: ServiceView[]): string {
  const lines = [t("Papeo {versao} — diagnóstico dos sites", { versao: version })];
  services
    .filter((s) => s.probes?.length)
    .forEach((s, i) => {
      lines.push("", t("{tipo} (conta {n})", { tipo: t(KIND_META[s.kind].label), n: i + 1 }));
      for (const p of s.probes!) lines.push(`- ${t(PROBE_LABEL[p.probe])}: ${t(PROBE_STATE_LABEL[p.state])}`);
    });
  if (lines.length === 1) lines.push("", t("Nenhuma conta aberta com pontos verificáveis."));
  return lines.join("\n");
}

export interface MailAccount {
  host: string;
  port: number;
  user: string;
  /** Deixa o site hibernado enquanto o IMAP avisa. */
  sleepSite: boolean;
  /** Conexão comum cifrada com STARTTLS (porta 143), em vez de TLS direto. */
  starttls?: boolean;
  /** Gmail: conta a Caixa de Entrada inteira, não só a aba Principal. */
  wholeInbox?: boolean;
  /** Outlook: login da Microsoft em vez de senha (ausente = senha). */
  auth?: "microsoft";
  /** Assinatura em texto do leitor (ausente = sem assinatura). */
  signature?: string;
}

/** Para ligar ou alterar; senha vazia mantém a guardada. */
export interface MailInput extends MailAccount {
  password?: string;
}

export type MailStatus =
  | { state: "connecting" }
  | { state: "ok" }
  | { state: "offline"; detail: string }
  | { state: "auth" }
  | { state: "noPassword" };

/** Contas que aceitam avisos por IMAP: Gmail e outros com usuário e senha;
 *  o Outlook só com o login da Microsoft. */
export function supportsMailWatch(kind: ServiceKind): boolean {
  return kind === "gmail" || kind === "email" || kind === "outlook";
}

export const GMAIL_IMAP = { host: "imap.gmail.com", port: 993 };

/** Servidores IMAP conhecidos (todos com TLS direto na porta 993). `domains`:
 *  endereços de e-mail que já indicam o provedor. */
export interface MailPreset {
  id: string;
  label: string;
  host: string;
  /** Dica curta mostrada ao escolher. */
  note?: string;
  domains?: string[];
  /** Webmail do provedor (para "Abrir no site"). */
  webmail?: string;
}

export const MAIL_PRESETS: MailPreset[] = [
  {
    id: "locaweb",
    webmail: "https://webmail-seguro.com.br/",
    label: "Locaweb",
    host: "email-ssl.com.br",
    note: tk("Use o e-mail completo como usuário."),
  },
  {
    id: "kinghost",
    label: "KingHost",
    host: "imap.kinghost.net",
    note: tk("Em domínio de revenda, o servidor é imap.uni5.net."),
  },
  { id: "hostinger",
    webmail: "https://mail.hostinger.com/", label: "Hostinger", host: "imap.hostinger.com", note: tk("Use o e-mail completo como usuário.") },
  {
    id: "uol",
    webmail: "https://email.uol.com.br/",
    label: "UOL",
    host: "imap.uol.com.br",
    note: tk("Ative o IMAP no webmail do UOL: Configurações → IMAP."),
    domains: ["uol.com.br"],
  },
  {
    id: "bol",
    webmail: "https://email.bol.uol.com.br/",
    label: "BOL",
    host: "imap.bol.com.br",
    note: tk("O BOL só libera IMAP no plano pago."),
    domains: ["bol.com.br"],
  },
  { id: "terra",
    webmail: "https://mail.terra.com.br/", label: "Terra", host: "imap.terra.com.br", domains: ["terra.com.br"] },
  {
    id: "yahoo",
    webmail: "https://mail.yahoo.com/",
    label: "Yahoo",
    host: "imap.mail.yahoo.com",
    note: tk("O Yahoo pede uma senha de app, criada na segurança da conta."),
    domains: ["yahoo.com", "yahoo.com.br", "ymail.com"],
  },
  {
    id: "icloud",
    webmail: "https://www.icloud.com/mail/",
    label: "iCloud",
    host: "imap.mail.me.com",
    note: tk("O iCloud pede uma senha de app, criada em account.apple.com."),
    domains: ["icloud.com", "me.com", "mac.com"],
  },
];

/** Webmail para "Abrir no site": o do provedor ou `webmail.<domínio>`. */
export function webmailFor(address: string, preset?: MailPreset): string {
  if (preset?.webmail) return preset.webmail;
  const domain = address.trim().toLowerCase().split("@")[1] ?? "";
  return /^[a-z0-9.-]+\.[a-z]{2,}$/.test(domain) ? `https://webmail.${domain}/` : "";
}

/** Provedor reconhecido pelo domínio do endereço de e-mail. */
export function presetForAddress(address: string): MailPreset | undefined {
  const domain = address.trim().toLowerCase().split("@")[1];
  return domain ? MAIL_PRESETS.find((p) => p.domains?.includes(domain)) : undefined;
}

export type Repeat = "daily" | "weekdays" | "weekly" | "monthly";

export const REPEAT_OPTIONS: { value: Repeat; label: string }[] = [
  { value: "daily", label: tk("Todo dia") },
  { value: "weekdays", label: tk("Dias úteis") },
  { value: "weekly", label: tk("Toda semana") },
  { value: "monthly", label: tk("Todo mês") },
];

/** Tarefa; com `remindAt`, é um lembrete. */
export interface Task {
  id: string;
  text: string;
  done: boolean;
  /** Segundos Unix. */
  doneAt?: number;
  /** Dia em que vence (`AAAA-MM-DD`, data local). */
  due?: string;
  /** Quando avisar (segundos Unix). */
  remindAt?: number;
  reminded?: number;
  /** `Contact.id`; sem valor = tarefa solta. */
  contact?: string;
  /** Repete (concluir passa para a próxima vez). */
  repeat?: Repeat;
  createdAt: number;
  updatedAt: number;
}

/** Conversa de uma conta (`chat` vazio = a conta inteira) e a anotação dela. */
export interface Contact {
  id: string;
  serviceId: string;
  serviceKind: string;
  serviceName: string;
  chat: string;
  note: string;
  updatedAt: number;
}

export interface Organizer {
  tasks: Task[];
  contacts: Contact[];
}

export const EMPTY_ORGANIZER: Organizer = { tasks: [], contacts: [] };

/** Conversa a ligar: conta + nome da conversa ("" = a conta inteira). */
export interface ChatLink {
  serviceId: string;
  chat: string;
}

export interface TaskFields {
  text: string;
  due?: string | null;
  remindAt?: number | null;
  /** Liga a esta conversa… */
  link?: ChatLink | null;
  /** …ou mantém este contato. Sem os dois: tarefa solta. */
  contactId?: string | null;
  repeat?: Repeat | null;
}

export interface QuietHours {
  enabled: boolean;
  /** Minutos desde a meia-noite. */
  start: number;
  end: number;
}

export interface WorkHours {
  /** 0 = domingo … 6 = sábado. */
  days: number[];
  /** Minutos desde a meia-noite. */
  start: number;
  end: number;
}

export interface Settings {
  dndUntil?: number | null;
  quietHours: QuietHours;
  downloadsDir?: string | null;
  closeToTray: boolean;
  startHidden: boolean;
  openOnNotificationFocus: boolean;
  notificationPreview: NotificationPreview;
  /** Esconde a janela em capturas e no compartilhamento de tela. */
  protectScreen: boolean;
  /** `afterMinutes` 0 = ao esconder a janela. */
  lock: { enabled: boolean; afterMinutes: number };
  appearance: Appearance;
  /** `noticeSeen` falso: o aviso da verificação ainda não foi mostrado. */
  updates: { check: boolean; noticeSeen: boolean };
  /** Recarrega serviço em segundo plano acima deste uso (MB); 0 = desligado. */
  reloadAboveMb: number;
  /** Links que os sites abrem fora: navegador padrão ou prévia do Papeo. */
  openLinks: LinkTarget;
  /** Anexos baixados nos serviços: salvar direto ou visualizar antes. */
  attachments: AttachmentMode;
  /** Mensagens de voz: som original, limpeza do Papeo ou redução do sistema (macOS). */
  voiceCapture: VoiceCapture;
  /** Textos salvos para a resposta rápida, chamados com "/". */
  quickReplies: QuickReply[];
  mode: ModeChoice;
  /** Expediente do modo automático. */
  workHours: WorkHours;
  /** Hiberna as contas fora do modo atual. */
  sleepOutsideMode: boolean;
  /** Moldura com a cor da conta em volta do site aberto. */
  accountFrame: boolean;
  /** Modo de diagnóstico: aviso na barra do topo quando o site muda (spec 14). */
  diagnostics: boolean;
  /** Espera antes de o leitor enviar um e-mail, para desfazer (0, 5, 10 ou 20 s). */
  mailUndoSecs: number;
  /** Resumo do que chegou durante o não perturbe (spec 17). */
  silenceSummary: boolean;
  /** Idioma: o do sistema, português ou inglês (spec 20). */
  language: Language;
}

export type Language = "system" | "pt" | "en";

/** O que uma conta recebeu durante o silêncio. */
export interface SilencedAccount {
  serviceId: string;
  count: number;
  /** Conversas, a mais recente primeiro (vazia com a prévia oculta). */
  chats: string[];
}

export interface SilenceSummary {
  since: number;
  until: number;
  total: number;
  accounts: SilencedAccount[];
}

export interface QuickReply {
  /** Atalho curto, sem "/" (pode ficar vazio). */
  shortcut: string;
  text: string;
}

export type LinkTarget = "browser" | "preview";
export type AttachmentMode = "save" | "preview";
export type VoiceCapture = "instant" | "enhanced" | "clean";

export type UpdateStatus =
  | { state: "idle" | "checking" | "upToDate" }
  | { state: "available"; version: string; notes?: string | null; notesEn?: string | null }
  | { state: "installing"; percent?: number | null }
  | { state: "failed"; message: string };

export interface DownloadEntry {
  id: number;
  serviceId: string;
  fileName: string;
  state: "downloading" | "done" | "failed";
  at: number;
}

/** Tela dividida: duas contas lado a lado; `active` é uma das duas. */
export interface Split {
  left: string;
  right: string;
  /** Fração da largura da metade esquerda (0,25–0,75). */
  ratio: number;
}

export interface Snapshot {
  services: ServiceView[];
  active: string | null;
  split: Split | null;
  totalUnread: number;
  settings: Settings;
  dndActive: boolean;
  autostart: boolean;
  downloads: DownloadEntry[];
  downloadsDir: string;
  /** Bloqueado: só a tela de desbloqueio aparece. */
  locked: boolean;
  /** Há como desbloquear: PIN do Papeo ou autenticação do sistema. */
  lockAvailable: boolean;
  /** Como o bloqueio confirma a identidade (o PIN do Papeo tem prioridade). */
  lockMethod: LockMethod;
  /** Touch ID/senha do Mac ou Windows Hello disponíveis. */
  systemAuth: boolean;
  update: UpdateStatus;
  appVersion: string;
  /** Conversas com mensagens recentes, mais nova primeiro. Só na memória;
   * vazia com o app bloqueado ou a prévia oculta. */
  recent: RecentChat[];
  /** Resumo do último silêncio, até fechar o cartão (vazio bloqueado). */
  silenceSummary: SilenceSummary | null;
  /** Tarefas abertas atrasadas e para hoje (zero com o app bloqueado). */
  tasks: { overdue: number; today: number };
  /** Modo valendo agora. */
  mode: Mode;
  /** Trocado à mão no automático (vale até o expediente mudar). */
  modeManual: boolean;
  /** Versão do estado: um estado com versão menor que a atual chegou atrasado. */
  rev: number;
  /** Idioma efetivo (spec 20); ausente no estado vazio de antes da carga. */
  lang?: "pt" | "en";
}

export interface ChatMessage {
  body: string;
  /** Segundos Unix. */
  at: number;
  /** Enviada por você pelo Papeo (resposta rápida). */
  mine: boolean;
}

export interface RecentChat {
  serviceId: string;
  /** Nome da conversa (contato ou grupo) para exibir; pode vir encurtado. */
  title: string;
  /** Nome completo, usado para abrir e responder. */
  chat: string;
  /** Última mensagem recebida; vazia com a prévia "só remetente". */
  body: string;
  /** Última atividade (segundos Unix). */
  at: number;
  /** Mais antiga primeiro; vazia com a prévia "só remetente". */
  messages: ChatMessage[];
  /** WhatsApp/Telegram: dá para responder pela mini janela. */
  canReply: boolean;
  /** Mensagem recebida depois da última vez que a conversa foi vista. */
  unread: boolean;
}

/** Limite de texto da resposta rápida (igual ao backend). */
export const MAX_REPLY_CHARS = 4096;

export interface MemoryReport {
  /** Bytes por serviço carregado. */
  services: Record<string, number>;
  app: number;
}

export interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const EMPTY_SNAPSHOT: Snapshot = {
  split: null,
  services: [],
  active: null,
  totalUnread: 0,
  settings: {
    dndUntil: null,
    quietHours: { enabled: false, start: 22 * 60, end: 7 * 60 },
    downloadsDir: null,
    closeToTray: true,
    startHidden: true,
    openOnNotificationFocus: true,
    notificationPreview: "full",
    protectScreen: false,
    lock: { enabled: false, afterMinutes: 5 },
    appearance: "system",
    updates: { check: true, noticeSeen: false },
    reloadAboveMb: 2048,
    openLinks: "browser",
    attachments: "save",
    voiceCapture: "instant",
    quickReplies: [],
    mode: "all",
    workHours: { days: [1, 2, 3, 4, 5], start: 9 * 60, end: 18 * 60 },
    sleepOutsideMode: false,
    accountFrame: true,
    diagnostics: false,
    mailUndoSecs: 5,
    silenceSummary: true,
    language: "system",
  },
  dndActive: false,
  autostart: false,
  downloads: [],
  downloadsDir: "",
  locked: false,
  lockAvailable: false,
  lockMethod: "none",
  systemAuth: false,
  update: { state: "idle" },
  appVersion: "",
  recent: [],
  silenceSummary: null,
  tasks: { overdue: 0, today: 0 },
  mode: "all",
  modeManual: false,
  rev: 0,
};

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  /** `reader`: e-mail no leitor do Papeo (IMAP) em vez do site (spec 23). */
  addService: (kind: ServiceKind, name?: string, url?: string, reader = false) =>
    invoke<Snapshot>("add_service", { kind, name: name ?? null, url: url ?? null, reader }),
  removeService: (id: string) => invoke<Snapshot>("remove_service", { id }),
  customizeService: (
    id: string,
    name: string,
    color: string | null,
    emoji: string | null,
    context: ServiceContext | null,
  ) => invoke<Snapshot>("customize_service", { id, name, color, emoji, context }),
  moveService: (id: string, to: number) => invoke<Snapshot>("move_service", { id, to }),
  setHibernation: (id: string, minutes: number) =>
    invoke<Snapshot>("set_hibernation", { id, minutes }),
  setMailAccount: (id: string, account: MailInput | null) =>
    invoke<Snapshot>("set_mail_account", { id, account }),
  /** Abre o login da Microsoft no navegador e liga os avisos do Outlook. */
  mailMicrosoftLogin: (id: string, sleepSite: boolean) =>
    invoke<Snapshot>("mail_microsoft_login", { id, sleepSite }),
  /** Desiste do login da Microsoft aberto no navegador. */
  mailMicrosoftCancel: () => invoke<void>("mail_microsoft_cancel"),
  mailFolders: (id: string, refresh = false) => invoke<MailFolder[]>("mail_folders", { id, refresh }),
  /** Até 50 mensagens depois das `skip` mais novas (rolagem infinita). */
  mailList: (id: string, folder: string, skip: number, query: string, filter: MailFilter = "all") =>
    invoke<MailPage>("mail_list", { id, folder, skip, query, filter }),
  /** Pastas e mais novas guardadas no disco da última vez (spec 27). */
  mailCached: (id: string) => invoke<MailCache>("mail_cached", { id }),
  /** Abre a mensagem (e a marca como lida). */
  /** `size`: o da lista; e-mail grande abre sem baixar os anexos (spec 28). */
  mailOpen: (id: string, folder: string, uid: number, peek = false, size?: number) =>
    invoke<MailMessage>("mail_open", { id, folder, uid, peek, size: size ?? null }),
  mailSetFlag: (id: string, folder: string, uids: number[], flag: "seen" | "flagged", on: boolean) =>
    invoke<void>("mail_set_flag", { id, folder, uids, flag, on }),
  /** `target`: caminho da pasta ou `role:archive`, `role:junk`, `role:inbox`. */
  /** Devolve os UIDs novos no destino (para desfazer); vazio se o servidor não diz. */
  mailMove: (id: string, folder: string, uids: number[], target: string) =>
    invoke<number[]>("mail_move", { id, folder, uids, target }),
  /** Como `mailMove` (para a Lixeira); vazio quando apaga de vez. */
  mailDelete: (id: string, folder: string, uids: number[]) =>
    invoke<number[]>("mail_delete", { id, folder, uids }),
  mailMarkAllRead: (id: string, folder: string) => invoke<void>("mail_mark_all_read", { id, folder }),
  /** Salva o anexo na pasta de downloads; devolve o caminho. */
  mailSaveAttachment: (id: string, folder: string, uid: number, index: number) =>
    invoke<string>("mail_save_attachment", { id, folder, uid, index }),
  /** Salva vários anexos numa pasta nova em Downloads, com o nome do
   * assunto (spec 24); devolve o caminho da pasta. */
  mailSaveAttachments: (id: string, folder: string, uid: number, indices: number[], subject: string) =>
    invoke<string>("mail_save_attachments", { id, folder, uid, indices, subject }),
  /** Abre o anexo na janela de visualização (spec 21); `false`: tipo sem
   * visualização (salve em Downloads). */
  mailPreviewAttachment: (id: string, folder: string, uid: number, index: number) =>
    invoke<boolean>("mail_preview_attachment", { id, folder, uid, index }),
  /** Endereços que a conta já usou e combinam com `query` (spec 25). */
  mailContacts: (id: string, query: string) => invoke<MailAddr[]>("mail_contacts", { id, query }),
  mailRevealFile: (path: string) => invoke<void>("mail_reveal_file", { path }),
  mailSend: (id: string, draft: MailDraft) => invoke<void>("mail_send", { id, draft }),
  /** Salva o rascunho na pasta Rascunhos do servidor; devolve o token dele. */
  mailSaveDraft: (id: string, draft: MailDraft) => invoke<string>("mail_save_draft", { id, draft }),
  mailDiscardDraft: (id: string, token: string | null, replaces: MailDraft["replaces"]) =>
    invoke<void>("mail_discard_draft", { id, token, replaces: replaces ?? null }),
  mailOpenDraft: (id: string, folder: string, uid: number) =>
    invoke<MailOpenedDraft>("mail_open_draft", { id, folder, uid }),
  /** Confere e agenda o envio para o fim da espera (spec 16). */
  mailSendLater: (id: string, draft: MailDraft) => invoke<MailHold>("mail_send_later", { id, draft }),
  /** Desfaz o envio em espera; `null` se ele já saiu. */
  mailSendCancel: (holdId: string) => invoke<MailDraft | null>("mail_send_cancel", { holdId }),
  /** E-mails guardados da conta que ainda não voltaram ao editor. */
  mailUnsent: (id: string) => invoke<MailUnsent[]>("mail_unsent", { id }),
  /** O e-mail guardado voltou ao editor: apaga o arquivo. */
  mailUnsentTaken: (holdId: string) => invoke<void>("mail_unsent_taken", { holdId }),
  /** Assinatura: texto e, se houver, a formatada (spec 22); `html` nulo
   * guarda só o texto. */
  setMailSignature: (id: string, signature: string, html: string | null = null) =>
    invoke<Snapshot>("set_mail_signature", { id, signature, html }),
  /** Assinatura para o editor (a formatada não vem no estado). */
  mailSignature: (id: string) => invoke<{ text: string; html: string }>("mail_signature", { id }),
  /** Conta com o leitor: mostra o site (`true`) ou volta ao leitor. */
  mailShowSite: (id: string, site: boolean) => invoke<Snapshot>("mail_show_site", { id, site }),
  // ---- Leitor do Nextcloud Talk (spec 29)
  /** Abre a aprovação no navegador; o resultado chega por `onTalkLogin`. */
  talkLogin: (id: string) => invoke<void>("talk_login", { id }),
  talkLoginCancel: (id: string) => invoke<void>("talk_login_cancel", { id }),
  talkLogout: (id: string) => invoke<Snapshot>("talk_logout", { id }),
  talkRooms: (id: string) => invoke<TalkRoom[]>("talk_rooms", { id }),
  talkMessages: (id: string, token: string, before?: number) =>
    invoke<TalkChunk>("talk_messages", { id, token, before: before ?? null }),
  /** Espera (até 30 s) mensagens depois de `after`. */
  talkWait: (id: string, token: string, after: number) => invoke<TalkChunk>("talk_wait", { id, token, after }),
  talkSend: (id: string, token: string, text: string, replyTo?: number | null) =>
    invoke<TalkMessage | null>("talk_send", { id, token, text, replyTo: replyTo ?? null }),
  talkRead: (id: string, token: string, last: number) => invoke<void>("talk_read", { id, token, last }),
  talkReact: (id: string, token: string, message: number, emoji: string, on: boolean) =>
    invoke<void>("talk_react", { id, token, message, emoji, on }),
  talkMentions: (id: string, token: string, search: string) =>
    invoke<TalkMention[]>("talk_mentions", { id, token, search }),
  talkAvatar: (id: string, token: string, version: string) =>
    invoke<string | null>("talk_avatar", { id, token, version }),
  talkPreview: (id: string, file: string) => invoke<string | null>("talk_preview", { id, file }),
  /** `open`: visualização (ou salva se não der); devolve o caminho salvo ou "". */
  talkFile: (id: string, path: string, name: string, open: boolean) =>
    invoke<string>("talk_file", { id, path, name, open }),
  talkUpload: (id: string, token: string, path: string) => invoke<void>("talk_upload", { id, token, path }),
  setNotifications: (id: string, enabled: boolean) =>
    invoke<Snapshot>("set_notifications", { id, enabled }),
  muteService: (id: string, minutes: number) => invoke<Snapshot>("mute_service", { id, minutes }),
  setMediaPolicy: (id: string, policy: MediaPolicy) =>
    invoke<Snapshot>("set_media_policy", { id, policy }),
  setZoom: (id: string, change: { zoom?: number; delta?: number }) =>
    invoke<Snapshot>("set_zoom", { id, zoom: change.zoom ?? null, delta: change.delta ?? null }),
  activate: (id: string | null) => invoke<Snapshot>("activate_service", { id }),
  hibernate: (id: string) => invoke<Snapshot>("hibernate_service", { id }),
  /** Abre o serviço e procura a conversa pelo nome na busca do próprio site. */
  /** `exact`: nome inteiro da conversa; senão, abre se houver um só resultado. */
  openChat: (id: string, query: string, exact: boolean) =>
    invoke<Snapshot>("open_chat", { id, query, exact }),
  /** Resposta rápida: o site envia sem o app vir para frente. */
  sendReply: (id: string, chat: string, text: string) =>
    invoke<Snapshot>("send_reply", { id, chat, text }),
  /** Mini janela → app: mostra a janela e, se pedido, a conta/conversa. */
  showApp: (id: string | null = null, chat: string | null = null) =>
    invoke<Snapshot>("show_app", { id, chat }),
  hidePopover: () => invoke<void>("hide_popover"),
  reload: (id: string) => invoke<void>("reload_service", { id }),
  setContentArea: (area: Area) => invoke<void>("set_content_area", { area }),
  setOverlay: (open: boolean) => invoke<void>("set_overlay", { open }),
  /** Tela dividida: `id` à direita da conta aberta (spec 30). */
  splitOpen: (id: string, ratio: number) => invoke<Snapshot>("split_open", { id, ratio }),
  splitClose: () => invoke<Snapshot>("split_close"),
  splitRatio: (ratio: number) => invoke<void>("split_ratio", { ratio }),
  /** `null` desliga; 0 = até reativar; n = por n minutos. */
  setDnd: (minutes: number | null) => invoke<Snapshot>("set_dnd", { minutes }),
  setQuietHours: (quietHours: QuietHours) => invoke<Snapshot>("set_quiet_hours", { quietHours }),
  /** Troca rápida; no automático, vale até o expediente mudar. */
  setMode: (mode: Mode) => invoke<Snapshot>("set_mode", { mode }),
  setModeSchedule: (mode: ModeChoice, workHours: WorkHours) =>
    invoke<Snapshot>("set_mode_schedule", { mode, workHours }),
  setPreferences: (
    prefs: Partial<
      Pick<
        Settings,
        | "closeToTray"
        | "startHidden"
        | "openOnNotificationFocus"
        | "notificationPreview"
        | "protectScreen"
        | "reloadAboveMb"
        | "openLinks"
        | "attachments"
        | "voiceCapture"
        | "sleepOutsideMode"
        | "accountFrame"
        | "diagnostics"
        | "mailUndoSecs"
        | "silenceSummary"
        | "language"
      >
    >,
  ) => invoke<Snapshot>("set_preferences", { prefs }),
  dismissSilenceSummary: () => invoke<Snapshot>("dismiss_silence_summary"),
  setQuickReplies: (replies: QuickReply[]) => invoke<Snapshot>("set_quick_replies", { replies }),
  setAutostart: (enabled: boolean) => invoke<Snapshot>("set_autostart", { enabled }),
  setDownloadsDir: (path: string | null) => invoke<Snapshot>("set_downloads_dir", { path }),
  revealDownload: (id: number) => invoke<void>("reveal_download", { id }),
  openDownloadsDir: () => invoke<void>("open_downloads_dir"),
  clearDownloads: () => invoke<Snapshot>("clear_downloads"),
  getMemory: () => invoke<MemoryReport>("get_memory"),
  /** Abre no navegador padrão (plugin opener; só http/https). */
  openUrl: (url: string) => invoke<void>("plugin:opener|open_url", { url }),
  exportConfig: (path: string) => invoke<void>("export_config", { path }),
  previewBackup: (path: string) =>
    invoke<{ services: string[]; tasks?: number | null; notes?: number | null }>("preview_backup", { path }),
  importConfig: (path: string) => invoke<Snapshot>("import_config", { path }),
  /** Sai da conta: troca a sessão e apaga cookies, cache e conversas guardadas. */
  clearServiceData: (id: string) => invoke<Snapshot>("clear_service_data", { id }),
  getOrphanSessions: () => invoke<number>("get_orphan_sessions"),
  cleanOrphanSessions: () => invoke<number>("clean_orphan_sessions"),
  setLock: (enabled: boolean, afterMinutes: number) =>
    invoke<Snapshot>("set_lock", { enabled, afterMinutes }),
  lockNow: () => invoke<Snapshot>("lock_now"),
  unlock: (pin?: string) => invoke<Snapshot>("unlock", { pin: pin ?? null }),
  /** Cria/troca (`pin`) ou remove (`pin` nulo) o PIN do Papeo; `current` = PIN atual. */
  setLockPin: (current: string | null, pin: string | null) =>
    invoke<Snapshot>("set_lock_pin", { current, pin }),
  /** "Esqueci o PIN": sai de todas as contas, apaga tarefas e desbloqueia. */
  forgetLockPin: () => invoke<Snapshot>("forget_lock_pin"),
  /** Windows: abre as opções de entrada (PIN do Windows Hello). */
  openSigninSettings: () => invoke<void>("open_signin_settings"),
  setAppearance: (appearance: Appearance) => invoke<Snapshot>("set_appearance", { appearance }),
  checkUpdate: () => invoke<Snapshot>("check_update"),
  /** Baixa, instala e reinicia o app. */
  installUpdate: () => invoke<Snapshot>("install_update"),
  /** Liga/desliga a verificação automática e marca o aviso como visto. */
  setUpdates: (check: boolean) => invoke<Snapshot>("set_updates", { check }),
  getOrganizer: () => invoke<Organizer>("get_organizer"),
  /** Cria (`id` nulo) ou altera uma tarefa. */
  saveTask: (id: string | null, task: TaskFields) => invoke<Organizer>("save_task", { id, task }),
  /** `seen`: a vez que a pessoa viu (tarefa que repete não pula duas). */
  setTaskDone: (id: string, done: boolean, seen?: Pick<Task, "remindAt" | "due">) =>
    invoke<Organizer>("set_task_done", {
      id,
      done,
      seen: seen ? { remindAt: seen.remindAt ?? null, due: seen.due ?? null } : null,
    }),
  /** Adia o aviso para `until` (segundos Unix). */
  snoozeTask: (id: string, until: number) => invoke<Organizer>("snooze_task", { id, until }),
  deleteTask: (id: string) => invoke<Organizer>("delete_task", { id }),
  /** "Responder depois": lembrete ligado à conversa para `at` (segundos Unix). */
  replyLater: (serviceId: string, chat: string, at: number) =>
    invoke<Organizer>("reply_later", { serviceId, chat, at }),
  /** Mini janela: abre o app na conversa da tarefa (ou em Tarefas). */
  showTask: (id: string) => invoke<void>("show_task", { id }),
  /** Anotação de uma conversa (`link`) ou de um contato existente. */
  saveNote: (target: { link?: ChatLink; contactId?: string }, note: string) =>
    invoke<Organizer>("save_note", {
      link: target.link ?? null,
      contactId: target.contactId ?? null,
      note,
    }),
  /** Barra da prévia de links (só a webview `preview-bar` pode chamar). */
  previewState: () => invoke<PreviewInfo>("preview_state"),
  previewOpenBrowser: () => invoke<void>("preview_open_browser"),
  previewClose: () => invoke<void>("preview_close"),
  attachmentState: () => invoke<AttachmentInfo>("attachment_state"),
  attachmentSave: () => invoke<void>("attachment_save"),
  attachmentOpenApp: () => invoke<void>("attachment_open_app"),
  attachmentReveal: () => invoke<void>("attachment_reveal"),
  attachmentClose: () => invoke<void>("attachment_close"),
};

/** Sincronização pelo Google Drive. */
export interface SyncStatus {
  available: boolean;
  signedIn: boolean;
  connected: boolean;
  /** Falta a frase-senha: criar (primeiro computador) ou digitar. */
  needsPassphrase?: "create" | "enter" | null;
  running: boolean;
  /** Segundos Unix. */
  lastSync?: number | null;
  error?: string | null;
  deviceName: string;
  /** Outros computadores sincronizando. */
  devices: number;
}

export const syncApi = {
  syncStatus: () => invoke<SyncStatus>("sync_status"),
  /** Abre o navegador para entrar na conta Google. */
  syncConnect: () => invoke<SyncStatus>("sync_connect"),
  syncPassphrase: (passphrase: string) => invoke<SyncStatus>("sync_passphrase", { passphrase }),
  syncNow: () => invoke<SyncStatus>("sync_now"),
  /** Esqueceu a frase: apaga a nuvem e recomeça com outra. */
  syncReset: (passphrase: string) => invoke<SyncStatus>("sync_reset", { passphrase }),
  /** `forget`: apaga da nuvem a cópia deste computador. */
  syncDisconnect: (forget: boolean) => invoke<SyncStatus>("sync_disconnect", { forget }),
};

export interface PreviewInfo {
  url: string;
  title: string;
}

export function onPreview(cb: (p: PreviewInfo) => void): Promise<UnlistenFn> {
  return listen<PreviewInfo>("papeo://preview", (e) => cb(e.payload));
}

/** Anexo aberto na visualização (`src-tauri/src/attachment.rs`). */
export interface AttachmentInfo {
  fileName: string;
  /** Sem valor: nada aberto. `external`: sem visualização aqui (Windows, Office). */
  view?: "quickLook" | "web" | "external" | null;
  saved: boolean;
}

export function onAttachment(cb: (a: AttachmentInfo) => void): Promise<UnlistenFn> {
  return listen<AttachmentInfo>("papeo://attachment", (e) => cb(e.payload));
}

export const ATTACHMENT_OPTIONS: { value: AttachmentMode; label: string }[] = [
  { value: "save", label: tk("Salvar na pasta") },
  { value: "preview", label: tk("Visualizar antes") },
];

export const LINK_OPTIONS: { value: LinkTarget; label: string }[] = [
  { value: "browser", label: tk("No navegador") },
  { value: "preview", label: tk("Na prévia do Papeo") },
];

export type MailRole = "inbox" | "drafts" | "sent" | "archive" | "all" | "flagged" | "junk" | "trash";

export interface MailFolder {
  /** Nome no servidor (vai de volta nos comandos). */
  path: string;
  name: string;
  depth: number;
  role?: MailRole | null;
}

export interface MailAddr {
  name: string;
  email: string;
}

export interface MailSummary {
  uid: number;
  from: MailAddr;
  to: MailAddr[];
  subject: string;
  /** Segundos Unix. */
  date: number;
  seen: boolean;
  flagged: boolean;
  answered: boolean;
  size: number;
  preview: string;
  hasAttachments: boolean;
}

export interface MailPage {
  messages: MailSummary[];
  total: number;
}

export interface MailCache {
  folders: MailFolder[];
  /** A pasta usada mais recentemente primeiro. */
  lists: { path: string; total: number; messages: MailSummary[] }[];
}

export interface MailAttachment {
  index: number;
  name: string;
  mime: string;
  size: number;
}

export interface MailMessage {
  uid: number;
  subject: string;
  from: MailAddr[];
  to: MailAddr[];
  cc: MailAddr[];
  replyTo: MailAddr[];
  date: number;
  messageId: string;
  references: string[];
  /** HTML já limpo pelo Papeo. */
  html?: string | null;
  /** O HTML pede imagens de fora (bloqueadas até mostrar). */
  hasRemote: boolean;
  text: string;
  attachments: MailAttachment[];
  seen: boolean;
  flagged: boolean;
  /** Sair da lista (`https:` ou `mailto:`), se o e-mail oferece. */
  unsubscribe?: string | null;
}

export type MailFilter = "all" | "unread" | "flagged" | "attachments";

export interface MailDraft {
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  body: string;
  /** Texto com formatação (spec 31): HTML do editor com o marcador da
   * assinatura; `null` = só o texto. */
  bodyHtml?: string | null;
  inReplyTo?: string | null;
  references?: string[];
  replyFolder?: string | null;
  replyUid?: number | null;
  /** Conteúdo em base64. */
  attachments: { name: string; mime: string; data: string }[];
  /** Rascunho no servidor (spec 18). */
  draftToken?: string | null;
  replaces?: { folder: string; uid: number } | null;
}

/** Rascunho da pasta Rascunhos aberto para continuar no editor. */
export interface MailOpenedDraft {
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  body: string;
  bodyHtml: string | null;
  inReplyTo: string | null;
  references: string[];
  attachments: { name: string; mime: string; data: string }[];
  draftToken: string | null;
  replaces: { folder: string; uid: number } | null;
}

export interface MailHold {
  holdId: string;
  secs: number;
}

/** E-mail guardado que ainda não voltou ao editor. */
export interface MailUnsent {
  holdId: string;
  draft: MailDraft;
  /** O app fechou durante o envio: pode ter saído. */
  maybeSent: boolean;
  /** O envio falhou com este erro. */
  error: string | null;
}

/** Fim de um envio em espera: sem `error`, enviado; com erro, o e-mail volta. */
export interface MailSendOutcome {
  holdId: string;
  id: string;
  error: string | null;
  draft: MailDraft | null;
}

export function onMailSend(cb: (o: MailSendOutcome) => void): Promise<UnlistenFn> {
  return listen<MailSendOutcome>("papeo://mail-send", (e) => cb(e.payload));
}

/** Clique num aviso de e-mail de conta com o leitor: procura o assunto. */
/** `uid`: a mensagem na Caixa de Entrada (aviso de e-mail novo), se conhecida. */
export function onMailReveal(
  cb: (r: { id: string; query: string; uid?: number | null }) => void,
): Promise<UnlistenFn> {
  return listen<{ id: string; query: string; uid?: number | null }>("papeo://mail-reveal", (e) => cb(e.payload));
}

export function onTalkLogin(cb: (r: { id: string; ok: boolean; error?: string | null }) => void): Promise<UnlistenFn> {
  return listen<{ id: string; ok: boolean; error?: string | null }>("papeo://talk-login", (e) => cb(e.payload));
}

/** Lista de conversas trazida pelo vigia do Talk (a cada 30–60 s). */
export function onTalkRooms(cb: (r: { id: string; rooms: TalkRoom[] }) => void): Promise<UnlistenFn> {
  return listen<{ id: string; rooms: TalkRoom[] }>("papeo://talk-rooms", (e) => cb(e.payload));
}

/** Clique num aviso do Talk com o leitor: abre a conversa. */
export function onTalkReveal(cb: (r: { id: string; token: string }) => void): Promise<UnlistenFn> {
  return listen<{ id: string; token: string }>("papeo://talk-reveal", (e) => cb(e.payload));
}

export interface TalkRoom {
  token: string;
  name: string;
  /** 1 a dois, 2 grupo, 3 pública, 4 novidades, 5 ex-conversa a dois, 6 anotações. */
  kind: number;
  unread: number;
  mention: boolean;
  favorite: boolean;
  activity: number;
  last?: { author: string; text: string; own: boolean } | null;
  lastId: number;
  readOnly: boolean;
  hasCall: boolean;
  avatarVersion: string;
}

export interface TalkFile {
  id: string;
  name: string;
  path: string;
  mime: string;
  size: number;
  preview: boolean;
}

export interface TalkMessage {
  id: number;
  author: string;
  authorId: string;
  own: boolean;
  time: number;
  text: string;
  system: boolean;
  files: TalkFile[];
  parent?: { id: number; author: string; text: string } | null;
  reactions: { emoji: string; count: number; mine: boolean }[];
  replyable: boolean;
}

export interface TalkChunk {
  messages: TalkMessage[];
  last: number;
  first: number;
  more: boolean;
}

export interface TalkMention {
  insert: string;
  label: string;
}

export function onSync(cb: (s: SyncStatus) => void): Promise<UnlistenFn> {
  return listen<SyncStatus>("papeo://sync", (e) => cb(e.payload));
}

export function onOrganizer(cb: (o: Organizer) => void): Promise<UnlistenFn> {
  return listen<Organizer>("papeo://organizer", (e) => cb(e.payload));
}

/** O estado recebido é o atual ou mais novo (não uma resposta atrasada)? */
export function fresher(next: Snapshot, current: Snapshot): Snapshot {
  return next.rev >= current.rev ? next : current;
}

export function onState(cb: (s: Snapshot) => void): Promise<UnlistenFn> {
  return listen<Snapshot>("papeo://state", (e) => cb(e.payload));
}

export function onCommand(cb: (cmd: string) => void): Promise<UnlistenFn> {
  return listen<string>("papeo://command", (e) => cb(e.payload));
}

export function onDownload(
  cb: (d: { id: number; fileName: string; success: boolean }) => void,
): Promise<UnlistenFn> {
  return listen<{ id: number; fileName: string; success: boolean }>("papeo://download", (e) =>
    cb(e.payload),
  );
}

/** Texto curto da situação da atualização. */
export function updateLabel(update: UpdateStatus): string {
  switch (update.state) {
    case "checking":
      return t("Verificando…");
    case "upToDate":
      return t("Você está na versão mais recente.");
    case "available":
      return t("Versão {versao} disponível.", { versao: update.version });
    case "installing":
      return update.percent != null
        ? t("Baixando atualização… {pct}%", { pct: update.percent })
        : t("Baixando atualização…");
    case "failed":
      return update.message;
    default:
      return "";
  }
}

export const isMac = navigator.userAgent.includes("Mac");
export const isWindows = navigator.userAgent.includes("Windows");
export const PIN_MIN = 4;
export const PIN_MAX = 64;
export const modKey = isMac ? "⌘" : "Ctrl";

// "Com redução do macOS" só existe no Mac (no Windows, a redução do sistema
// já é o padrão e começa na hora).
export const VOICE_OPTIONS: { value: VoiceCapture; label: string; description: string }[] = isMac
  ? [
      {
        value: "instant",
        label: tk("Instantânea"),
        description: tk("Grava no toque, com o som do microfone como ele chega, sem tratamento."),
      },
      {
        value: "enhanced",
        label: tk("Instantânea com limpeza do Papeo"),
        description: tk("Grava no toque e tira o ruído de fundo (ventilador, teclado, rua), deixando a voz num volume constante."),
      },
      {
        value: "clean",
        label: tk("Redução do macOS"),
        description: tk("O macOS tira eco e ruído, mas o começo da gravação pode atrasar cerca de 1 segundo."),
      },
    ]
  : [
      {
        value: "instant",
        label: tk("Padrão do Windows"),
        description: tk("Grava no toque, com a redução de eco e ruído de sempre do Windows."),
      },
      {
        value: "enhanced",
        label: tk("Com limpeza do Papeo"),
        description: tk("Grava no toque e tira mais ruído de fundo (ventilador, teclado, rua), deixando a voz num volume constante."),
      },
    ];

export const KIND_META: Record<ServiceKind, { label: string; color: string; hint: string }> = {
  whatsapp: { label: "WhatsApp", color: "#25d366", hint: "web.whatsapp.com" },
  telegram: { label: "Telegram", color: "#2aabee", hint: "web.telegram.org" },
  discord: { label: "Discord", color: "#5865f2", hint: "discord.com" },
  gmail: { label: "Gmail", color: "#ea4335", hint: "mail.google.com" },
  outlook: { label: "Outlook", color: "#0a64b8", hint: tk("Pessoal ou Microsoft 365") },
  email: { label: tk("Outro e-mail"), color: "#0f766e", hint: tk("Webmail (Roundcube e outros)") },
  custom: { label: tk("Outro site"), color: "#8b5cf6", hint: tk("Qualquer endereço https") },
  slack: { label: "Slack", color: "#611f69", hint: "app.slack.com" },
  teams: { label: "Teams", color: "#4b53bc", hint: tk("Trabalho ou pessoal") },
  nextcloud: { label: "Nextcloud Talk", color: "#0082c9", hint: tk("No servidor da sua empresa") },
};

/** Paleta para diferenciar contas; tons escuros o bastante para ícone branco. */
export const SERVICE_COLORS = [
  { value: "#16a34a", label: tk("Verde") },
  { value: "#0ea5e9", label: tk("Azul-claro") },
  { value: "#2563eb", label: tk("Azul") },
  { value: "#7c3aed", label: tk("Roxo") },
  { value: "#db2777", label: tk("Rosa") },
  { value: "#dc2626", label: tk("Vermelho") },
  { value: "#ea580c", label: tk("Laranja") },
  { value: "#ca8a04", label: tk("Amarelo") },
  { value: "#0d9488", label: tk("Verde-água") },
  { value: "#475569", label: tk("Cinza") },
];

export const EMOJI_SUGGESTIONS = ["🏠", "💼", "❤️", "👨‍👩‍👧", "🎓", "🛒", "⭐", "🔒", "⚽", "🚀"];

export function serviceColor(service: Pick<ServiceView, "kind" | "color">): string {
  return service.color ?? KIND_META[service.kind].color;
}

export function mediaPolicy(service: Pick<ServiceView, "kind" | "media">): MediaPolicy {
  return (
    service.media ??
    (isMessenger(service) || ["discord", "slack", "teams", "nextcloud"].includes(service.kind) ? "allow" : "ask")
  );
}

/** WhatsApp e Telegram: conversas que dá para abrir e responder. */
export function isMessenger(service: Pick<ServiceView, "kind">): boolean {
  return service.kind === "whatsapp" || service.kind === "telegram";
}

/** Caixas de e-mail: o assunto aberto faz as vezes da conversa. */
export function isEmail(service: Pick<ServiceView, "kind">): boolean {
  return service.kind === "gmail" || service.kind === "outlook" || service.kind === "email";
}

/** Tipos em que a pessoa informa o endereço. */
export function needsUrl(kind: ServiceKind): boolean {
  return kind === "custom" || kind === "email" || kind === "nextcloud";
}

/** Outlook: endereço de quem entra com conta de trabalho ou escola. */
export const OUTLOOK_WORK_URL = "https://outlook.office.com/mail/";

/** Teams: endereço de quem entra com conta pessoal (o padrão é o de
 * trabalho ou escola). */
export const TEAMS_PERSONAL_URL = "https://teams.live.com/v2/";

/** Tipos cujo endereço muda de uma conta para outra (vale ao duplicar). */
export function keepsUrl(kind: ServiceKind): boolean {
  return needsUrl(kind) || kind === "outlook" || kind === "teams";
}

export const MEDIA_OPTIONS: { value: MediaPolicy; label: string }[] = [
  { value: "allow", label: tk("Permitir") },
  { value: "ask", label: tk("Perguntar") },
  { value: "block", label: tk("Bloquear") },
];

export const MODE_LABEL: Record<Mode, string> = {
  all: tk("Todas"),
  work: tk("Trabalho"),
  personal: tk("Pessoal"),
};

export const CONTEXT_OPTIONS: { value: ServiceContext | null; label: string }[] = [
  { value: "work", label: tk("Trabalho") },
  { value: "personal", label: tk("Pessoal") },
  { value: null, label: tk("Os dois") },
];

export const MODE_CHOICE_OPTIONS: { value: ModeChoice; label: string }[] = [
  { value: "all", label: tk("Todas as contas") },
  { value: "work", label: tk("Só trabalho") },
  { value: "personal", label: tk("Só pessoal") },
  { value: "auto", label: tk("Pelo expediente") },
];

/** Separa trabalho e pessoal: alguma conta marcada ou modo escolhido. */
export function usesModes(snap: Pick<Snapshot, "services" | "settings">): boolean {
  return snap.settings.mode !== "all" || snap.services.some((s) => s.context);
}

/** Troca rápida: trabalho ↔ pessoal (de "todas", vai para trabalho). */
export function nextMode(mode: Mode): Mode {
  return mode === "work" ? "personal" : "work";
}

export const THEME_OPTIONS: { value: Appearance; label: string }[] = [
  { value: "system", label: tk("Sistema") },
  { value: "light", label: tk("Claro") },
  { value: "dark", label: tk("Escuro") },
  { value: "oled", label: "OLED" },
];

export const PREVIEW_OPTIONS: { value: NotificationPreview; label: string }[] = [
  { value: "full", label: tk("Completa") },
  { value: "sender", label: tk("Só remetente") },
  { value: "hidden", label: tk("Oculta") },
];

export const LOCK_OPTIONS = [
  { minutes: 0, label: tk("Ao esconder a janela") },
  { minutes: 1, label: tk("1 min sem usar") },
  { minutes: 5, label: tk("5 min sem usar") },
  { minutes: 15, label: tk("15 min sem usar") },
  { minutes: 60, label: tk("1 hora sem usar") },
];

/** Site personalizado sem criptografia (http). */
export function isInsecure(service: Pick<ServiceView, "url">): boolean {
  return service.url.startsWith("http://");
}

/** Serviços em que a busca de conversas funciona (WhatsApp e Telegram). */
export function searchable(service: Pick<ServiceView, "kind">): boolean {
  return isMessenger(service);
}

/** Créditos e links mostrados em Configurações → Sobre. */
export const ABOUT = {
  author: "Ygor Moura",
  profile: "https://github.com/ygormteus",
  repo: "https://github.com/ygormteus/papeo",
  issues: "https://github.com/ygormteus/papeo/issues",
};

export const RELOAD_OPTIONS = [
  { mb: 0, label: tk("Nunca") },
  { mb: 1536, label: tk("Acima de 1,5 GB") },
  { mb: 2048, label: tk("Acima de 2 GB") },
  { mb: 3072, label: tk("Acima de 3 GB") },
  { mb: 4096, label: tk("Acima de 4 GB") },
];

/** Espera para desfazer o envio de e-mail (spec 16). */
export const UNDO_OPTIONS = [
  { secs: 0, label: tk("Desligado") },
  { secs: 5, label: tk("5 segundos") },
  { secs: 10, label: tk("10 segundos") },
  { secs: 20, label: tk("20 segundos") },
];

export const HIBERNATE_OPTIONS = [
  { minutes: 0, label: tk("Nunca") },
  { minutes: 15, label: tk("15 min") },
  { minutes: 30, label: tk("30 min") },
  { minutes: 60, label: tk("1 hora") },
  { minutes: 120, label: tk("2 horas") },
];

/** Notificações deste serviço estão silenciadas agora (toggle ou temporário). */
export function isMuted(service: Pick<ServiceView, "notifications" | "mutedUntil">): boolean {
  return !service.notifications || (service.mutedUntil ?? 0) > Date.now() / 1000;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) {
    const gb = (bytes / 1024 ** 3).toFixed(1);
    return `${locale() === "pt-BR" ? gb.replace(".", ",") : gb} GB`;
  }
  return `${Math.round(bytes / 1024 ** 2)} MB`;
}

export function formatClock(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

export function parseClock(value: string): number | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value);
  if (!match) return null;
  const h = Number(match[1]);
  const m = Number(match[2]);
  return h < 24 && m < 60 ? h * 60 + m : null;
}

/** "até 14:30", "até reativar"… para o não perturbe e silêncio temporário. */
export function untilLabel(untilSecs: number | null | undefined): string {
  if (!untilSecs) return "";
  if (untilSecs >= Number.MAX_SAFE_INTEGER / 1000) return t("até reativar");
  const d = new Date(untilSecs * 1000);
  return t("até {hora}", { hora: `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}` });
}

export const STATUS_LABEL: Record<ServiceStatus, string> = {
  ok: tk("Em memória"),
  sleeping: tk("Dormindo"),
  loggedOut: tk("Desconectado"),
  unresponsive: tk("Não responde"),
};

export function errorMessage(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : t("Algo deu errado.");
}

export type MenuItem = {
  label: string;
  icon: Component<{ size?: number }>;
  run: () => void;
  danger?: boolean;
};
