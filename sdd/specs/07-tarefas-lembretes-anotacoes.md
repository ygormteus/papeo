# 07 — Tarefas, lembretes e anotações

- **Estado:** entregue (0.4), retroativa
- **Relacionadas:** 03, 08, 11

## Problema

Combinados feitos numa conversa se perdem. O diferencial é o lembrete saber
de qual conversa veio e abri-la num clique.

## Requisitos

- **R1.** O Papeo deve permitir criar tarefas com texto (até 500
  caracteres), data e lembrete (data e hora), soltas ou ligadas a uma
  conversa de uma conta.
- **R2.** Com uma conversa aberta, ⌘⇧N / Ctrl⇧N deve abrir um painel ao lado
  do site (que continua visível) com as tarefas e a anotação daquela
  conversa.
- **R3.** O Papeo deve entender datas no texto em português ("amanhã 15h",
  "dia 5", "em 30 min").
- **R4.** O Papeo deve repetir tarefas (todo dia, dias úteis, semanal,
  mensal) a partir de uma âncora fixa. Adiar não desloca a série.
- **R5.** Quando chegar a hora de um lembrete, o Papeo deve notificar com
  "Adiar 1 h" e "Concluir", mesmo em não perturbe. O clique abre a
  conversa na conta certa.
- **R6.** O Papeo deve guardar uma anotação por contato (até 20.000
  caracteres), salva sozinha.
- **R7.** O Papeo deve mostrar as tarefas em Tarefas, no "Para hoje" do
  Início, na mini janela e na busca (⌘K).
- **R8.** Tarefas apagadas devem ficar como apagadas por 90 dias (para a
  sincronização) e então sumir.
- **R9.** O Papeo deve gravar tudo em `organizer.json` e no backup.

## Design

`papeo_core::organizer` (modelo, limites, `due_reminders`, `Occurrence`
contra "Concluir" velho), `organizer.rs` (comandos, `tick` no laço de
manutenção, evento `papeo://organizer`), `when.ts`, `tasks.ts`,
`ContactPanel.svelte`, `Tasks.svelte`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R4, R8 | testes de `organizer.rs` (núcleo e app) | — | — |
| R3 | `when.test.mjs` | — | — |
| R5 | uso real | ok | não verificado |
| R7 | `tasks.test.mjs`; mock `?demo` | parcial | — |
