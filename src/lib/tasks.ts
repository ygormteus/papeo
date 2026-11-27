// Regras de data das tarefas (hora local), sem dependências: usadas pela
// interface e testadas em `tests/tasks.test.mjs`.
import type { Contact, Organizer, Task } from "./api";

// Este módulo roda também no Node (testes), onde `i18n.svelte.ts` não carrega:
// o tradutor é injetado pelo módulo da API (`setTranslator`); sem ele, português.
type Vars = Record<string, string | number>;
let translate = (pt: string, vars?: Vars): string =>
  vars ? pt.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : pt;
let localeTag = () => "pt-BR";
export function setTranslator(fn: typeof translate, locale: () => string) {
  translate = fn;
  localeTag = locale;
}
const t = (pt: string, vars?: Vars) => translate(pt, vars);
/** Marca a frase para o catálogo; quem exibe chama `t(valor)`. */
const tk = (pt: string): string => pt;

export type Group = "overdue" | "today" | "upcoming" | "someday" | "done";

export const GROUP_LABEL: Record<Group, string> = {
  overdue: tk("Atrasadas"),
  today: tk("Hoje"),
  upcoming: tk("Próximas"),
  someday: tk("Sem data"),
  done: tk("Feitas"),
};

const pad = (n: number) => String(n).padStart(2, "0");

/** `AAAA-MM-DD` na hora local. */
export function dateKey(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Meia-noite local do dia `AAAA-MM-DD`. */
export function fromDateKey(key: string): Date {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

export function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes());
}

const secs = (d: Date) => Math.floor(d.getTime() / 1000);

/** Mesmas regras do backend (`Organizer::counts`). */
export function groupOf(task: Task, now: Date): Group {
  if (task.done) return "done";
  const today = dateKey(now);
  const start = secs(startOfDay(now));
  const end = secs(addDays(startOfDay(now), 1));
  const late = (task.due != null && task.due < today) || (task.remindAt != null && task.remindAt < start);
  if (late) return "overdue";
  const isToday = task.due === today || (task.remindAt != null && task.remindAt >= start && task.remindAt < end);
  if (isToday) return "today";
  if (task.due != null || task.remindAt != null) return "upcoming";
  return "someday";
}

/** Momento que ordena a tarefa (lembrete, senão fim do dia em que vence). */
function sortKey(task: Task): number {
  if (task.remindAt != null) return task.remindAt;
  if (task.due) return secs(addDays(fromDateKey(task.due), 1)) - 1;
  return Number.MAX_SAFE_INTEGER;
}

export function sortTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) =>
    a.done && b.done
      ? (b.doneAt ?? 0) - (a.doneAt ?? 0)
      : sortKey(a) - sortKey(b) || a.createdAt - b.createdAt,
  );
}

export function groupTasks(tasks: Task[], now: Date): { group: Group; tasks: Task[] }[] {
  const order: Group[] = ["overdue", "today", "upcoming", "someday", "done"];
  const by = new Map<Group, Task[]>(order.map((g) => [g, []]));
  for (const t of sortTasks(tasks)) by.get(groupOf(t, now))!.push(t);
  return order.map((group) => ({ group, tasks: by.get(group)! })).filter((g) => g.tasks.length);
}

const WEEKDAY = [tk("dom."), tk("seg."), tk("ter."), tk("qua."), tk("qui."), tk("sex."), tk("sáb.")];
const MONTH = [
  tk("jan."),
  tk("fev."),
  tk("mar."),
  tk("abr."),
  tk("mai."),
  tk("jun."),
  tk("jul."),
  tk("ago."),
  tk("set."),
  tk("out."),
  tk("nov."),
  tk("dez."),
];

/** "Hoje", "Amanhã", "Ontem", "qui., 3 dez." ou "3 dez. 2027". */
export function dayLabel(d: Date, now: Date): string {
  const diff = Math.round((startOfDay(d).getTime() - startOfDay(now).getTime()) / 86_400_000);
  if (diff === 0) return t("Hoje");
  if (diff === 1) return t("Amanhã");
  if (diff === -1) return t("Ontem");
  const base = t("{dia} {mes}", { dia: d.getDate(), mes: t(MONTH[d.getMonth()]) });
  if (d.getFullYear() !== now.getFullYear()) return t("{data} {ano}", { data: base, ano: d.getFullYear() });
  if (diff > 1 && diff < 7) return t("{semana}, {data}", { semana: t(WEEKDAY[d.getDay()]), data: base });
  return base;
}

export function timeLabel(d: Date): string {
  if (localeTag() !== "pt-BR") {
    const h = d.getHours() % 12 || 12;
    return `${h}:${pad(d.getMinutes())} ${d.getHours() < 12 ? "AM" : "PM"}`;
  }
  return `${d.getHours()}:${pad(d.getMinutes())}`;
}

/** Quando a tarefa avisa ou vence, para exibir ("Hoje 15:30", "Amanhã"). */
export function whenLabel(task: Task, now: Date): string | null {
  if (task.remindAt != null) {
    const d = new Date(task.remindAt * 1000);
    return `${dayLabel(d, now)} ${timeLabel(d)}`;
  }
  if (task.due) return dayLabel(fromDateKey(task.due), now);
  return null;
}

export interface QuickTime {
  label: string;
  /** Segundos Unix. */
  at: number;
}

/** Atalhos de hora: em 1 h, hoje 18h (se ainda der), amanhã 9h, segunda 9h. */
export function quickTimes(now: Date): QuickTime[] {
  const at = (d: Date) => secs(d);
  const inHour = new Date(now.getTime() + 60 * 60 * 1000);
  inHour.setSeconds(0, 0);
  const list: QuickTime[] = [{ label: t("Em 1 h"), at: at(inHour) }];
  const evening = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 18, 0);
  if (evening.getTime() - now.getTime() > 30 * 60 * 1000) list.push({ label: t("Hoje 18h"), at: at(evening) });
  const tomorrow = addDays(startOfDay(now), 1);
  tomorrow.setHours(9, 0);
  list.push({ label: t("Amanhã 9h"), at: at(tomorrow) });
  // Próxima segunda, a partir de depois de amanhã (não repete o "amanhã").
  let monday = addDays(startOfDay(now), 2);
  while (monday.getDay() !== 1) monday = addDays(monday, 1);
  monday.setHours(9, 0);
  list.push({ label: t("Segunda 9h"), at: at(monday) });
  return list;
}

/** Expediente: dias (0 = domingo) e minutos do dia de início e fim. */
export interface Hours {
  days: number[];
  start: number;
  end: number;
}

/** Próximo começo de expediente depois de `now` (null sem expediente). */
export function nextWorkStart(now: Date, hours: Hours): Date | null {
  if (hours.start === hours.end || !hours.days.length) return null;
  for (let i = 0; i <= 7; i++) {
    const d = addDays(startOfDay(now), i);
    if (!hours.days.includes(d.getDay())) continue;
    d.setHours(Math.floor(hours.start / 60), hours.start % 60, 0, 0);
    if (d.getTime() > now.getTime()) return d;
  }
  return null;
}

/** "Responder depois": atalhos de hora mais o próximo expediente. */
export function laterTimes(now: Date, hours?: Hours): QuickTime[] {
  const list = quickTimes(now);
  const work = hours ? nextWorkStart(now, hours) : null;
  if (work) {
    const at = secs(work);
    if (!list.some((q) => q.at === at)) {
      list.push({ label: t("Expediente · {quando}", { quando: `${dayLabel(work, now)} ${timeLabel(work)}` }), at });
    }
  }
  return list.sort((a, b) => a.at - b.at);
}

/** "HH:MM" de um instante, para `<input type="time">`. */
export function toTimeInput(at: number): string {
  const d = new Date(at * 1000);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Junta `AAAA-MM-DD` e "HH:MM" (hora local) em segundos Unix. */
export function fromInputs(date: string, time: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || !/^\d{1,2}:\d{2}$/.test(time)) return null;
  const [h, m] = time.split(":").map(Number);
  const d = fromDateKey(date);
  d.setHours(h, m, 0, 0);
  const at = secs(d);
  return Number.isFinite(at) ? at : null;
}

/** Nome para exibir do contato: "Ana · Pessoal" ou só a conta. */
export function contactLabel(contact: Contact, serviceName?: string): string {
  const account = serviceName ?? contact.serviceName;
  return contact.chat ? `${contact.chat} · ${account}` : account;
}

export function contactOf(org: Organizer, task: Task): Contact | undefined {
  return task.contact ? org.contacts.find((c) => c.id === task.contact) : undefined;
}

/** Tarefas ligadas a uma conversa (conta + nome; `chat` vazio = conta). */
export function tasksFor(org: Organizer, serviceId: string, chat: string): Task[] {
  const ids = new Set(
    org.contacts.filter((c) => c.serviceId === serviceId && c.chat === chat).map((c) => c.id),
  );
  return org.tasks.filter((t) => t.contact && ids.has(t.contact));
}

export function contactFor(org: Organizer, serviceId: string, chat: string): Contact | undefined {
  return org.contacts.find((c) => c.serviceId === serviceId && c.chat === chat);
}
