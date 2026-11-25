// Testa a leitura de datas no texto da tarefa (`src/lib/when.ts`): `pnpm test`.
const { parseWhen } = await import("../src/lib/when.ts");

const failures = [];
const check = (ok, msg) => ok || failures.push(msg);
const at = (...a) => Math.floor(new Date(...a).getTime() / 1000);
// Quarta, 25 nov. 2026, 14:10.
const now = new Date(2026, 10, 25, 14, 10);
const p = (s) => parseWhen(s, now);

let r = p("Ligar pra Ana amanhã 15h");
check(r?.text === "Ligar pra Ana" && r.remindAt === at(2026, 10, 26, 15), `amanhã 15h: ${JSON.stringify(r)}`);
check(r?.match === "amanhã 15h", "trecho reconhecido");
r = p("Reunião sexta às 9h30");
check(r?.remindAt === at(2026, 10, 27, 9, 30), `sexta às 9h30: ${JSON.stringify(r)}`);
r = p("Mandar proposta 10h amanhã");
check(r?.remindAt === at(2026, 10, 26, 10), `10h amanhã: ${JSON.stringify(r)}`);
r = p("Pagar boleto amanhã");
check(r?.due === "2026-11-26" && r.remindAt === null, `só o dia: ${JSON.stringify(r)}`);
r = p("Pagar aluguel dia 5");
check(r?.due === "2026-12-05", `dia 5 (mês que vem): ${JSON.stringify(r)}`);
r = p("Aniversário 10/12");
check(r?.due === "2026-12-10", `10/12: ${JSON.stringify(r)}`);
r = p("Renovar 20/03");
check(r?.due === "2027-03-20", `data passada vai pro ano que vem: ${JSON.stringify(r)}`);
r = p("Ligar de volta às 9");
check(r?.remindAt === at(2026, 10, 26, 9), `às 9 já passou → amanhã: ${JSON.stringify(r)}`);
r = p("Café às 3 da tarde");
check(r?.remindAt === at(2026, 10, 25, 15), `3 da tarde: ${JSON.stringify(r)}`);
r = p("Tirar o bolo em 30 min");
check(r?.remindAt === at(2026, 10, 25, 14, 40), `em 30 min: ${JSON.stringify(r)}`);
r = p("Revisar texto em 2 horas");
check(r?.remindAt === at(2026, 10, 25, 16, 10), `em 2 horas: ${JSON.stringify(r)}`);
r = p("Ver na segunda");
check(r?.due === "2026-11-30", `na segunda: ${JSON.stringify(r)}`);
r = p("Ligar quarta 8h");
check(r?.remindAt === at(2026, 11, 2, 8), `quarta é hoje → próxima: ${JSON.stringify(r)}`);

check(p("Comprar 2 pães") === null, "número solto não é hora");
check(p("Entregar 3 amanhã")?.due === "2026-11-26", `"3 amanhã" não vira 3h: ${JSON.stringify(p("Entregar 3 amanhã"))}`);
check(p("Ver segunda 2")?.remindAt == null, "segunda 2 não vira 2h");
check(p("Nota de 0 a 10") === null, "de 0 a 10");
check(p("Dividir a conta a 3") === null, "a 3 sem h");
check(p("Pagar parcela 3/12") === null, "parcela 3/12 não é data");
check(p("Ligar hoje 9h") === null, "hoje 9h já passou: não adivinha");
check(p("Ligar hoje 18h")?.remindAt === at(2026, 10, 25, 18), "hoje 18h");
check(p("Ligar para a Ana") === null, "sem data");
check(p("amanhã") === null, "só a data, sem tarefa");
check(p("Ligar 25h") === null, "hora inválida");
check(p("Pagar dia 31/02") === null, "dia inválido");

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("datas ok");
