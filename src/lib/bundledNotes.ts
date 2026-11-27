// Notas de versão embutidas no app, para mostrar "o que mudou" depois de
// atualizar sem depender da internet: `release-notes/<versão>.md` (português)
// e `release-notes/en/<versão>.md` (inglês, spec 20). Sem a versão em
// inglês, vale a em português.
const bundled = import.meta.glob(["../../release-notes/*.md", "../../release-notes/en/*.md"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

export type NotesLang = "pt" | "en";

const path = (version: string, lang: NotesLang) =>
  lang === "en" ? `../../release-notes/en/${version}.md` : `../../release-notes/${version}.md`;

export function bundledNotes(version: string, lang: NotesLang = "pt"): string | null {
  return bundled[path(version, lang)] ?? bundled[path(version, "pt")] ?? null;
}

/** Compara "0.10.0" e "0.9.1" pelos números (negativo: `a` é mais antiga). */
export function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map((n) => parseInt(n, 10) || 0);
  const pb = b.split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

/** Notas embutidas até `upTo` (inclusive), da mais nova para a mais antiga. */
export function notesHistory(upTo: string, lang: NotesLang = "pt"): { version: string; notes: string }[] {
  return Object.keys(bundled)
    .filter((p) => !p.includes("/en/"))
    .map((p) => p.replace(/^.*\/|\.md$/g, ""))
    .filter((version) => compareVersions(version, upTo) <= 0)
    .map((version) => ({ version, notes: bundledNotes(version, lang)! }))
    .sort((a, b) => compareVersions(b.version, a.version));
}
