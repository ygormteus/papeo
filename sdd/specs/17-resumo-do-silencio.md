# 17 — Resumo do silêncio

- **Estado:** entregue
- **Versão:** 0.15.0
- **Relacionadas:** 02, 04

## Problema

Em "não perturbe" ou no horário silencioso, o Papeo descarta os avisos: ao
voltar, a pessoa não sabe o que chegou nem onde, e precisa abrir conta por
conta.

## Fora do escopo

- Conta silenciada sozinha (`muted_until`), conta fora do modo e conta com
  notificações desligadas: a pessoa escolheu não saber delas.
- Guardar o resumo depois de fechar o app (fica só na memória).
- Conteúdo das mensagens no resumo: só contagens e nomes de conversas.
- Separar dois silêncios seguidos cuja passagem aconteceu com o computador
  dormindo (um não perturbe que venceu no repouso e o horário silencioso já
  valendo ao acordar): viram um resumo só.

## Requisitos

- **R1.** Enquanto o não perturbe ou o horário silencioso estiver valendo,
  quando uma conta que notificaria fora do silêncio receber uma mensagem, o
  Papeo deve contá-la (por conta e por conversa, até 5 conversas por conta,
  a mais recente primeiro) sem notificar.
- **R2.** A mensagem da conta à vista, com a janela em foco, não conta.
- **R3.** Quando o silêncio terminar (fim do prazo, do horário silencioso
  ou reativado à mão) com ao menos uma mensagem contada, o Papeo deve
  mostrar uma notificação "Enquanto você estava em silêncio" com o total e
  as contas, e um cartão na tela inicial com cada conta, o número de
  mensagens e as conversas, que abrem a conta ou a conversa.
- **R4.** O cartão deve ficar até a pessoa fechá-lo ou até o próximo resumo.
- **R5.** Enquanto o app estiver bloqueado, a notificação deve mostrar só o
  total e o número de contas, e o cartão não aparece (como as recentes).
- **R6.** Onde a prévia das notificações for "oculta", o cartão não deve
  mostrar nomes de conversas, só as contas e as contagens.
- **R7.** O Papeo deve oferecer a opção "Resumo ao sair do silêncio" em
  Configurações → Notificações, ligada por padrão. Desligada, nada é
  contado.
- **R8.** Conta removida (inclusive por backup restaurado) some do resumo e
  da contagem.
- **R9.** Enquanto uma conta estiver fora do modo atual, o cartão não deve
  mostrá-la (como as recentes).
- **R10.** Quando a pessoa desligar a opção, o cartão do último resumo deve
  sair.

## Perguntas

- [x] Quais silêncios contam: não perturbe e horário silencioso (usuário,
  2026-11-27).

## Design

- **Módulos:** `papeo_core::silence` (novo, sem Tauri): `SilenceLog`
  (`start`, `record`, `finish`, `forget`) e `SilenceSummary`, com testes.
  `notify::deliver`: se o silêncio começou depois da última conferência,
  ele já começa ali (a conferência não zera o que foi contado); quando `should_notify` recusa só por causa do
  silêncio (`dnd_active`, com a conta notificando, sem silêncio próprio e
  dentro do modo) e a conta não está à vista, chama `record`.
  `silence.rs` (app, novo): `check` compara o silêncio de agora com o da
  última conferência; na saída, fecha o resumo, publica e notifica. Chamado
  pelo laço de manutenção (15 s), por `set_dnd` e `set_quiet_hours`.
  Interface: cartão no `Home.svelte`; opção em `Settings.svelte`.
- **Dados persistidos:** `Settings.silence_summary` (`bool`, padrão
  `true`; arquivo antigo lê `true`). Contagem e resumo só na memória
  (`Inner`).
- **Comandos e permissões:** `dismiss_silence_summary` em `COMMANDS` e na
  capability `default`; `api.ts`, `devMock.ts`. `set_preferences` ganha
  `silenceSummary`. `Snapshot.silence_summary` (vazio com o app bloqueado;
  conversas vazias com a prévia oculta).
- **Concorrência:** `check` pega o lock do estado só para a transição;
  notificação e `publish` fora dele. Nada de `is_focused` com o lock (o
  `deliver` já lê o foco antes).
- **Segurança e privacidade:** nada novo em disco nem na rede; os nomes de
  conversa seguem as mesmas regras das recentes.
- **Plataformas:** iguais.

## Tarefas

1. [x] `papeo_core::silence` e `Settings.silence_summary` (R1, R7, R8) —
   testes do registro e do fechamento.
2. [x] `deliver` registra; `silence::check` e o laço (R1–R3).
3. [x] Snapshot, comando e cartão (R3–R6); opção (R7).

## Verificação

Revisão crítica independente: sem bloqueante; 1 médio (mensagens do começo
do horário silencioso zeradas) e 3 baixos; 3 corrigidos e 1 aceito (ver
"Fora do escopo"); re-checagem aprovada. A notificação também passou a
ignorar contas fora do modo, como o cartão.

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R8 | testes `counts_by_account_and_keeps_recent_chats`, `removed_account_leaves_log_and_summary`, `nothing_held_means_no_summary` | não verificado (aviso real) | não verificado |
| R2 | `notify::deliver` (mesma regra da conta à vista) | não verificado | não verificado |
| R3, R5 | teste `summary_text` (texto e versão bloqueada); mock `?silencio` | não verificado | não verificado |
| R4, R6, R9, R10 | `state::silence_view`, `set_preferences`; mock `?silencio` | não verificado | — |
| R7 | teste de arquivo antigo em `settings.rs` | não verificado | — |
