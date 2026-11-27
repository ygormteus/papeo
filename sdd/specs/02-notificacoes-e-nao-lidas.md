# 02 — Notificações e não lidas

- **Estado:** entregue (0.1–0.13), retroativa
- **Relacionadas:** 03, 04, 05, 14

## Problema

Os sites dentro de uma webview não mostram notificações do sistema e não
dizem quantas mensagens faltam ler.

## Fora do escopo

- Notificações push disparadas pelo service worker com a página fechada.
- Saber de cliques em notificações do plugin no desktop.

## Requisitos

- **R1.** Quando o site de uma conta criar uma notificação, o Papeo deve
  mostrar uma notificação nativa com o nome da conta.
- **R2.** O Papeo deve tratar a permissão de notificação como concedida
  para o site e não pedir permissão do sistema em nome dele.
- **R3.** Se a mesma conta notificar menos de 1,5 s depois da anterior,
  então o Papeo deve descartar a nova.
- **R4.** Enquanto a conta estiver visível e a janela focada, o Papeo não
  deve notificar.
- **R5.** O Papeo só deve notificar quando `should_notify` permitir: aviso
  ligado na conta, conta dentro do modo atual, não silenciada e fora do
  não perturbe.
- **R6.** O Papeo deve montar o texto conforme a prévia escolhida: tudo, só
  o remetente ou nada.
- **R7.** Se a página aberta na conta não for do próprio site do serviço,
  então o Papeo deve ignorar a notificação.
- **R8.** Quando a pessoa clicar numa notificação do Papeo (macOS e
  Windows nativos), o Papeo deve abrir a conta e, se possível, a conversa.
- **R9.** Onde só houver o plugin, quando a janela ganhar foco até 20 s
  depois de uma notificação mostrada em segundo plano, o Papeo deve abrir
  aquela conta (opção `open_on_notification_focus`).
- **R10.** O Papeo deve mostrar o número de não lidas por conta (pelo título
  da página, ou pelo número que o site informa) e o total na barra
  lateral, no ícone do app e na bandeja.
- **R11.** Enquanto uma conta estiver hibernada, o Papeo não deve atualizar
  o contador dela, exceto pelo vigia IMAP (spec 09).
- **R12.** Com a barra lateral recolhida, o número de não lidas de cada
  conta deve aparecer em cima do ícone (até "99+"), sem precisar passar o
  mouse. Quando o número de uma conta subir (mensagem ou e-mail novo), o
  contador deve pulsar duas vezes (~2,4 s), nas duas larguras da barra; sem
  movimento se o sistema pedir menos animação. Só usa o número que o
  Papeo já recebe (nenhuma consulta a mais).

## Design

- Shim: troca `Notification`, `permissions.query` e
  `showNotification`. Chama `service_notify`.
- `notify.rs`: identifica a conta pelo rótulo da webview, aplica
  `same_site`, o intervalo mínimo e `should_notify`.
- `notifications.rs`: `UNUserNotificationCenter` (categorias
  `papeo.reply`/`papeo.later`) e toast WinRT.
- `parse_unread` (título) e `Inner::page_unread` (número do shim).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R2 | `notification-shim.test.mjs` | ok (uso real) | não verificado |
| R3–R5 | testes de `settings.rs` (`should_notify`) | ok | — |
| R6 | testes de `privacy.rs` | ok | — |
| R8 | uso real | ok | não verificado |
| R10 | `parse_unread`; shim do WhatsApp e do e-mail | ok | — |
| R12 | `pnpm check`; `Sidebar.svelte` (`badge`, `fresh`) | não verificado (app) | não verificado |
