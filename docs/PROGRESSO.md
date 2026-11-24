# Progresso

## 2026-11-24 — Notificações nativas e várias contas

- **Objetivo:** notificações nativas dos serviços e mais de uma conta por mensageiro.
- **Raiz/branch:** `/srv/projetos/papeo`, `main`. Base: `c7b7d62` (shell inicial, já no GitHub).
- **Estado:** implementado e enviado à `main` (commit "feat: notificações nativas e várias contas por serviço"); revisão independente aprovada após corrigir um deadlock.

### Evidência (na LXC)

- `cargo test -p papeo-core`: 7 testes ok (inclui nomes "WhatsApp 2", `clip` e `services.json` antigo sem `notifications`).
- `pnpm test` (shim de Notification no Node): ok.
- `cargo clippy -p papeo -- -D warnings` cruzado para Windows e macOS: sem avisos; o build script validou o manifesto ACL e as capabilities.
- `cargo fmt --check`, `pnpm check`, `pnpm build`: ok.

### Não testado

- App nativo (macOS/Windows): notificação real do WhatsApp/Telegram, permissão de notificação do sistema, IPC a partir da página remota (a CSP do WhatsApp deve forçar o fallback `postMessage` do Tauri), login em duas contas ao mesmo tempo.
- Workflow de build no GitHub Actions.

### Próximos passos

1. Rodar `pnpm tauri dev` no Mac: logar em dois WhatsApp e mandar mensagem de outro aparelho com o Papeo em segundo plano.
2. Clique na notificação abrir o serviço; RAM por serviço; reordenar por arrastar.
3. Remover serviço no macOS ainda não apaga o `WKWebsiteDataStore`.
