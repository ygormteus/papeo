// Catálogo inglês (spec 20, R7): toda frase marcada para tradução — `t()`/
// `tk()` na interface, `tr()`/`trf()`/`tr_in()`/`tk()` no Rust — precisa estar
// no catálogo; o catálogo não pode ter frase que ninguém usa nem trocar os
// `{nomes}` dos valores. `pnpm test`.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const failures = [];
const check = (ok, msg) => ok || failures.push(msg);

// Caminho com "/" também no Windows (para filtrar pastas).
const posix = (f) => f.split(sep).join("/");

function walk(dir, exts, out = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, exts, out);
    else if (exts.some((e) => name.endsWith(e))) out.push(path);
  }
  return out;
}

// Literais JS/Rust: aspas duplas, simples ou crase sem `${`.
const unescape = (s) =>
  s.replace(/\\(u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4}|.)/g, (_, c) =>
    c[0] === "u" ? String.fromCodePoint(parseInt(c.replace(/[u{}]/g, ""), 16)) : c === "n" ? "\n" : c === "t" ? "\t" : c,
  );

function used(files, pattern) {
  const keys = new Map();
  for (const file of files) {
    const src = readFileSync(file, "utf8");
    for (const m of src.matchAll(pattern)) {
      const raw = m.groups.dq ?? m.groups.sq ?? m.groups.bq;
      if (raw === undefined || (m.groups.bq !== undefined && raw.includes("${"))) continue;
      const key = unescape(raw);
      if (!keys.has(key)) keys.set(key, relative(root, file));
    }
  }
  return keys;
}

const names = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

// Interface.
const uiFiles = walk(join(root, "src"), [".svelte", ".ts"]).filter((f) => !posix(f).includes("/i18n/"));
const uiUsed = used(
  uiFiles,
  /\b(?:t|tk)\(\s*(?:"(?<dq>(?:[^"\\\n]|\\.)*)"|'(?<sq>(?:[^'\\\n]|\\.)*)'|`(?<bq>(?:[^`\\]|\\.)*)`)/g,
);
const uiCatalog = new Map();
const fragDir = join(root, "src/lib/i18n/en");
for (const name of readdirSync(fragDir).filter((n) => n.endsWith(".ts") && n !== "index.ts")) {
  const frag = (await import(pathToFileURL(join(fragDir, name)).href)).default;
  check(readFileSync(join(fragDir, "index.ts"), "utf8").includes(`./${name.replace(/\.ts$/, "")}"`), `i18n/en/index.ts não junta ${name}`);
  for (const [pt, en] of Object.entries(frag)) {
    const prev = uiCatalog.get(pt);
    check(prev === undefined || prev.en === en, `tradução diferente para "${pt}" em ${prev?.file} e ${name}`);
    uiCatalog.set(pt, { en, file: name });
  }
}
for (const [key, file] of uiUsed) check(uiCatalog.has(key), `interface sem inglês: "${key}" (${file})`);
for (const [pt, { en, file }] of uiCatalog) {
  check(uiUsed.has(pt), `interface: frase sem uso no catálogo ${file}: "${pt}"`);
  check(typeof en === "string" && en.trim() !== "", `interface: tradução vazia para "${pt}"`);
  check(names(pt) === names(en), `interface: {valores} diferentes em "${pt}" → "${en}"`);
}

// Rust.
const rsFiles = [...walk(join(root, "src-tauri/src"), [".rs"]), ...walk(join(root, "crates/papeo-core/src"), [".rs"])].filter(
  (f) => !posix(f).includes("/i18n/"),
);
const rsUsed = used(rsFiles, /\b(?:(?:trf?|tk)\(|tr_in\([^,()]+,)\s*"(?<dq>(?:[^"\\]|\\.)*)"/g);
const rsCatalog = new Map();
const enRs = ["en_app.rs", "en_core.rs"]
  .map((n) => readFileSync(join(root, "crates/papeo-core/src/i18n", n), "utf8"))
  .join("\n");
for (const m of enRs.matchAll(/^\s*"((?:[^"\\]|\\.)*)"\s*=>\s*"((?:[^"\\]|\\.)*)",\s*$/gm)) {
  const pt = unescape(m[1]);
  check(!rsCatalog.has(pt), `en_*.rs: frase repetida "${pt}"`);
  rsCatalog.set(pt, unescape(m[2]));
}
for (const [key, file] of rsUsed) check(rsCatalog.has(key), `Rust sem inglês: "${key}" (${file})`);
for (const [pt, en] of rsCatalog) {
  check(rsUsed.has(pt), `en_*.rs: frase sem uso: "${pt}"`);
  check(names(pt) === names(en), `en_*.rs: {valores} diferentes em "${pt}" → "${en}"`);
}

if (failures.length) {
  console.error("Falhas:\n- " + failures.join("\n- "));
  process.exit(1);
}
console.log(`i18n ok (${uiUsed.size} frases na interface, ${rsUsed.size} no Rust)`);
