import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Component } from "svelte";

export type ServiceKind = "whatsapp" | "telegram" | "custom";
export type MediaPolicy = "ask" | "allow" | "block";
export type Appearance = "system" | "light" | "dark" | "oled";
/** Conteúdo nas notificações: completo · só remetente · oculto. */
export type NotificationPreview = "full" | "sender" | "hidden";
/** ok · dormindo (hibernado) · desconectado (tela de login) · não responde */
export type ServiceStatus = "ok" | "sleeping" | "loggedOut" | "unresponsive";

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
}

export interface QuietHours {
  enabled: boolean;
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
}

export type UpdateStatus =
  | { state: "idle" | "checking" | "upToDate" }
  | { state: "available"; version: string; notes?: string | null }
  | { state: "installing"; percent?: number | null }
  | { state: "failed"; message: string };

export interface DownloadEntry {
  id: number;
  serviceId: string;
  fileName: string;
  state: "downloading" | "done" | "failed";
  at: number;
}

export interface Snapshot {
  services: ServiceView[];
  active: string | null;
  totalUnread: number;
  settings: Settings;
  dndActive: boolean;
  autostart: boolean;
  downloads: DownloadEntry[];
  downloadsDir: string;
  /** Bloqueado: só a tela de desbloqueio aparece. */
  locked: boolean;
  /** Touch ID/senha do Mac ou Windows Hello disponíveis. */
  lockAvailable: boolean;
  update: UpdateStatus;
  appVersion: string;
  /** Conversas com mensagens recentes, mais nova primeiro. Só na memória;
   * vazia com o app bloqueado ou a prévia oculta. */
  recent: RecentChat[];
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
  },
  dndActive: false,
  autostart: false,
  downloads: [],
  downloadsDir: "",
  locked: false,
  lockAvailable: false,
  update: { state: "idle" },
  appVersion: "",
  recent: [],
};

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  addService: (kind: ServiceKind, name?: string, url?: string) =>
    invoke<Snapshot>("add_service", { kind, name: name ?? null, url: url ?? null }),
  removeService: (id: string) => invoke<Snapshot>("remove_service", { id }),
  customizeService: (id: string, name: string, color: string | null, emoji: string | null) =>
    invoke<Snapshot>("customize_service", { id, name, color, emoji }),
  moveService: (id: string, to: number) => invoke<Snapshot>("move_service", { id, to }),
  setHibernation: (id: string, minutes: number) =>
    invoke<Snapshot>("set_hibernation", { id, minutes }),
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
  /** `null` desliga; 0 = até reativar; n = por n minutos. */
  setDnd: (minutes: number | null) => invoke<Snapshot>("set_dnd", { minutes }),
  setQuietHours: (quietHours: QuietHours) => invoke<Snapshot>("set_quiet_hours", { quietHours }),
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
      >
    >,
  ) => invoke<Snapshot>("set_preferences", { prefs }),
  setAutostart: (enabled: boolean) => invoke<Snapshot>("set_autostart", { enabled }),
  setDownloadsDir: (path: string | null) => invoke<Snapshot>("set_downloads_dir", { path }),
  revealDownload: (id: number) => invoke<void>("reveal_download", { id }),
  openDownloadsDir: () => invoke<void>("open_downloads_dir"),
  clearDownloads: () => invoke<Snapshot>("clear_downloads"),
  getMemory: () => invoke<MemoryReport>("get_memory"),
  /** Abre no navegador padrão (plugin opener; só http/https). */
  openUrl: (url: string) => invoke<void>("plugin:opener|open_url", { url }),
  exportConfig: (path: string) => invoke<void>("export_config", { path }),
  previewBackup: (path: string) => invoke<{ services: string[] }>("preview_backup", { path }),
  importConfig: (path: string) => invoke<Snapshot>("import_config", { path }),
  /** Sai da conta: troca a sessão e apaga cookies, cache e conversas guardadas. */
  clearServiceData: (id: string) => invoke<Snapshot>("clear_service_data", { id }),
  getOrphanSessions: () => invoke<number>("get_orphan_sessions"),
  cleanOrphanSessions: () => invoke<number>("clean_orphan_sessions"),
  setLock: (enabled: boolean, afterMinutes: number) =>
    invoke<Snapshot>("set_lock", { enabled, afterMinutes }),
  lockNow: () => invoke<Snapshot>("lock_now"),
  unlock: () => invoke<Snapshot>("unlock"),
  setAppearance: (appearance: Appearance) => invoke<Snapshot>("set_appearance", { appearance }),
  checkUpdate: () => invoke<Snapshot>("check_update"),
  /** Baixa, instala e reinicia o app. */
  installUpdate: () => invoke<Snapshot>("install_update"),
  /** Liga/desliga a verificação automática e marca o aviso como visto. */
  setUpdates: (check: boolean) => invoke<Snapshot>("set_updates", { check }),
};

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
      return "Verificando…";
    case "upToDate":
      return "Você está na versão mais recente.";
    case "available":
      return `Versão ${update.version} disponível.`;
    case "installing":
      return update.percent != null
        ? `Baixando atualização… ${update.percent}%`
        : "Baixando atualização…";
    case "failed":
      return update.message;
    default:
      return "";
  }
}

export const isMac = navigator.userAgent.includes("Mac");
export const modKey = isMac ? "⌘" : "Ctrl";

export const KIND_META: Record<ServiceKind, { label: string; color: string; hint: string }> = {
  whatsapp: { label: "WhatsApp", color: "#25d366", hint: "web.whatsapp.com" },
  telegram: { label: "Telegram", color: "#2aabee", hint: "web.telegram.org" },
  custom: { label: "Outro site", color: "#8b5cf6", hint: "Qualquer endereço https" },
};

/** Paleta para diferenciar contas; tons escuros o bastante para ícone branco. */
export const SERVICE_COLORS = [
  { value: "#16a34a", label: "Verde" },
  { value: "#0ea5e9", label: "Azul-claro" },
  { value: "#2563eb", label: "Azul" },
  { value: "#7c3aed", label: "Roxo" },
  { value: "#db2777", label: "Rosa" },
  { value: "#dc2626", label: "Vermelho" },
  { value: "#ea580c", label: "Laranja" },
  { value: "#ca8a04", label: "Amarelo" },
  { value: "#0d9488", label: "Verde-água" },
  { value: "#475569", label: "Cinza" },
];

export const EMOJI_SUGGESTIONS = ["🏠", "💼", "❤️", "👨‍👩‍👧", "🎓", "🛒", "⭐", "🔒", "⚽", "🚀"];

export function serviceColor(service: Pick<ServiceView, "kind" | "color">): string {
  return service.color ?? KIND_META[service.kind].color;
}

export function mediaPolicy(service: Pick<ServiceView, "kind" | "media">): MediaPolicy {
  return service.media ?? (service.kind === "custom" ? "ask" : "allow");
}

export const MEDIA_OPTIONS: { value: MediaPolicy; label: string }[] = [
  { value: "allow", label: "Permitir" },
  { value: "ask", label: "Perguntar" },
  { value: "block", label: "Bloquear" },
];

export const THEME_OPTIONS: { value: Appearance; label: string }[] = [
  { value: "system", label: "Sistema" },
  { value: "light", label: "Claro" },
  { value: "dark", label: "Escuro" },
  { value: "oled", label: "OLED" },
];

export const PREVIEW_OPTIONS: { value: NotificationPreview; label: string }[] = [
  { value: "full", label: "Completa" },
  { value: "sender", label: "Só remetente" },
  { value: "hidden", label: "Oculta" },
];

export const LOCK_OPTIONS = [
  { minutes: 0, label: "Ao esconder a janela" },
  { minutes: 1, label: "1 min sem usar" },
  { minutes: 5, label: "5 min sem usar" },
  { minutes: 15, label: "15 min sem usar" },
  { minutes: 60, label: "1 hora sem usar" },
];

/** Site personalizado sem criptografia (http). */
export function isInsecure(service: Pick<ServiceView, "url">): boolean {
  return service.url.startsWith("http://");
}

/** Serviços em que a busca de conversas funciona (WhatsApp e Telegram). */
export function searchable(service: Pick<ServiceView, "kind">): boolean {
  return service.kind === "whatsapp" || service.kind === "telegram";
}

/** Créditos e links mostrados em Configurações → Sobre. */
export const ABOUT = {
  author: "Ygor Moura",
  profile: "https://github.com/ygormteus",
  repo: "https://github.com/ygormteus/papeo",
  issues: "https://github.com/ygormteus/papeo/issues",
};

export const RELOAD_OPTIONS = [
  { mb: 0, label: "Nunca" },
  { mb: 1536, label: "Acima de 1,5 GB" },
  { mb: 2048, label: "Acima de 2 GB" },
  { mb: 3072, label: "Acima de 3 GB" },
  { mb: 4096, label: "Acima de 4 GB" },
];

export const HIBERNATE_OPTIONS = [
  { minutes: 0, label: "Nunca" },
  { minutes: 15, label: "15 min" },
  { minutes: 30, label: "30 min" },
  { minutes: 60, label: "1 hora" },
  { minutes: 120, label: "2 horas" },
];

/** Notificações deste serviço estão silenciadas agora (toggle ou temporário). */
export function isMuted(service: Pick<ServiceView, "notifications" | "mutedUntil">): boolean {
  return !service.notifications || (service.mutedUntil ?? 0) > Date.now() / 1000;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1).replace(".", ",")} GB`;
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
  if (untilSecs >= Number.MAX_SAFE_INTEGER / 1000) return "até reativar";
  const d = new Date(untilSecs * 1000);
  return `até ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

export const STATUS_LABEL: Record<ServiceStatus, string> = {
  ok: "Em memória",
  sleeping: "Dormindo",
  loggedOut: "Desconectado",
  unresponsive: "Não responde",
};

export function errorMessage(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : "Algo deu errado.";
}

export type MenuItem = {
  label: string;
  icon: Component<{ size?: number }>;
  run: () => void;
  danger?: boolean;
};
