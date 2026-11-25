// Lê datas escritas no fim do texto da tarefa ("ligar pra Ana amanhã 15h")
// e devolve o texto sem elas e quando avisar. Hora local. Testado em
// `tests/when.test.mjs`.

export interface ParsedWhen {
  /** Texto sem a data. */
  text: string;
  /** Quando avisar (segundos Unix), se veio hora. */
  remindAt: number | null;
  /** Dia (`AAAA-MM-DD`), se veio só o dia. */
  due: string | null;
  /** Trecho reconhecido, para mostrar. */
  match: string;
}

const pad = (n: number) => String(n).padStart(2, "0");
const key = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const strip = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

const WEEKDAYS: Record<string, number> = {
  domingo: 0,
  segunda: 1,
  "segunda-feira": 1,
  terca: 2,
  "terca-feira": 2,
  quarta: 3,
  "quarta-feira": 3,
  quinta: 4,
  "quinta-feira": 4,
  sexta: 5,
  "sexta-feira": 5,
  sabado: 6,
};

// Hora: "15h", "15h30", "15:30", "às 9", "as 9h", "9 da manha", "3 da tarde".
const TIME = String.raw`(?:(?:as|a)\s+)?(\d{1,2})(?:(?:h|:)(\d{2})?|h)?(?:\s*(?:da|de)\s+(manha|tarde|noite))?`;
// Dia: hoje, amanhã, depois de amanhã, (na) segunda(-feira), dia 10, 10/12.
const DAY = String.raw`(hoje|amanha|depois de amanha|(?:na |no |proxima |proximo )?(?:segunda|terca|quarta|quinta|sexta)(?:-feira)?|(?:no |proximo )?(?:sabado|domingo)|dia\s+\d{1,2}(?:\/\d{1,2})?|\d{1,2}\/\d{1,2}(?:\/\d{2,4})?)`;

// Palavras antes de "3/12" que indicam número, não data.
const NOT_DATE = /(?:^|\s)(?:parcela|parcelas|prestacao|parte|cap|capitulo|item|nota|questao|pagina|pag|vol|numero|n|no|nº)\s*$/;

/** Hora escrita de um jeito que só pode ser hora: "às 9", "9h", "9:30", "3 da tarde". */
function explicitTime(raw: string, period: string | undefined): boolean {
  return Boolean(period) || /^as\s/.test(raw) || /\d\s*(h|:)/.test(raw);
}

const PATTERNS: RegExp[] = [
  // "em 2 horas", "daqui a 30 min"
  new RegExp(String.raw`\s+(?:em|daqui a)\s+(\d{1,3})\s*(min|minutos?|h|horas?)$`),
  // dia + hora ("amanhã 15h", "sexta às 9")
  new RegExp(String.raw`\s+${DAY}\s+${TIME}$`),
  // hora + dia ("15h amanhã")
  new RegExp(String.raw`\s+${TIME}\s+${DAY}$`),
  // só o dia ("amanhã", "dia 10")
  new RegExp(String.raw`\s+${DAY}$`),
  // só a hora com "às" ou "h" ("às 15h", "15h30")
  new RegExp(String.raw`\s+((?:as|a)\s+\d{1,2}(?:(?:h|:)\d{2}|h)?|\d{1,2}(?:h\d{2}|h|:\d{2}))(?:\s*(?:da|de)\s+(manha|tarde|noite))?$`),
];

function dayFrom(word: string, now: Date): Date | null {
  const base = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const w = word.replace(/^(na|no|proxima|proximo)\s+/, "");
  if (w === "hoje") return base;
  if (w === "amanha") return new Date(base.getFullYear(), base.getMonth(), base.getDate() + 1);
  if (w === "depois de amanha") return new Date(base.getFullYear(), base.getMonth(), base.getDate() + 2);
  if (w in WEEKDAYS) {
    const target = WEEKDAYS[w];
    let diff = (target - base.getDay() + 7) % 7;
    if (diff === 0) diff = 7;
    return new Date(base.getFullYear(), base.getMonth(), base.getDate() + diff);
  }
  const m = w.match(/^(?:dia\s+)?(\d{1,2})(?:\/(\d{1,2})(?:\/(\d{2,4}))?)?$/);
  if (m) {
    const day = Number(m[1]);
    let month = m[2] ? Number(m[2]) - 1 : base.getMonth();
    let year = m[3] ? Number(m[3].length === 2 ? `20${m[3]}` : m[3]) : base.getFullYear();
    if (day < 1 || day > 31 || month < 0 || month > 11) return null;
    let d = new Date(year, month, day);
    if (d.getDate() !== day) return null;
    // Sem mês: "dia 3" já passou → mês que vem. Sem ano: data passada → ano que vem.
    if (d < base && !m[3]) {
      if (m[2]) year += 1;
      else month += 1;
      d = new Date(year, month, day);
      if (d.getDate() !== day) return null;
    }
    return d;
  }
  return null;
}

function hourFrom(h: string, m: string | undefined, period: string | undefined): [number, number] | null {
  let hour = Number(h);
  const minute = m ? Number(m) : 0;
  if (period === "tarde" || period === "noite") {
    if (hour < 12) hour += 12;
  } else if (period === "manha" && hour === 12) {
    hour = 0;
  }
  if (hour > 23 || minute > 59) return null;
  return [hour, minute];
}

const at = (d: Date, h: number, m: number) => {
  const x = new Date(d.getFullYear(), d.getMonth(), d.getDate(), h, m);
  return Math.floor(x.getTime() / 1000);
};

/** Data/hora escrita no fim do texto; `null` se não houver. */
export function parseWhen(input: string, now: Date): ParsedWhen | null {
  const original = input.trimEnd();
  // Compara sem acentos e em minúsculas, com o mesmo tamanho do original.
  const plain = strip(original);
  if (plain.length !== original.length) return null;
  for (const [i, re] of PATTERNS.entries()) {
    const m = plain.match(re);
    if (!m || m.index === undefined) continue;
    const text = original.slice(0, m.index).trim();
    if (!text) continue;
    const match = original.slice(m.index).trim();
    if (i === 0) {
      const n = Number(m[1]);
      if (!n) continue;
      const mins = m[2].startsWith("h") ? n * 60 : n;
      const when = new Date(now.getTime() + mins * 60_000);
      when.setSeconds(0, 0);
      return { text, remindAt: Math.floor(when.getTime() / 1000), due: null, match };
    }
    if (i === 1 || i === 2) {
      const [day, h, mm, period] = i === 1 ? [m[1], m[2], m[3], m[4]] : [m[4], m[1], m[2], m[3]];
      // Número solto junto do dia ("entregar 3 amanhã") não é hora.
      const rawTime = plain.slice(m.index).trim().replace(day, "").trim();
      if (!explicitTime(rawTime, period)) continue;
      const d = dayFrom(day, now);
      const time = hourFrom(h, mm, period);
      if (!d || !time) continue;
      const when = at(d, time[0], time[1]);
      // "hoje 9h" às 14h: já passou; melhor não adivinhar nada.
      if (when * 1000 <= now.getTime()) return null;
      return { text, remindAt: when, due: null, match };
    }
    if (i === 3) {
      if (/^\d/.test(m[1]) && NOT_DATE.test(plain.slice(0, m.index))) continue;
      const d = dayFrom(m[1], now);
      if (!d) continue;
      return { text, remindAt: null, due: key(d), match };
    }
    // "a 3" sem "h" (ex.: "de 0 a 10", "dividir a 3") não é hora.
    if (!explicitTime(m[1], m[2])) continue;
    const t = m[1].replace(/^(as|a)\s+/, "").match(/^(\d{1,2})(?:(?:h|:)(\d{2}))?/);
    const time = t && hourFrom(t[1], t[2], m[2]);
    if (!time) continue;
    // Hora que já passou hoje: amanhã.
    let target = at(now, time[0], time[1]);
    if (target * 1000 <= now.getTime()) {
      target = at(new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1), time[0], time[1]);
    }
    return { text, remindAt: target, due: null, match };
  }
  return null;
}

/** Relógio da interface: devolve a mesma data enquanto o minuto não muda,
 * para não recalcular listas inteiras a cada tique. */
export function minuteTick(prev: Date, now = new Date()): Date {
  return Math.floor(now.getTime() / 60_000) === Math.floor(prev.getTime() / 60_000) ? prev : now;
}
