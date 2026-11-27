# 12 — Memória, hibernação e volta do repouso

- **Estado:** entregue (0.1–0.13), retroativa
- **Relacionadas:** 01, 04, 09

## Problema

Cada WhatsApp Web passa de 1–2 GB com o tempo. Depois de horas em repouso,
sites podem ficar presos carregando.

## Requisitos

- **R1.** O Papeo deve mostrar a memória de cada conta (footprint no macOS,
  árvore de processos do WebView2 no Windows).
- **R2.** O Papeo deve hibernar uma conta parada depois do tempo escolhido
  (até 24 h) ou à mão, nunca com download em curso.
- **R3.** Enquanto hibernada, a conta não recebe notificações (exceto e-mail
  por IMAP).
- **R4.** Onde o limite de memória estiver ligado (padrão 2 GB), o Papeo
  deve recarregar uma conta acima dele fora de vista, parada há 10 min,
  sem mídia e no máximo 1 vez por hora.
- **R5.** Quando o computador voltar do repouso (salto de 5 min ou mais no
  relógio), o Papeo deve esperar a pessoa presente (tela acesa, uso há
  menos de 2 min) e a rede, e então reabrir as contas carregadas, a visível
  primeiro e as demais a cada 3 s.
- **R6.** O botão Recarregar e ⌘R devem fechar e recriar a webview (com
  download em curso, só recarregar).

## Design

`memory.rs`, `idle.rs`, `webviews::{hibernate, restart, reload}`,
`spawn_maintenance` (`lib.rs`), `HomeStats.svelte`,
`homeState.svelte.ts`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | medição real (~1,1–2,4 GB por WhatsApp) | ok | não verificado |
| R4 | — | não verificado (disparo real) | não verificado |
| R5 | teste `detects_sleep_by_wall_clock_gap`; logs de um repouso real | parcial | não verificado |
