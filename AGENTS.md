# Papeo — regras do projeto

As regras globais da LXC valem aqui; este arquivo registra só o que é próprio
do Papeo. Contexto durável em `docs/CONTEXTO.md`; continuidade em
`docs/PROGRESSO.md`.

## Consulta de impacto (RepoWise + Graphify)

Antes de alterar contratos compartilhados, consulte impacto com RepoWise e
Graphify e confirme no código com `rg`:

- comandos Tauri e ACL: `src-tauri/build.rs`, `src-tauri/capabilities/`,
  `src-tauri/permissions/`, `src/lib/api.ts`, `src/lib/devMock.ts`;
- estado e locks: `src-tauri/src/state.rs`, `webviews.rs`, `notify.rs`,
  `tray.rs`, `downloads.rs`;
- formato persistido: `crates/papeo-core/src/store.rs`, `service.rs`,
  `settings.rs`;
- script injetado nos sites: `src-tauri/src/notification_shim.js`.

Frescor: antes da primeira consulta da tarefa (e após edições relevantes),
rode `sh scripts/repowise.sh update` e `sh scripts/graphify.sh` e aguarde
saída zero. Ajuste pequeno em arquivo conhecido fora dessa lista segue com
`rg`. Uso e limites em `docs/repowise.md`.

## Invariantes

- Nunca segurar `AppState::lock()` durante `add_child`, `is_focused`,
  `with_webview`+espera ou métodos de tray/menu (deadlock com a thread
  principal).
- Webviews `svc-*` são sites remotos: só `service_notify` e `service_status`
  na capability `services`; o backend identifica o serviço pelo label.
- `id`/`session` de serviço só no formato gerado pelo app (viram rótulo de
  webview e nome de pasta).

## Gates

`cargo fmt --all -- --check`, `cargo test -p papeo-core`, `pnpm check`,
`pnpm build`, `pnpm test` e clippy `-D warnings` cruzado para Windows e
macOS (ver `docs/PROGRESSO.md`). O app nativo não roda na LXC.
