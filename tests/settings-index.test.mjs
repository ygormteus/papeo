// Confere o índice da busca nas Configurações (`src/lib/settingsIndex.ts`):
// cada rótulo precisa existir na tela, senão a busca leva a uma linha que
// sumiu. `pnpm test`.
import { readFileSync } from "node:fs";

const { SETTINGS_INDEX, searchSettings } = await import("../src/lib/settingsIndex.ts");

const failures = [];
const check = (ok, msg) => ok || failures.push(msg);

const source = ["Settings", "SyncSettings", "QuickReplies"]
  .map((f) => readFileSync(new URL(`../src/lib/${f}.svelte`, import.meta.url), "utf8"))
  .join("\n");

for (const e of SETTINGS_INDEX) {
  const asLabel = source.includes(`<span class="label">${e.label}</span>`);
  const asHeading = new RegExp(`<h2[^>]*>(?:<[^>]+/>)?\\s*${e.label}</h2>`).test(source);
  check(asLabel || asHeading, `rótulo não encontrado na tela: "${e.label}"`);
}

check(searchSettings("escuro")[0]?.label === "Tema", "escuro → Tema");
check(searchSettings("senha")[0]?.label === "Bloquear o Papeo", "senha → Bloquear o Papeo");
check(searchSettings("NOTIFICAÇÃO").length > 0, "busca sem acento e maiúsculas");
check(searchSettings("bandeja")[0]?.label === "Começar escondido na bandeja", "bandeja → rótulo primeiro");
check(searchSettings("").length === 0, "busca vazia não lista nada");
check(searchSettings("xyzzy").length === 0, "sem resultado");

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("índice das configurações ok");
