// Busca das respostas prontas digitadas com "/" (resposta rápida).
import type { QuickReply } from "./api";

/** Respostas prontas que combinam com o que foi digitado depois de "/". */
export function matchQuickReplies(replies: QuickReply[], draft: string): QuickReply[] {
  if (!draft.startsWith("/")) return [];
  const fold = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const q = fold(draft.slice(1).trim());
  const starts = (r: QuickReply) => fold(r.shortcut).startsWith(q);
  return replies
    .filter((r) => !q || starts(r) || fold(r.shortcut).includes(q) || fold(r.text).includes(q))
    .sort((a, b) => Number(starts(b)) - Number(starts(a)))
    .slice(0, 8);
}
