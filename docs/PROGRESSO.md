# Progresso

## 2026-11-24 — Testes no Mac: remoção, memória, contas reabertas, recarregamento

- **Raiz/branch:** `~/Developer/PROJETOS - GIT/papeo` (Mac), `main`, base `a083cd2`.
- **Corrigido:** remover serviço, "sair e apagar dados" e restaurar backup não faziam nada: o `{@const}` do Svelte 5 é derivado e virava `null` ao fechar o diálogo antes de ler o id (`App.svelte`).
- **Memória:** no macOS a medição passou a usar o footprint (`proc_pid_rusage`, igual ao Monitor de Atividade); o RSS mostrava menos da metade. WhatsApp Web: ~0,9–1 GB ao abrir, ~2–2,4 GB após 10–20 min; o Papeo em si ~50 MB.
- **Novo:** `Config.open` (local, fora do backup) reabre escondidos ao iniciar os serviços que estavam carregados; `Settings.reloadAboveMb` (padrão 2048, 0 = nunca) recarrega serviço acima do limite, fora de vista, parado há 10 min, sem download, no máximo 1×/h (Configurações → Sistema).
- **RepoWise/Graphify no Mac:** `flock` via Homebrew, venv 0.60.1, `graphify.sh` sem caminho da LXC, hook `post-commit`; testes de proteção 0; MCP responde.
- **Evidência (Mac):** `cargo fmt --check` 0; `cargo test -p papeo-core` 0 (31); clippy `-D warnings` macOS (core `--all-targets` e app) 0; `pnpm check`/`build`/`test` 0; `pnpm build:local` 0. App nativo: remover Telegram ok (usuário); reiniciar reabriu as 2 contas (2 WebContent, ~900 MB em 35 s).
- **Não verificado:** clippy cruzado para Windows (sem `rustup` no Mac); recarregamento automático disparando de fato; notificações de mensagens que chegam durante o recarregamento.

## 2026-11-24 — Atualização com um clique (tauri-plugin-updater)

- **Objetivo (decisões do usuário):** atualizador do Tauri; verificação ligada por padrão com aviso na primeira abertura; instalação só com clique.
- **Raiz/branch:** `/srv/projetos/papeo`, `main`, base `3e54ac8`; commit `67f815c` com push (junto da entrega de README/licença abaixo).
- **Feito:** `Settings.updates` (core, compatível com arquivos antigos, teste `updates_wait_for_notice`); `updater.rs`; comandos `check_update`, `install_update`, `set_updates` (ACL + capability `default`); `Snapshot.update`/`appVersion`; aviso em `App.svelte` (`ConfirmDialog` ganhou `cancelLabel`, `danger`, `oncancel`); Configurações → Sistema (interruptor, versão, "Verificar agora"/"Atualizar e reiniciar"); cartão e ponto na barra lateral; mock. `tauri.conf.json`: `createUpdaterArtifacts`, `plugins.updater`, `targets` `app/dmg/nsis`. Workflow: job `create-release` (idempotente) → builds com `releaseId` → `check-updater-json` (3 plataformas) e `attest` separado (único com `id-token`); cache do Rust fora de tags. README: atualização, publicação de versão, chave.
- **Consulta de impacto:** RepoWise e Graphify atualizados (saída 0) e consultados para `settings.rs`, `state.rs`, `build.rs`, `api.ts`, `devMock.ts`.
- **Revisão independente:** sem bloqueante; corrigidos timeout (estado preso), MSI×NSIS (MSI removido), corrida de rascunhos, `id-token` só no job de atestação, `.app.tar.gz` na atestação, nova tentativa após falha com relógio de parede, backup não traz `updates`, textos de IP. Re-checagem ok. Mantidos: Esc = manter ligada (decisão do usuário); ícone da bandeja pode ficar "fantasma" no Windows após o `exit(0)` do instalador.
- **Evidência (LXC, estado final):** `cargo fmt --all -- --check` 0; `cargo test -p papeo-core` 0 (29); clippy `-D warnings` core 0, Windows 0, macOS 0; `pnpm check` 0; `pnpm build` 0; `pnpm test` 0; YAML válido; `git diff --check` 0. Visual no mock (ui-checker, conferido nas capturas): aviso, Esc/Desligar/Manter ligada, Sistema, disponível, baixando 40%, ponto com barra recolhida, 960×600, tema escuro; console sem erros. Tab após o último botão do `<dialog>` vai ao navegador e volta (nativo do `showModal`, igual aos outros diálogos).
- **Não verificado:** app nativo (consulta, download, instalação e reinício em macOS e Windows); workflow no Actions (execução manual sem release com `releaseId` vazio na `tauri-action` do SHA fixado; tag gerando `latest.json` com as 3 plataformas); dark do passo "baixando".
- **Secrets:** `TAURI_SIGNING_PRIVATE_KEY` e `_PASSWORD` cadastrados no GitHub em 2026-11-24 (autorizado pelo usuário); chave pública do `tauri.conf.json` conferida contra `~/.tauri/papeo-updater.key.pub`. Licença: usuário confirmou GPL-3.0-or-later.
- **Backup:** chave e senha copiadas pelo usuário para o computador dele em 2026-11-24 (cópia temporária apagada da LXC).
- **Mac (2026-11-24, clone em `~/Developer/PROJETOS - GIT/papeo`):** `pnpm build:local` 0 (~147 s; Rust 1.99.1, Node 24.13.1, pnpm 10.29.3); `codesign` `Signature=adhoc`, `flags=0x10002(adhoc,runtime)`, `--verify --deep --strict` válido; entitlements de microfone e câmera presentes; `spctl` rejeita (esperado sem notarização); app abriu e ficou de pé ~20 s, sem crash. Interface, microfone, câmera e notificações ainda não testados à mão.
- **Pendente:** teste manual no Mac (lista acima); repositório público (as atualizações dependem disso).
- **Próximo passo:** o desenvolvimento continua no Mac (o app nativo roda lá). Na LXC, `git pull` antes de retomar; clippy cruzado para Windows só aqui ou no Actions.

## 2026-11-24 — README para usuários e revisão de segurança do repositório

- **Objetivo:** README com instalação (incl. app sem assinatura), primeiros passos, atalhos, dados, problemas comuns; conferir segurança.
- **Raiz/branch:** `/srv/projetos/papeo`, `main`, base `3e54ac8`; no mesmo commit da atualização com um clique.
- **Feito:** README reescrito e conferido contra o código (atalhos em `App.svelte`/`menu.rs`, pastas em `lib.rs`, seções de `Settings.svelte`); removida a limitação antiga "remover no macOS não apaga o `WKWebsiteDataStore`" (`sessions.rs` chama `remove_data_store`). `scripts/__pycache__/*.pyc` saiu do Git e entrou no `.gitignore`.
- **Assinatura e publicação (decisões do usuário):** sem conta Apple Developer por ora → macOS com assinatura ad-hoc (`signingIdentity: "-"`); Windows pela SignPath Foundation (grátis, exige repositório público). O repositório vai ser público só depois de o usuário testar tudo. Licença GPL-3.0-or-later (`LICENSE`, `Cargo.toml`, `package.json`). Workflow: `permissions: {}` no topo e só o necessário no job, actions fixadas por SHA nas mesmas majors, `persist-credentials: false`, atestação `attest-build-provenance` que só roda com o repositório público.
- **Evidência:** YAML e `tauri.conf.json` válidos; `cargo test -p papeo-core` 0 (28 testes); `pnpm build` 0; `cargo metadata` com a licença nova.
- **Não verificado:** build no Actions; app com assinatura ad-hoc + Hardened Runtime abrindo e pedindo microfone/câmera num Mac (se falhar, testar `hardenedRuntime: false`); nomes dos instaladores citados no README.
- **Pendente:** inscrição na SignPath (após tornar o repositório público) e `signCommand`/integração no workflow; revisar antes de publicar o que hoje é interno (`AGENTS.md`, `CLAUDE.md`, `docs/PROGRESSO.md`, `.codex/config.toml`, `.mcp.json`, `scripts/repowise*`); actions em majors antigas (Node 20).

## 2026-11-24 — Logo "Balão P" e temas (claro, escuro, OLED)

- **Objetivo:** logo nova em todo o app; tema OLED; tema aplicado também aos sites; voltar a "seguir o sistema"; contraste e menu de contexto.
- **Raiz/branch:** `/srv/projetos/papeo`, `main`, base `9e130b0`.
- **Estado:** commit "feat: logo Balão P e tema OLED" na `main`, com push pedido pelo usuário.
- **Evidência (LXC, estado final):** `cargo fmt --all -- --check` 0; `cargo test -p papeo-core` 0 (28 testes, inclui `Appearance`); `pnpm check` 0; `pnpm build` 0; `pnpm test` 0; clippy `-D warnings` Windows 0, macOS 0, core 0. Mock 1280×820 em claro, escuro e OLED: Início, menu de contexto (uma linha), Adicionar serviço, Configurações (Aparência); fundo OLED `rgb(0,0,0)`; `papeo.theme` antigo migrado; botão da barra percorre Sistema → Claro → Escuro → OLED. Contraste: `--text-faint` saiu dos textos (2,6:1 no claro) para `--text-muted` (4,8:1).
- **Não verificado:** app nativo — ícone no Dock/barra de tarefas, silhueta na barra de menus do Mac, WhatsApp/Telegram seguindo o tema da janela.

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
