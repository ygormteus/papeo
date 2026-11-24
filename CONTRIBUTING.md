# Contribuindo com o Papeo

O Papeo é feito em Rust com [Tauri 2](https://tauri.app) e interface em Svelte 5. Cada serviço abre na webview nativa do sistema (WKWebView no macOS, WebView2 no Windows) em vez de um Chromium embutido, com sessão isolada por conta.

## Pré-requisitos

- [Rust](https://rustup.rs) estável
- Node 24 e pnpm 10
- macOS: Xcode Command Line Tools (`xcode-select --install`)
- Windows: [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) e WebView2

Detalhes por sistema em [Tauri — Prerequisites](https://tauri.app/start/prerequisites/). Linux ainda não é suportado.

## Rodar

```sh
pnpm install
pnpm tauri dev      # app nativo com hot reload
pnpm dev            # só a interface no navegador, com backend simulado (http://localhost:1420)
```

`http://localhost:1420/?demo` carrega dados fictícios (usados nas capturas do README).

## Checagens

Rode antes de abrir um PR:

```sh
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test -p papeo-core        # modelo e persistência (roda em qualquer SO)
pnpm check                      # tipos do frontend (svelte-check)
pnpm test                       # shim de notificações injetado nos sites
pnpm build                      # build da interface
```

## Estrutura

| Caminho | Conteúdo |
|---|---|
| `src/` | Interface (Svelte 5 + Vite): barra lateral, topbar, Início, Configurações, diálogos; `Popover.svelte` é a mini janela da barra de menus/bandeja |
| `src-tauri/src/` | App nativo: `webviews` (serviços), `commands` (interface), `notify` (sites), `downloads`, `memory`, `tray`, `popover` (mini janela), `reply` (resposta rápida), `notifications` (notificações nativas com "Responder"), `menu` (macOS), `lock`, `updater`, `sessions`, `state` |
| `src-tauri/src/notification_shim.js` | Script injetado nos sites: notificações, sinal de vida, abrir conversa e resposta rápida |
| `src-tauri/capabilities/` | Permissões da interface (`default`), da mini janela (`popover`) e dos sites (`services`) |
| `crates/papeo-core/` | Modelo, configurações, `services.json`, backup e regras puras, sem dependência do Tauri |

## Regras importantes

- **Não segure o lock do estado** (`AppState::lock()`) durante `add_child`, `is_focused`, `with_webview` com espera ou métodos de bandeja/menu: eles esperam a thread principal e causam deadlock.
- **Comando novo** precisa entrar em `COMMANDS` (`src-tauri/build.rs`), na capability certa e no mock (`src/lib/devMock.ts`).
- **Webviews `svc-*` são sites remotos:** só `service_notify` e `service_status` na capability `services`; o backend identifica o serviço pelo rótulo da webview.
- **`id`/`session` de serviço** só no formato gerado pelo app (viram rótulo de webview e nome de pasta).
- Mudanças em `services.json` (`crates/papeo-core/src/store.rs`, `settings.rs`) precisam continuar lendo arquivos antigos; cubra com teste. Um tipo de serviço novo (ex.: Discord na 0.2.0) não é lido por versões anteriores: quem voltar de versão perde a lista (o arquivo fica salvo como `.corrompido`).

## Modelo de segurança

- **ACL de comandos.** Todos os comandos passam pelo manifesto de `src-tauri/build.rs`. A interface (`ui`) recebe os comandos pela capability `default`; a mini janela (`popover`), só ler o estado, responder, abrir o app, não perturbe e bloquear. Os sites recebem **apenas** `service_notify` e `service_status`, então um site não fala em nome de outro nem aciona outros comandos.
- **Resposta rápida.** O Papeo pede ao script injetado (`__papeoReply`) para abrir a conversa exata, conferir o nome no cabeçalho, escrever no campo do próprio site e enviar; nunca escreve por cima de um rascunho, recusa nomes de conversa repetidos e tem prazo: depois dele (ou se o Papeo cancelar) nada é escrito nem enviado, para uma nova tentativa não duplicar a mensagem. Como o site roda no mesmo contexto do script, ele poderia forjar o resultado "enviado" (efeito: só uma entrada errada nas conversas recentes, na memória); o texto só é entregue se a página aberta for do próprio site do serviço. O andamento é lido da página (`eval_with_callback`), sem comando novo para os sites. Só WhatsApp e Telegram, só desbloqueado e, na notificação, só se a prévia não estiver oculta. As conversas recentes ficam só na memória.
- **Origem.** Notificações, "desconectado" e microfone/câmera só valem quando a página aberta é do próprio site do serviço (`same_site`, comparação aproximada de domínio).
- **Arquivos.** Nomes de download passam por `safe_file_name` e nunca sobrescrevem arquivos existentes.
- **CSP** da interface em `src-tauri/tauri.conf.json`: só recursos locais e IPC.

Limites conhecidos: os dados das sessões não têm criptografia própria (use FileVault/BitLocker); o backup é um JSON legível (sem logins nem conversas); notificações disparadas pelo service worker do site (push) não passam pelo Papeo; a resposta rápida depende dos seletores do WhatsApp Web/Telegram Web (se o site mudar, o Papeo avisa e oferece abrir a conversa). No macOS, "Responder" na notificação só existe no app instalado (`.app`); fora dele as notificações usam o plugin, sem resposta.

## Onde ficam os dados

| Sistema | Pasta |
|---|---|
| macOS | `~/Library/Application Support/io.github.ygormteus.papeo/` (sessões no armazenamento do WebKit, por conta) |
| Windows | `%APPDATA%\io.github.ygormteus.papeo\` |

`services.json` guarda serviços e configurações; `sessions/<id>/`, logins, cookies e cache de cada conta.

No macOS, `webcrypto-master.key` (permissão 0600) guarda a chave com que o WebKit cifra as chaves criptográficas dos sites (o login do WhatsApp Web). Por padrão o WebKit a guardaria nas Chaves, liberada só para o build exato do app: sem assinatura com ID de equipe da Apple, cada atualização pediria a senha das Chaves. O Papeo a entrega pelo delegate privado `webCryptoMasterKey:` do `WKWebsiteDataStore` (macOS 15+; `src-tauri/src/webcrypto.rs`). Na primeira abertura com esse recurso, a chave antiga é lida uma vez das Chaves, para os logins continuarem valendo.

## Gerar instaladores

- **Local:** `pnpm build:local` gera para o sistema em uso, sem os pacotes de atualização. `pnpm tauri build` também gera os pacotes de atualização e exige a chave privada em `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- **GitHub Actions:** rode o workflow **Build** manualmente (instaladores em **Artifacts**) ou crie uma tag `v*`, que gera um release em rascunho com `.dmg` (Apple Silicon e Intel) e `.exe` (Windows).
- **Assinatura:** macOS com assinatura ad-hoc (sem notarização); Windows ainda sem assinatura. Com o repositório público, um job separado gera a atestação de procedência dos instaladores e dos pacotes de atualização.

Conferir um instalador baixado (com o [GitHub CLI](https://cli.github.com)):

```sh
gh attestation verify Papeo_0.1.0_aarch64.dmg -R ygormteus/papeo
```

## Publicar uma versão

1. Atualize `version` em `package.json`, `src-tauri/tauri.conf.json` e nos dois `Cargo.toml`.
2. Escreva `release-notes/<versão>.md` (obrigatório) com as seções `## Novidades`, `## Melhorias` e `## Correções` e itens `- texto` (aceita `**negrito**` e `` `código` ``). Escreva para quem usa, não para quem programa. O app mostra essas notas antes de atualizar (pelo `latest.json`) e uma vez depois de atualizar (embutidas no build); o workflow também as coloca no texto do release, junto com `.github/release-template.md`.
3. Crie e envie a tag: `git tag v0.2.0 && git push origin v0.2.0`.
4. O workflow **Build** cria o release em rascunho com os instaladores, os pacotes de atualização e o `latest.json`, e confere se o `latest.json` tem as três plataformas. Se a conferência falhar, use **Re-run** só no job da plataforma que faltou e depois reexecute `check-updater-json`.
5. Revise e **publique** o rascunho. Só então os apps instalados enxergam a versão nova.

A chave privada das atualizações fica nos secrets do repositório; a pública, em `plugins.updater.pubkey` de `tauri.conf.json`. Sem a chave privada, os apps instalados deixam de aceitar atualizações.
