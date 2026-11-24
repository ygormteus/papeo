# Papeo

Mensageiros num só lugar, leve e rápido. Escrito em Rust com [Tauri 2](https://tauri.app).

Uma alternativa enxuta ao Ferdium: cada serviço (WhatsApp, Telegram…) roda numa webview nativa do sistema (WKWebView no macOS, WebView2 no Windows), com sessão isolada e hibernação dos serviços inativos para economizar RAM.

## Status

Em desenvolvimento inicial. Já funciona:

- barra lateral com os serviços, badge de não lidas (lido do título da página) e badge no ícone do app;
- WhatsApp e Telegram por padrão, mais qualquer site https;
- sessão isolada por serviço (duas contas do mesmo mensageiro);
- hibernação manual e automática por serviço (descarrega a webview da memória);
- atalhos `⌘/Ctrl+0…9`, busca `⌘/Ctrl+K`, novo serviço `⌘/Ctrl+N`;
- tema claro/escuro, barra lateral recolhível;
- links externos abrem no navegador padrão.

Ainda falta: notificações nativas, contagem de RAM por serviço, reordenar arrastando, atalhos com o foco dentro do serviço no Windows.

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

As webviews dos serviços carregam sites remotos e **não** recebem nenhuma permissão de IPC: só a webview da interface (`ui`) está na capability `default`.
