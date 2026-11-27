# 05 — Bloqueio e privacidade

- **Estado:** entregue (0.1, 0.12), retroativa
- **Relacionadas:** 02, 03, 06

## Problema

As conversas ficam à vista de quem passa pelo computador, e os sites dentro
do app poderiam agir em nome de outros.

## Requisitos

- **R1.** Onde o bloqueio estiver ligado, o Papeo deve abrir bloqueado,
  bloquear por tempo parado ou ao esconder a janela e pedir Touch ID, senha
  do sistema ou Windows Hello para desbloquear.
- **R2.** Se o sistema não tiver autenticação, então o Papeo deve oferecer
  um PIN próprio (4–64 caracteres, Argon2id, espera crescente depois de 5
  erros, até 15 min).
- **R3.** Enquanto bloqueado, o Papeo deve esconder todas as webviews,
  tornar a interface `inert` e recusar ações que mostrem conteúdo
  (responder, abrir anexo, prévia de link).
- **R4.** Ligar o bloqueio deve exigir autenticação.
- **R5.** O PIN nunca deve ir para o backup nem para a sincronização.
- **R6.** O Papeo deve permitir esconder a janela de capturas e
  compartilhamentos de tela.
- **R7.** Os sites só devem poder chamar `service_notify` e
  `service_status`; a conta é identificada pelo rótulo da webview.
- **R8.** Notificação, "desconectado", microfone/câmera e resposta rápida só
  devem valer quando a página aberta for do próprio site (`same_site`).
- **R9.** Onde a conta for um site `http`, o Papeo deve avisar na barra do
  topo.
- **R10.** "Sair da conta e apagar dados" deve apagar a sessão da conta,
  com confirmação.

## Design

`lock.rs` (`LAPolicy::DeviceOwnerAuthentication`,
`IUserConsentVerifierInterop`, PIN com `papeo_core::pin`),
`apply_layout` (esconde com `locked`), `LockScreen.svelte`,
capabilities (§7.1 da arquitetura), `privacy::same_site`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | uso real (Touch ID) | ok | não verificado |
| R2, R5 | testes de `pin.rs` e `store.rs` | — | não verificado (sem Hello) |
| R3 | mock (tela de bloqueio, foco) | parcial | — |
| R7 | `capabilities.test.mjs` | ok | ok |
| R8 | testes de `privacy.rs` | — | — |
