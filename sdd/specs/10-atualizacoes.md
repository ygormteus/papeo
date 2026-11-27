# 10 — Atualizações

- **Estado:** entregue (0.1), retroativa

## Requisitos

- **R1.** Na primeira abertura, o Papeo deve avisar que procura versões
  novas e deixar desligar (Esc mantém ligado).
- **R2.** Onde a consulta estiver ligada, o Papeo deve consultar o
  `latest.json` dos Releases 60 s depois de abrir e depois periodicamente
  (1 h após uma falha), com tempo-limite de 30 s na consulta e 30 min no
  download.
- **R3.** O Papeo só deve instalar com um clique da pessoa e reiniciar em
  seguida.
- **R4.** O Papeo deve recusar pacotes sem a assinatura da chave pública do
  `tauri.conf.json`.
- **R5.** Quando a versão mudar, o Papeo deve mostrar "o que mudou" a partir
  de `release-notes/<versão>.md` (ou `release-notes/en/<versão>.md` com a
  interface em inglês, spec 20 R8).
- **R6.** Restaurar um backup não deve mudar a preferência de atualização
  deste computador.
- **R7.** No macOS, o Papeo deve tirar a marca de quarentena do app
  atualizado.

## Design

`updater.rs` (só pelo Rust, sem permissões `updater:*` para JS, TLS do
sistema), `WhatsNew.svelte`, `notes.ts`, `release.yml`
(`check-updater-json`).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R2–R4 | atualizações reais 0.4.1 → 0.13 | ok | não verificado |
| R5 | `notes.test.mjs` | ok | — |
| R6 | teste de `updates` no núcleo | — | — |
