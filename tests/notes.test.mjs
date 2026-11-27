// Testa o leitor das notas de versão (`src/lib/notes.ts`): `pnpm test`.
import { readFileSync } from "node:fs";

const { parseNotes, spans } = await import("../src/lib/notes.ts");

const failures = [];
const check = (ok, msg) => ok || failures.push(msg);

const md = readFileSync(new URL("../release-notes/0.2.0.md", import.meta.url), "utf8");
const s = parseNotes(md);
check(s.length === 2 && s[0].kind === "new" && s[1].kind === "improved", "seções e tipos");
check(s[0].items.length >= 3, "itens");
check(JSON.stringify(spans("a **b** `c` d")) === JSON.stringify([
  { text: "a " }, { text: "b", bold: true }, { text: " " }, { text: "c", code: true }, { text: " d" },
]), "negrito e código");
const loose = parseNotes("Só um texto\n- item <b>x</b>\n## Correções\n- conserto");
check(loose[0].title === "O que mudou" && loose[0].items.length === 2, "texto sem seção");
check(loose[0].items[1][0].text === "item <b>x</b>", "HTML fica como texto");
check(loose[1].kind === "fixed", "correções");
const t0 = Date.now();
parseNotes("- " + "**a".repeat(20000));
check(Date.now() - t0 < 500, "texto hostil não trava");
check(parseNotes("").length === 0 && parseNotes(null).length === 0, "vazio");
check(parseNotes("texto", "What's new")[0].title === "What's new", "título sem seção no idioma da tela");

// Notas em inglês (spec 20): toda versão tem as duas, com as mesmas seções,
// os mesmos tipos e o mesmo número de itens.
import { readdirSync } from "node:fs";
const dir = new URL("../release-notes/", import.meta.url);
const versions = readdirSync(dir).filter((n) => n.endsWith(".md"));
const english = new Set(readdirSync(new URL("en/", dir)).filter((n) => n.endsWith(".md")));
for (const name of versions) {
  check(english.has(name), `falta release-notes/en/${name}`);
  if (!english.has(name)) continue;
  const pt = parseNotes(readFileSync(new URL(name, dir), "utf8"));
  const en = parseNotes(readFileSync(new URL(`en/${name}`, dir), "utf8"));
  const shape = (n) => n.map((x) => `${x.kind}:${x.items.length}`).join(",");
  check(shape(pt) === shape(en), `${name}: seções diferentes em inglês (${shape(pt)} × ${shape(en)})`);
}
for (const name of english) check(versions.includes(name), `release-notes/en/${name} sem a versão em português`);

if (failures.length) {
  console.error("FALHOU:", failures.join("; "));
  process.exit(1);
}
console.log("notas ok");
