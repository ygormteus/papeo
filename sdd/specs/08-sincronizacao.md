# 08 — Sincronização das tarefas

- **Estado:** em espera (código completo e revisado; falta o cliente OAuth
  do Google)
- **Relacionadas:** 07

## Problema

Quem usa o Papeo em dois computadores quer as mesmas tarefas e anotações nos
dois, sem confiar o conteúdo a um servidor.

## Fora do escopo

- Sincronizar contas, logins ou configurações.
- Servidor próprio.

## Requisitos

- **R1.** O Papeo deve sincronizar tarefas e anotações pelo `appDataFolder`
  do Google Drive (escopo `drive.appdata`: não vê outros arquivos).
- **R2.** O Papeo deve cifrar tudo com uma frase-senha (mínimo 10
  caracteres): Argon2id (64 MiB, 3 passadas) e XChaCha20-Poly1305, com o
  id do arquivo como dado associado.
- **R3.** O Papeo deve gravar um arquivo por computador e juntar registro a
  registro: vence o `updated_at` mais novo. Anotação mudada dos dois lados
  desde a base guarda as duas versões com uma marca de conflito.
- **R4.** Se o cliente OAuth não estiver no build, então o Papeo deve
  esconder a sincronização e não usar a rede.
- **R5.** O Papeo deve sincronizar 5 s depois de uma mudança e a cada 2 min.
  Depois de um erro, espera 5 min.
- **R6.** Se o arquivo de um computador estiver parado há 80 dias ou mais,
  então o Papeo deve juntá-lo uma última vez e tirá-lo da nuvem (a partir de
  88 dias, só tirar).
- **R7.** "Esqueci a frase-senha" deve recomeçar a sincronização com outra
  frase, mantendo as tarefas de cada computador.

## Perguntas

- [ ] Criar o cliente OAuth "App para computador" no Google Cloud e os
  secrets `PAPEO_GOOGLE_CLIENT_ID`/`PAPEO_GOOGLE_CLIENT_SECRET` (ver
  CONTRIBUTING).
- [ ] Impor um piso aos parâmetros do Argon2 lidos do `meta.json` na nuvem
  (arquitetura §14, risco 9)?

## Design

`papeo_core::sync` (cifra, junção, retenção), `sync.rs` (rodadas,
`sync.json` 0600, evento `papeo://sync`), `drive.rs` (OAuth com PKCE e
`state`, API do Drive), `SyncSettings.svelte`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R2–R3, R6 | testes de `sync.rs` do núcleo | — | — |
| R4 | mock `?semsync` | ok | — |
| R1, R5, R7 | — | não verificado | não verificado |
