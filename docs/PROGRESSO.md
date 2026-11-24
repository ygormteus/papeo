# Progresso

## 2026-11-24 — Privacidade (10 itens da análise)

- **Objetivo:** apagar sessões de verdade (macOS incluído), restringir notificação/câmera/microfone ao site do serviço, prévia das notificações, "sair da conta e apagar dados", sessões antigas, bloqueio com Touch ID/Windows Hello, proteção contra captura de tela, aviso de http e documentação de privacidade.
- **Raiz/branch:** `/srv/projetos/papeo`, `main`, base `221f3c6`.
- **Estado:** no commit "feat: privacidade, pacotes 1–3 e índices de código" sobre `221f3c6`, com os pacotes 1–3 e RepoWise/Graphify (push pedido pelo usuário).
- **Revisão independente:** sem bloqueante; 3 importantes (teclado atrás da tela de bloqueio, foco na webview escondida, apagar sessões sem confirmação) e 7 menores corrigidos; re-checagem aprovada (README não reconferido).
- **Evidência (LXC, estado final):** `cargo fmt --all -- --check` 0; `cargo test -p papeo-core` 0 (27 testes, inclui `same_site`, prévia e `pending_removal`); `pnpm check` 0; `pnpm build` 0; `pnpm test` 0; clippy `-D warnings` Windows 0, macOS 0, core 0. Mock 1280×820 tema claro: seção Privacidade; bloquear → tela de bloqueio com `.shell` inert e foco em "Desbloquear"; desbloquear volta.
- **Não verificado:** app nativo — Touch ID/Windows Hello reais, `remove_data_store` no macOS, apagar pasta da sessão no Windows com WebView2, `content_protected` em chamada real, foco do teclado em WebView2 escondido, cancelamento do Hello por prazo.
- **Pendente:** matriz visual completa (tema escuro, barra recolhida, alertas e aviso http na Topbar).

## 2026-11-24 — Pacotes 1–3: mídia, arquivos, uso diário e confiabilidade

- **Objetivo:** áudio/câmera, arrastar e baixar arquivos, não perturbe, zoom, reordenar, bandeja/autostart, memória por serviço, status (desconectado/não responde), backup.
- **Raiz/branch:** `/srv/projetos/papeo`, `main`, base `221f3c6`.
- **Estado:** no mesmo commit da entrega de privacidade (acima).
- **Revisão independente:** sem achado bloqueante; corrigidos nome automático longo (invalidava `services.json`), check de DND da bandeja, falso "não responde" ao reexibir, downloads presos ao hibernar, colisão de nomes em downloads simultâneos, `safe_file_name` (bidi, extensão, COM¹–³) e `LSFileQuarantineEnabled`.
- **Evidência (LXC, estado final):** `cargo fmt --all -- --check` 0; `cargo test -p papeo-core` 0 (23 testes); `pnpm test` 0; `pnpm check` 0; `pnpm build` 0; `cargo clippy -D warnings` cruzado Windows 0, macOS 0, core `--all-targets` 0. Visual com mock (1280×820, tema claro): Início, Configurações e menu de contexto.
- **RepoWise/Graphify:** configurados a partir do modelo da LXC (`scripts/repowise*.{sh,py}`, `scripts/graphify.sh`, `.mcp.json`, `.codex/config.toml`, `.graphifyignore`, hook `post-commit` local, `AGENTS.md`/`CLAUDE.md`, `docs/repowise.md`). `repowise.sh init` 0; testes de proteção (`--write-seal`, `--read-seal`, post-commit) 0; `graphify.sh` 0 (595 nós, 1193 arestas); `repowise.sh search unique_path` encontrou `util.rs`.
- **Pendente:** matriz visual (tema escuro, barra recolhida, alertas da Topbar).
- **Não verificado:** app nativo (macOS/Windows): gravação de áudio, arrastar arquivos, downloads, bandeja, autostart, memória real, detecção de deslogado/travado e clique na notificação; notarização com entitlements.


## 2026-11-24 — Cor e emoji por serviço

- **Objetivo:** diferenciar contas do mesmo mensageiro (ex.: "Pessoal" e "Trabalho").
- **Estado:** commit "feat: cor e emoji por serviço" na `main`, sobre `17e608f`; push confirmado em seguida pelo `git status -sb`.
- **Evidência:** `cargo test -p papeo-core` (8 testes, inclui validação de cor/emoji/nome), clippy cruzado Windows/macOS sem avisos, `pnpm check`, `pnpm build`, `pnpm test`. No navegador com mock: diálogo Personalizar, duas contas renomeadas com cores diferentes na barra lateral e na tabela.
- **Não verificado:** emoji renderizado (sem fonte de emoji na LXC) e app nativo.


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
