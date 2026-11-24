# Contexto do Papeo

Fatos duráveis e decisões com justificativa.

## Decisões

- **Tauri 2 com multiwebview (`features = ["unstable"]`).** Uma janela `main` com a webview `ui` (interface Svelte, `auto_resize`) e uma webview filha por serviço, posicionada sobre a área de conteúdo. Webview do sistema em vez de Chromium embutido é o que reduz a RAM em relação ao Ferdium/Electron.
- **A interface informa a área do conteúdo** (`set_content_area`, via `ResizeObserver`) e o Rust posiciona a webview ativa ali. As webviews ficam por cima do HTML: qualquer modal chama `set_overlay(true)` para ocultar a webview ativa. Popovers da barra lateral precisam ficar dentro dela.
- **Comandos `async`.** `Window::add_child` despacha para a thread principal e espera; comando síncrono ou evento de menu na thread principal causaria deadlock. Eventos de menu fazem `async_runtime::spawn`. Nunca segurar o `Mutex` do estado ao criar webview.
- **Sessão isolada por serviço:** `data_directory` (Windows/Linux) e `data_store_identifier` de 16 bytes (macOS 14+, por isso `minimumSystemVersion` 14.0). No Windows, cada pasta de dados cria um processo de navegador WebView2 próprio: custa RAM, mas permite duas contas do mesmo mensageiro.
- **User-agent Safari em macOS/Linux:** WKWebView não traz o token "Safari" e o WhatsApp Web recusa o navegador. No Windows, o UA do Edge/WebView2 já é aceito.
- **Não lidas pelo título** (`on_document_title_changed` + `papeo_core::parse_unread`): sem injetar JS nos sites. Serviço hibernado não atualiza o contador.
- **`papeo-core` sem Tauri** para os testes rodarem em qualquer máquina (a LXC não tem WebKitGTK).
- **Menu nativo só no macOS** (atalhos `⌘0–9`, `⌘R`, `⌘K` funcionam mesmo com o foco dentro do serviço; o menu padrão mantém Editar/copiar-colar). No Windows, os atalhos só funcionam com o foco na interface.

## Verificação na LXC (sem sudo, sem WebKitGTK)

O app não compila para Linux aqui; a checagem de tipos é cruzada:

- Windows: `RC_x86_64_pc_windows_msvc=$PWD/work/fake-rc.sh cargo clippy -p papeo --target x86_64-pc-windows-msvc -- -D warnings` (o stub finge ser o `llvm-rc`; não gera recurso real).
- macOS: `CC_aarch64_apple_darwin=$PWD/work/zig-cc-macos.sh cargo clippy -p papeo --target aarch64-apple-darwin -- -D warnings` (zig instalado via `uv tool install ziglang`; o wrapper remove `-arch`, `--target` e `-mmacosx-version-min`).
- Os scripts ficam em `work/` (ignorado pelo Git); recrie-os se sumirem.
- Interface: `pnpm dev` usa `src/lib/devMock.ts` (mock do IPC) fora do Tauri.
