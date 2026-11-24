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
