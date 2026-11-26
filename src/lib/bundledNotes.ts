// Notas de versão embutidas no app, para mostrar "o que mudou" depois de
// atualizar sem depender da internet (`release-notes/<versão>.md`).
const bundled = import.meta.glob("../../release-notes/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

export function bundledNotes(version: string): string | null {
  return bundled[`../../release-notes/${version}.md`] ?? null;
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
export function notesHistory(upTo: string): { version: string; notes: string }[] {
  return Object.entries(bundled)
    .map(([path, notes]) => ({ version: path.replace(/^.*\/|\.md$/g, ""), notes }))
    .filter((n) => compareVersions(n.version, upTo) <= 0)
    .sort((a, b) => compareVersions(b.version, a.version));
}
