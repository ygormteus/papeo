# 03 — Resposta rápida e mini janela

- **Estado:** entregue (0.1–0.13), retroativa
- **Relacionadas:** 02, 05, 07

## Problema

Responder uma mensagem curta exige trazer o app para frente, achar a conta
e a conversa.

## Fora do escopo

- Resposta rápida em Discord e e-mail.
- Anexos, áudio ou emojis na resposta rápida.

## Requisitos

- **R1.** O Papeo deve mostrar, numa mini janela ancorada no ícone da barra
  de menus ou da bandeja, as conversas recentes (até 8, só na memória) e um
  campo para responder.
- **R2.** Onde a conta for WhatsApp ou Telegram e a prévia mostrar a
  conversa, a notificação nativa deve oferecer "Responder" e "Lembrar em
  1 h".
- **R3.** Quando a pessoa enviar uma resposta, o Papeo deve abrir a conversa
  exata no site, conferir o nome no cabeçalho, escrever no campo do próprio
  site e enviar.
- **R4.** Se houver um rascunho no campo, ou mais de uma conversa com o mesmo
  nome, então o Papeo não deve escrever nada e deve avisar.
- **R5.** Se o envio não se confirmar em 40 s, então o Papeo deve cancelar e
  dizer que o resultado é incerto, sem tentar de novo.
- **R6.** Enquanto o app estiver bloqueado, o Papeo deve recusar resposta e
  "Lembrar em 1 h", conferindo o bloqueio de novo no momento do clique.
- **R7.** Se a página aberta não for do próprio site, então o Papeo não deve
  entregar o texto.
- **R8.** Se a conta estiver hibernada, então o Papeo deve carregá-la
  escondida (até 60 s) antes de responder.
- **R9.** Quando a mini janela perder o foco, o Papeo deve escondê-la, e
  fechá-la 5 min depois de escondida.
- **R10.** "Lembrar em 1 h" deve criar uma tarefa ligada à conversa (spec 07)
  com o texto "Responder: <última mensagem>" quando a prévia estiver
  completa.

## Design

- `popover.rs` + `Popover.svelte` (capability `popover`).
- `reply.rs` (`ensure_loaded`, `same_site`, `__papeoReply`, consulta a cada
  400 ms, `give_up`).
- Shim: `__papeoReply`, `__papeoReplyStatus` e `__papeoReplyCancel`, com
  seletores por site (spec 14).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R9 | uso real | ok | não verificado |
| R3–R5 | `notification-shim.test.mjs` (envio, rascunho, vazio) | ok (uso real) | não verificado |
| R4 (nome repetido fora da tela) | — | não verificado | não verificado |
| R6–R7 | revisão; leitura do código | — | — |
