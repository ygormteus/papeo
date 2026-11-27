# 11 — Backup e persistência

- **Estado:** entregue (0.1–0.13), retroativa
- **Relacionadas:** 01, 07

## Requisitos

- **R1.** O Papeo deve exportar e importar um backup (`kind:
  "papeo-backup"`) com contas, configurações e tarefas, sem logins, PIN,
  senhas de e-mail nem preferências de atualização.
- **R2.** Se o backup tiver qualquer item inválido, então o Papeo deve
  recusá-lo inteiro e mostrar uma prévia antes de restaurar.
- **R3.** Ao restaurar, o Papeo deve manter os logins das sessões que já
  existem neste computador e não apagar pastas de sessão de contas fora do
  backup.
- **R4.** Ao carregar `services.json`, o Papeo deve descartar só a conta
  inválida (guardando o original como `.json.corrompido`) e encurtar nomes
  longos.
- **R5.** Se o arquivo for de uma versão mais nova ou ilegível, então o
  Papeo deve preservá-lo como `.corrompido` e abrir com os padrões.
- **R6.** Se `services.json` não puder ser gravado, então o Papeo deve gravar
  a sessão em `services.sessao.json` e não limpar sessões órfãs.
- **R7.** O Papeo deve gravar de forma atômica (arquivo temporário, `fsync`,
  troca de nome).
- **R8.** Todo campo novo deve ler arquivos antigos (`serde(default)`) com
  teste.
- **R9.** O Papeo deve listar as sessões órfãs e deixar a pessoa apagá-las.

## Design

`papeo_core::store` (`load_guarded`, `write_atomic`, sanitização estrita do
backup), `state.rs` (`DiskWriter`), comandos `export_config`,
`preview_backup` e `import_config`.

## Verificação

| Req. | Evidência |
|---|---|
| R1–R2, R4–R8 | testes de `store.rs` e `settings.rs` |
| R3, R9 | uso real no macOS |
