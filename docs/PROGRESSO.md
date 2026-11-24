# Progresso

## 2026-11-24 — Shell inicial

- **Objetivo:** primeiro marco do Papeo (alternativa leve ao Ferdium): shell Tauri com barra lateral, topbar e Início no estilo do "Studio Admin", WhatsApp e Telegram.
- **Raiz/branch:** `/srv/projetos/papeo`, `main` (repositório privado `ygormteus/papeo`).
- **Estado:** implementado, **não commitado** além do commit inicial.

### Evidência (na LXC)

- `cargo test -p papeo-core`: 4 testes ok.
- `cargo clippy -p papeo -p papeo-core -- -D warnings` cruzado para `x86_64-pc-windows-msvc` e `aarch64-apple-darwin`: sem avisos.
- `cargo fmt --all -- --check`, `pnpm check` (0 erros/avisos), `pnpm build`: ok.
- Interface no navegador com mock: Início claro e diálogo "Adicionar serviço" escuro, 1280×820.

### Não testado

- App nativo rodando (macOS/Windows): criação e troca das webviews, login no WhatsApp/Telegram, isolamento de sessão, badges, hibernação automática, menu e barra de título sobreposta no macOS.
- Workflow `.github/workflows/release.yml` ainda não executado.
- Matriz visual completa (palette, menu de contexto, renomear, remover, barra recolhida).

### Próximos passos

1. Rodar `pnpm tauri dev` no Mac e validar o fluxo acima.
2. Notificações nativas (shim de `Notification` injetado + plugin de notificação).
3. RAM por serviço no Início; reordenar por arrastar; atalhos no Windows com foco no serviço.
4. Remover serviço no macOS ainda não apaga o `WKWebsiteDataStore` (só a pasta no Windows/Linux).
