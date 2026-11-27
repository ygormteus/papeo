# 26 — Nextcloud Talk

- **Estado:** entregue
- **Versão:** 0.16.0
- **Relacionadas:** 01, 02, 19

## Problema

Quem usa o Nextcloud Talk da empresa só consegue colocá-lo no Papeo como
"Outro site": sem ícone nem cor, sem contador (o título da aba não traz
número), sem aviso de conta desconectada, e o endereço colado costuma ser
o do Nextcloud (Arquivos, Painel), não o do Talk.

## Fora do escopo

- Resposta rápida, conversas recentes e abrir conversa (só WhatsApp e
  Telegram).
- Login único (SSO) em outro domínio: só a tela de login do próprio
  Nextcloud marca "desconectado".
- Usar a API do Nextcloud (tudo pela página, como nos outros sites).

## Requisitos

- **R1.** O Papeo deve oferecer Nextcloud Talk ao adicionar uma conta, com
  nome, ícone e cor próprios, pedindo o endereço do servidor.
- **R2.** Quando a pessoa informar o servidor (com ou sem pasta) ou uma
  página dele (Arquivos, Painel, login), o Papeo deve abrir a lista de
  conversas do Talk (`…/index.php/apps/spreed/`); um endereço do Talk
  (lista ou conversa `/call/…`) fica como está.
- **R3.** O Papeo deve liberar microfone e câmera por padrão (chamadas),
  como nos mensageiros.
- **R4.** Quando a página do Nextcloud for a de visitante (login, segundo
  fator), o Papeo deve marcar a conta como desconectada.
- **R5.** No Talk, o contador deve ser a soma dos números das conversas na
  lista; fora do Talk ou sem a lista, vale o título.
- **R6.** Os avisos do site (notificações do navegador) devem virar
  notificações do Papeo, como nos outros serviços.

## Design

- **Núcleo (`service.rs`):** `ServiceKind::Nextcloud` (`"nextcloud"`), sem
  endereço padrão; `talk_url` ao criar. Sem diagnóstico (`probes_for`).
- **Script injetado:** `site()` reconhece o Nextcloud por
  `<head data-requesttoken>` (qualquer domínio); `LOGGED_OUT_SELECTORS`
  com `#body-login`; `MAIL_UNREAD.nextcloud` soma os
  `.counter-bubble__counter` da lista de conversas.
- **Interface:** tipo em `ServiceKind`, `KIND_META`, `needsUrl`,
  `mediaPolicy`, `ServiceAvatar` (ícone de conversas), campo de endereço
  com explicação no `AddServiceDialog`.
- **Dados persistidos:** valor novo de `kind` em `services.json` (versão
  anterior do Papeo não lê a lista: aceito, como no Slack e no Teams).
- **Segurança:** o servidor é o que a pessoa informou; avisos e
  microfone/câmera só no mesmo site (`same_site`), como nos outros.

## Tarefas

1. [x] Núcleo: tipo, endereço do Talk, testes.
2. [x] Script: reconhecer, desconectado, contador — testes.
3. [x] Interface.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R3 | teste `nextcloud_talk_url`; `AddServiceDialog` | não verificado | não verificado |
| R2 | teste `nextcloud_talk_url` | — | — |
| R4, R5 | `tests/email-shim.test.mjs` (Nextcloud) | não verificado (servidor real) | — |
| R6 | shim comum de notificações | não verificado | — |
