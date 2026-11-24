# Contexto do Papeo

Fatos duráveis e decisões com justificativa.

## Decisões

- **Tauri 2 com multiwebview (`features = ["unstable"]`).** Uma janela `main` com a webview `ui` (interface Svelte, `auto_resize`) e uma webview filha por serviço, posicionada sobre a área de conteúdo. Webview do sistema em vez de Chromium embutido é o que reduz a RAM em relação ao Ferdium/Electron.
- **A interface informa a área do conteúdo** (`set_content_area`, via `ResizeObserver`) e o Rust posiciona a webview ativa ali. As webviews ficam por cima do HTML: qualquer modal chama `set_overlay(true)` para ocultar a webview ativa. Popovers da barra lateral precisam ficar dentro dela.
- **Comandos `async`.** `Window::add_child` despacha para a thread principal e espera; comando síncrono ou evento de menu na thread principal causaria deadlock. Eventos de menu fazem `async_runtime::spawn`. Nunca segurar o `Mutex` do estado ao criar webview.
- **Sessão isolada por serviço:** `data_directory` (Windows/Linux) e `data_store_identifier` de 16 bytes (macOS 14+, por isso `minimumSystemVersion` 14.0). No Windows, cada pasta de dados cria um processo de navegador WebView2 próprio: custa RAM, mas permite duas contas do mesmo mensageiro.
- **User-agent Safari em macOS/Linux:** WKWebView não traz o token "Safari" e o WhatsApp Web recusa o navegador. No Windows, o UA do Edge/WebView2 já é aceito.
- **Notificações nativas:** `notification_shim.js` é injetado (`initialization_script`) e troca `window.Notification`, `permissions.query` e `ServiceWorkerRegistration.showNotification` por chamadas a `service_notify`. Os comandos do app têm manifesto ACL (`build.rs`); a capability `services` libera só `allow-service-notify` para webviews `svc-*` em `http(s)://*`. Limitações aceitas: qualquer site aberto dentro da webview do serviço pode notificar em nome dele (só esse comando, com o limite de frequência); notificações disparadas de dentro do service worker (push) não passam pelo shim; `is_focused()` nunca pode ser chamado com o lock do estado seguro (espera a thread principal). Ao criar um comando novo, inclua-o em `COMMANDS` e na capability `default`, senão a interface recebe "not allowed". Sem notificação quando o serviço está ativo e a janela focada; no mínimo 1,5 s entre notificações do mesmo serviço. Clicar na notificação só traz o app para frente (o plugin não expõe clique no desktop).
- **Várias contas:** cada serviço já tem sessão própria; adicionar o mesmo tipo de novo cria "WhatsApp 2" (`papeo_core::unique_name`).
- **Personalização:** `customize_service` substituiu `rename_service` e grava nome, `color` (`#rrggbb`, validada em `papeo_core::normalize_color`) e `emoji` (emoji ou até 2 letras, `normalize_emoji`). Sem cor/emoji, o ícone usa a cor e o símbolo do tipo. O Chromium headless da LXC não tem fonte de emoji: nas capturas, emojis aparecem como quadrados.
- **Não lidas pelo título** (`on_document_title_changed` + `papeo_core::parse_unread`): sem injetar JS nos sites. Serviço hibernado não atualiza o contador.
- **`papeo-core` sem Tauri** para os testes rodarem em qualquer máquina (a LXC não tem WebKitGTK).
- **Menu nativo só no macOS** (atalhos `⌘0–9`, `⌘R`, `⌘K` funcionam mesmo com o foco dentro do serviço; o menu padrão mantém Editar/copiar-colar). No Windows, os atalhos só funcionam com o foco na interface.

## Verificação na LXC (sem sudo, sem WebKitGTK)

O app não compila para Linux aqui; a checagem de tipos é cruzada:

- Windows: `RC_x86_64_pc_windows_msvc=$PWD/work/fake-rc.sh cargo clippy -p papeo --target x86_64-pc-windows-msvc -- -D warnings` (o stub finge ser o `llvm-rc`; não gera recurso real).
- macOS: `CC_aarch64_apple_darwin=$PWD/work/zig-cc-macos.sh cargo clippy -p papeo --target aarch64-apple-darwin -- -D warnings` (zig instalado via `uv tool install ziglang`; o wrapper remove `-arch`, `--target` e `-mmacosx-version-min`, e troca o `notify.m` do `mac-notification-sys`, que exige o SDK Cocoa, por `work/empty.c`).
- Os scripts ficam em `work/` (ignorado pelo Git); recrie-os se sumirem.
- Interface: `pnpm dev` usa `src/lib/devMock.ts` (mock do IPC) fora do Tauri.
