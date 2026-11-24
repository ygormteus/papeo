# Papeo

Mensageiros num só lugar, leve e rápido. Escrito em Rust com [Tauri 2](https://tauri.app).

Uma alternativa enxuta ao Ferdium: cada serviço (WhatsApp, Telegram…) roda numa webview nativa do sistema (WKWebView no macOS, WebView2 no Windows), com sessão isolada e hibernação dos serviços inativos para economizar RAM.

## Status

Em desenvolvimento inicial. Já funciona:

- barra lateral com os serviços, badge de não lidas (lido do título da página) e badge no ícone do app;
- WhatsApp e Telegram por padrão, mais qualquer site https;
- várias contas do mesmo mensageiro (ex.: dois WhatsApp), cada uma com sessão isolada — pelo botão direito → **Adicionar outra conta**;
- notificações nativas do sistema, com interruptor por serviço; não notifica o serviço que você está vendo;
- hibernação manual e automática por serviço (descarrega a webview da memória);
- atalhos `⌘/Ctrl+0…9`, busca `⌘/Ctrl+K`, novo serviço `⌘/Ctrl+N`;
- tema claro/escuro, barra lateral recolhível;
- links externos abrem no navegador padrão.

Ainda falta: clicar na notificação abrir o serviço certo, contagem de RAM por serviço, reordenar arrastando, atalhos com o foco dentro do serviço no Windows.

## Plataformas

- macOS 14+ (Apple Silicon e Intel)
- Windows 10/11 (WebView2)

## Desenvolvimento

Pré-requisitos: [Rust](https://rustup.rs), Node 24 e pnpm 10. No Windows, também o WebView2 (já vem no Windows 11).

```sh
pnpm install
pnpm tauri dev      # app nativo com hot reload
pnpm dev            # só a interface no navegador, com backend simulado
```

Checagens:

```sh
pnpm check                      # tipos do frontend (svelte-check)
cargo test -p papeo-core        # modelo e persistência (roda em qualquer SO)
pnpm test                       # shim de notificações injetado nos sites
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

Build local de instalador: `pnpm tauri build`. Os instaladores de macOS (`.dmg`) e Windows (`.msi`/`.exe`) também saem pelo GitHub Actions: rode o workflow **Build** manualmente ou crie uma tag `v*` (gera um release em rascunho).

## Estrutura

| Caminho | Conteúdo |
|---|---|
| `src/` | Interface (Svelte 5 + Vite): barra lateral, topbar, Início, diálogos |
| `src-tauri/` | App nativo: janela, webviews dos serviços, comandos, menu, hibernação |
| `crates/papeo-core/` | Modelo dos serviços e `services.json`, sem dependência do Tauri |

Os dados ficam no diretório de configuração do app (`services.json`) e as sessões em `sessions/<id>` no diretório de dados (no macOS, no `WKWebsiteDataStore` identificado pela sessão).

## Segurança

Os comandos do app passam por um manifesto de ACL (`src-tauri/build.rs`). A interface (`ui`) recebe todos pela capability `default`. Os sites dos serviços (`svc-*`, origem remota) recebem **apenas** `service_notify` pela capability `services`; o backend identifica o serviço pelo rótulo da webview, então um site não consegue notificar em nome de outro nem acionar outros comandos.
