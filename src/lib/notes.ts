// Notas de versão ("o que mudou"), escritas em `release-notes/<versão>.md`.
// Formato: seções `## Novidades`, `## Melhorias`, `## Correções` (ou outro
// título) com itens `- texto`; `**negrito**` e `` `código` `` dentro do texto.
// Tudo vira dados simples e é exibido pelo Svelte (escapado), nunca como HTML.

export type NoteKind = "new" | "improved" | "fixed" | "other";
export type Span = { text: string; bold?: boolean; code?: boolean };
export interface NoteSection {
  title: string;
  kind: NoteKind;
  items: Span[][];
}

function kindOf(title: string): NoteKind {
  const t = title.toLowerCase();
  if (/novidade|novo|nova/.test(t)) return "new";
  if (/melhori|aprimora/.test(t)) return "improved";
  if (/corre|conserto|bug/.test(t)) return "fixed";
  return "other";
}

/** `**negrito**` e `` `código` ``; o resto é texto. */
export function spans(text: string): Span[] {
  const out: Span[] = [];
  const re = /\*\*([^*]+)\*\*|`([^`]+)`/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (m.index! > last) out.push({ text: text.slice(last, m.index) });
    if (m[1] !== undefined) out.push({ text: m[1], bold: true });
    else out.push({ text: m[2], code: true });
    last = m.index! + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out;
}

/** Notas maiores que isto são cortadas (o `latest.json` não é assinado). */
const MAX_NOTES = 20_000;

export function parseNotes(markdown: string | null | undefined): NoteSection[] {
  const sections: NoteSection[] = [];
  let current: NoteSection | null = null;
  for (const raw of (markdown ?? "").slice(0, MAX_NOTES).split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const heading = /^#{1,4}\s+(.+)$/.exec(line);
    if (heading) {
      current = { title: heading[1].trim(), kind: kindOf(heading[1]), items: [] };
      sections.push(current);
      continue;
    }
    const item = /^[-*•]\s+(.+)$/.exec(line);
    const text = item ? item[1] : line;
    if (!current) {
      current = { title: "O que mudou", kind: "other", items: [] };
      sections.push(current);
    }
    current.items.push(spans(text));
  }
  return sections.filter((s) => s.items.length > 0);
}
