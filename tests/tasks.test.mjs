// Testa as regras de data das tarefas (`src/lib/tasks.ts`): `pnpm test`.
const t = await import("../src/lib/tasks.ts");

const failures = [];
const check = (ok, msg) => ok || failures.push(msg);
const at = (d) => Math.floor(d.getTime() / 1000);
const task = (extra) => ({ id: "x", text: "t", done: false, createdAt: 0, updatedAt: 0, ...extra });

// Quarta, 25 nov. 2026, 14:10 (hora local).
const now = new Date(2026, 10, 25, 14, 10);
check(t.dateKey(now) === "2026-11-25", "dateKey");
check(t.groupOf(task({ due: "2026-11-24" }), now) === "overdue", "vencida ontem");
check(t.groupOf(task({ due: "2026-11-25" }), now) === "today", "vence hoje");
check(t.groupOf(task({ remindAt: at(new Date(2026, 10, 25, 9)) }), now) === "today", "lembrete de hoje que já passou");
check(t.groupOf(task({ remindAt: at(new Date(2026, 10, 24, 23)) }), now) === "overdue", "lembrete de ontem");
check(t.groupOf(task({ remindAt: at(new Date(2026, 10, 26, 9)) }), now) === "upcoming", "amanhã");
check(t.groupOf(task({}), now) === "someday", "sem data");
check(t.groupOf(task({ done: true, due: "2026-11-01" }), now) === "done", "feita");

const quick = t.quickTimes(now);
check(quick.map((q) => q.label).join("|") === "Em 1 h|Hoje 18h|Amanhã 9h|Segunda 9h", `atalhos: ${quick.map((q) => q.label)}`);
check(quick[0].at === at(new Date(2026, 10, 25, 15, 10)), "em 1 h");
check(quick[3].at === at(new Date(2026, 10, 30, 9)), "segunda 9h");
const late = t.quickTimes(new Date(2026, 10, 25, 17, 45));
check(!late.some((q) => q.label === "Hoje 18h"), "sem 'hoje 18h' perto das 18h");
// Domingo: segunda é amanhã, então a "segunda 9h" é a da outra semana.
const sunday = t.quickTimes(new Date(2026, 10, 29, 10));
check(sunday[sunday.length - 1].at === at(new Date(2026, 11, 7, 9)), "segunda depois de amanhã");

check(t.dayLabel(new Date(2026, 10, 26), now) === "Amanhã", "amanhã");
check(t.dayLabel(new Date(2026, 10, 27), now) === "sex., 27 nov.", `dia da semana: ${t.dayLabel(new Date(2026, 10, 27), now)}`);
check(t.dayLabel(new Date(2027, 0, 5), now) === "5 jan. 2027", "outro ano");
check(t.whenLabel(task({ remindAt: at(new Date(2026, 10, 25, 9, 5)) }), now) === "Hoje 9:05", "hora");
check(t.fromInputs("2026-11-25", "09:30") === at(new Date(2026, 10, 25, 9, 30)), "data + hora");
check(t.fromInputs("2026-11-25", "") === null, "sem hora");

const sorted = t.sortTasks([
  task({ id: "b", due: "2026-11-26" }),
  task({ id: "a", remindAt: at(new Date(2026, 10, 25, 10)) }),
  task({ id: "c" }),
]);
check(sorted.map((x) => x.id).join("") === "abc", "ordem por data");

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("tarefas ok");
