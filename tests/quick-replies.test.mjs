// Testa a busca das respostas prontas (`src/lib/quickReplies.ts`): `pnpm test`.
const { matchQuickReplies } = await import("../src/lib/quickReplies.ts");

const failures = [];
const check = (ok, msg) => ok || failures.push(msg);
const replies = [
  { shortcut: "orcamento", text: "Segue o orçamento." },
  { shortcut: "recebido", text: "Recebido, retorno até amanhã." },
  { shortcut: "", text: "Bom dia! Como posso ajudar?" },
  { shortcut: "pix", text: "A chave Pix é o CNPJ; o orçamento vale 7 dias." },
];
const shortcuts = (draft) => matchQuickReplies(replies, draft).map((r) => r.shortcut || r.text);

check(shortcuts("oi").length === 0, "sem / não sugere nada");
check(shortcuts("/").length === 4, "só / mostra todas");
check(shortcuts("/rec")[0] === "recebido", "atalho pelo começo");
check(JSON.stringify(shortcuts("/orç")) === '["orcamento","pix"]', `sem acento e atalho primeiro: ${shortcuts("/orç")}`);
check(shortcuts("/bom dia")[0] === "Bom dia! Como posso ajudar?", "acha pelo texto");
check(shortcuts("/xyz").length === 0, "nada combina");
const many = Array.from({ length: 20 }, (_, i) => ({ shortcut: `a${i}`, text: "x" }));
check(matchQuickReplies(many, "/a").length === 8, "no máximo 8 sugestões");

if (failures.length) {
  console.error("FALHOU:", failures.join("; "));
  process.exit(1);
}
console.log("respostas prontas ok");
