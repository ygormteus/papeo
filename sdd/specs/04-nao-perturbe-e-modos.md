# 04 — Não perturbe, trabalho e pessoal

- **Estado:** entregue (0.1, 0.9), retroativa
- **Relacionadas:** 02, 12

## Problema

Mensagens do trabalho à noite e do pessoal no expediente interrompem a
pessoa.

## Requisitos

- **R1.** O Papeo deve oferecer não perturbe por 1 h, por 8 h e "até
  reativar", e um horário silencioso diário (inclusive passando da
  meia-noite).
- **R2.** O Papeo deve permitir silenciar uma conta até um momento.
- **R3.** O Papeo deve permitir marcar cada conta como trabalho, pessoal ou
  as duas, e escolher o modo Todos, Trabalho, Pessoal ou Automático (pelo
  expediente, com dias e horários; expediente que passa da meia-noite conta
  pelo dia de início).
- **R4.** Enquanto uma conta estiver fora do modo, o Papeo deve tirá-la da
  barra lateral, da bandeja e dos atalhos (a ativa continua visível), não
  notificar nem contar no total.
- **R5.** Onde `sleep_outside_mode` estiver ligado, o Papeo deve hibernar as
  contas fora do modo.
- **R6.** Quando a pessoa trocar o modo à mão dentro do Automático, o Papeo
  deve manter a escolha até a próxima virada do expediente. Ao reiniciar,
  volta ao automático.
- **R7.** Os lembretes de tarefas devem tocar mesmo em não perturbe.
- **R8.** ⌘⇧M / Ctrl⇧M deve alternar o modo, só com o app desbloqueado e com
  os modos em uso.

## Design

`papeo_core::settings` (`dnd_active`, `QuietHours`, `Mode::allows`,
`WorkHours::contains`, `minutes_until_change`, `ModeOverride` só na
memória). `should_notify` junta tudo.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R3, R6 | testes de `settings.rs` | — | — |
| R4 | mock `?modos` | parcial | — |
| R5, R7–R8 | uso real | não verificado | não verificado |
