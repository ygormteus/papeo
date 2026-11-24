import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Component } from "svelte";

export type ServiceKind = "whatsapp" | "telegram" | "custom";

export interface ServiceView {
  id: string;
  kind: ServiceKind;
  name: string;
  url: string;
  session: string;
  hibernateAfterMinutes: number;
  /** Repassa as notificações do site como notificações nativas. */
  notifications: boolean;
  userAgent?: string;
  /** A webview do serviço está em memória (não hibernado). */
  loaded: boolean;
  unread: number;
}

export interface Snapshot {
  services: ServiceView[];
  active: string | null;
  totalUnread: number;
}

export interface Area {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  addService: (kind: ServiceKind, name?: string, url?: string) =>
    invoke<Snapshot>("add_service", { kind, name: name ?? null, url: url ?? null }),
  removeService: (id: string) => invoke<Snapshot>("remove_service", { id }),
  renameService: (id: string, name: string) => invoke<Snapshot>("rename_service", { id, name }),
  moveService: (id: string, to: number) => invoke<Snapshot>("move_service", { id, to }),
  setHibernation: (id: string, minutes: number) =>
    invoke<Snapshot>("set_hibernation", { id, minutes }),
  setNotifications: (id: string, enabled: boolean) =>
    invoke<Snapshot>("set_notifications", { id, enabled }),
  activate: (id: string | null) => invoke<Snapshot>("activate_service", { id }),
  hibernate: (id: string) => invoke<Snapshot>("hibernate_service", { id }),
  reload: (id: string) => invoke<void>("reload_service", { id }),
  setContentArea: (area: Area) => invoke<void>("set_content_area", { area }),
  setOverlay: (open: boolean) => invoke<void>("set_overlay", { open }),
};

export function onState(cb: (s: Snapshot) => void): Promise<UnlistenFn> {
  return listen<Snapshot>("papeo://state", (e) => cb(e.payload));
}

export function onCommand(cb: (cmd: string) => void): Promise<UnlistenFn> {
  return listen<string>("papeo://command", (e) => cb(e.payload));
}

export const isMac = navigator.userAgent.includes("Mac");
export const modKey = isMac ? "⌘" : "Ctrl";

export const KIND_META: Record<ServiceKind, { label: string; color: string; hint: string }> = {
  whatsapp: { label: "WhatsApp", color: "#25d366", hint: "web.whatsapp.com" },
  telegram: { label: "Telegram", color: "#2aabee", hint: "web.telegram.org" },
  custom: { label: "Outro site", color: "#8b5cf6", hint: "Qualquer endereço https" },
};

export const HIBERNATE_OPTIONS = [
  { minutes: 0, label: "Nunca" },
  { minutes: 15, label: "15 min" },
  { minutes: 30, label: "30 min" },
  { minutes: 60, label: "1 hora" },
  { minutes: 120, label: "2 horas" },
];

export function errorMessage(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : "Algo deu errado.";
}

export type MenuItem = {
  label: string;
  icon: Component<{ size?: number }>;
  run: () => void;
  danger?: boolean;
};
