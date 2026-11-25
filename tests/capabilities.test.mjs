// Cada comando que uma interface chama precisa estar liberado na capability
// dela (src-tauri/capabilities/), senão falha só no app: "not allowed by ACL".
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, posix } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (p) => readFileSync(join(root, p), "utf8");

// `api.nome` → comando, a partir de src/lib/api.ts.
const api = read("src/lib/api.ts");
const commandOf = new Map();
// Cada entrada vai do seu nome até o próximo; o comando é o primeiro invoke.
const entries = api.split(/\n {2}(?=\w+: )/);
for (const entry of entries) {
  const name = entry.match(/^(\w+): /)?.[1];
  const cmd = entry.match(/invoke<[\s\S]*?>\(\s*"([a-z_:|]+)"/)?.[1];
  if (name && cmd && !commandOf.has(name)) commandOf.set(name, cmd);
}

const files = (dir) =>
  readdirSync(join(root, dir)).flatMap((f) => {
    // Sempre com "/": é como os caminhos aparecem abaixo (também no Windows).
    const p = posix.join(dir, f);
    return statSync(join(root, p)).isDirectory() ? files(p) : /\.(svelte|ts)$/.test(f) ? [p] : [];
  });
const used = (paths) => {
  const names = new Set();
  for (const p of paths) for (const m of read(p).matchAll(/\b(?:api|syncApi)\.(\w+)\(/g)) names.add(m[1]);
  return names;
};
const allowed = (cap) => new Set(JSON.parse(read(`src-tauri/capabilities/${cap}.json`)).permissions);

// Cada interface: arquivos que montam só ela; o resto é a janela principal.
const own = { popover: ["src/lib/Popover.svelte"], preview: ["src/lib/PreviewBar.svelte"] };
const skip = new Set(["src/lib/api.ts", "src/lib/devMock.ts", ...Object.values(own).flat()]);
const views = {
  default: [...files("src"), "src/App.svelte"].filter((p) => !skip.has(p)),
  popover: [...own.popover, "src/lib/RecentChats.svelte"],
  preview: own.preview,
};

const failures = [];
for (const [cap, paths] of Object.entries(views)) {
  const perms = allowed(cap);
  for (const name of used(paths)) {
    const cmd = commandOf.get(name);
    if (!cmd) failures.push(`${cap}: api.${name} sem comando conhecido em api.ts`);
    else if (cmd.startsWith("plugin:")) {
      // "plugin:opener|open_url" → permissão do plugin ("opener:...").
      const plugin = cmd.slice("plugin:".length).split("|")[0];
      if (![...perms].some((p) => p.startsWith(`${plugin}:`))) failures.push(`${cap}: ${cmd} (api.${name})`);
    } else if (!perms.has(`allow-${cmd.replaceAll("_", "-")}`)) failures.push(`${cap}: ${cmd} (api.${name})`);
  }
}

if (failures.length) {
  console.error("FALTA PERMISSÃO:", failures.join("; "));
  process.exit(1);
}
console.log("permissões ok");
