# Papeo

Mensageiros num só lugar, leve e rápido. Escrito em Rust com [Tauri 2](https://tauri.app).

Uma alternativa enxuta ao Ferdium: cada serviço (WhatsApp, Telegram…) roda numa webview nativa do sistema (WKWebView no macOS, WebView2 no Windows), com sessão isolada e hibernação dos serviços inativos para economizar RAM.

## Status

Em desenvolvimento inicial. Já funciona:

- barra lateral com os serviços, badge de não lidas (lido do título da página) e badge no ícone do app;
- WhatsApp e Telegram por padrão, mais qualquer site https;
- várias contas do mesmo mensageiro (ex.: dois WhatsApp), cada uma com sessão isolada — pelo botão direito → **Adicionar outra conta**;
- nome, cor, emoji e permissão de microfone/câmera por serviço (botão direito → **Personalizar…**);
- mensagens de voz e chamadas: microfone e câmera declarados no macOS (`Info.plist`/entitlements) e liberados por serviço (Permitir, Perguntar ou Bloquear);
- arrastar arquivos para dentro da conversa e baixar arquivos para uma pasta escolhida, com aviso e "Mostrar na pasta";
- notificações nativas com interruptor e silêncio temporário (1 h, 8 h) por serviço, **não perturbe** global e horário silencioso;
- voltar ao app logo após uma notificação abre o serviço dela;
- zoom por serviço (`⌘/Ctrl +`, `-`, `⇧0`), lembrado entre sessões;
- reordenar arrastando na barra lateral (ou "Mover para cima/baixo");
- bandeja/menu bar com contador, abrir com o sistema (escondido) e fechar mantendo o app ativo;
- memória por serviço no Início, com sugestão de hibernar os pesados e parados;
- aviso de serviço desconectado (tela de login) ou que não responde, com recarregar;
- backup e restauração da configuração (sem conversas nem logins);
- hibernação manual e automática por serviço;
- tema claro/escuro, barra lateral recolhível, links externos no navegador padrão.

Limitações conhecidas: no Windows, atalhos só funcionam com o foco na interface do Papeo (não dentro do serviço); remover um serviço no macOS não apaga o `WKWebsiteDataStore`; o clique na notificação é detectado pelo retorno do foco ao app (o plugin de notificação não informa o clique).

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
| `src/` | Interface (Svelte 5 + Vite): barra lateral, topbar, Início, Configurações, diálogos |
| `src-tauri/src/` | App nativo: `webviews` (serviços), `commands` (interface), `notify` (sites), `downloads`, `memory`, `tray`, `menu` (macOS), `state` |
| `src-tauri/src/notification_shim.js` | Script injetado nos sites: notificações e sinal de vida |
| `crates/papeo-core/` | Modelo, configurações, `services.json`, backup e regras puras, sem dependência do Tauri |

Os dados ficam no diretório de configuração do app (`services.json`) e as sessões em `sessions/<id>` no diretório de dados (no macOS, no `WKWebsiteDataStore` identificado pela sessão).

## Segurança

Os comandos do app passam por um manifesto de ACL (`src-tauri/build.rs`). A interface (`ui`) recebe todos pela capability `default`. Os sites dos serviços (`svc-*`, origem remota) recebem **apenas** `service_notify` e `service_status` pela capability `services`; o backend identifica o serviço pelo rótulo da webview, então um site não consegue falar em nome de outro nem acionar outros comandos.

Arquivos: nomes de download passam por `safe_file_name` (sem pastas nem nomes reservados) e nunca sobrescrevem; `id`/`session` de serviços vindos de backup ou de `services.json` editado à mão só são aceitos no formato gerado pelo app, porque viram rótulo de webview e nome de pasta.

Origem: notificações, "desconectado" e microfone/câmera só valem quando a página aberta na webview é do próprio site do serviço (`same_site`); outra página aberta dentro dele não fala em nome do serviço e tem a câmera e o microfone negados.

## Privacidade

- **Nada sai do computador por conta do Papeo.** Sem telemetria, relatório de erros, atualizador ou chamadas de rede próprias; a rede vem só dos sites que você adicionou.
- **Sessões isoladas** por conta. Remover um serviço ou usar "Sair da conta e apagar dados" apaga cookies, cache e conversas guardadas (pasta da sessão e, no macOS, o armazenamento do WebKit). O que não puder ser apagado na hora fica numa fila e é apagado na próxima abertura. "Sessões antigas" em Configurações → Privacidade remove logins de serviços que não estão mais na lista.
- **Prévia nas notificações:** completa, só remetente ou oculta. Bloqueado, o Papeo nunca mostra conteúdo.
- **Bloqueio** com Touch ID/senha do Mac ou Windows Hello: ao abrir, ao esconder a janela ou após um tempo sem uso. Enquanto bloqueado, nenhuma conversa aparece.
- **Captura de tela:** opção para a janela aparecer vazia em gravações e compartilhamento de tela.
- **Limites conhecidos:**
  - os dados das sessões e o `services.json` não têm criptografia própria; dependem da criptografia do disco (FileVault, BitLocker);
  - o backup exportado é um JSON legível (nomes e endereços dos serviços, sem logins nem conversas);
  - no Windows, o WebView2 segue as configurações de diagnóstico do Windows e o SmartScreen da Microsoft;
  - sites personalizados em `http://` não têm criptografia; o Papeo avisa ao adicionar e na barra superior.
  - a checagem de "mesmo site" é aproximada: em hospedagens compartilhadas (`*.github.io`, `*.vercel.app`) outro projeto do mesmo domínio conta como o mesmo site, e um iframe de terceiro dentro do serviço recebe a política de câmera/microfone do serviço (o WebView não informa a origem do frame);
  - o bloqueio protege contra quem usa o computador pela interface; quem consegue editar os arquivos do seu usuário pode desligá-lo no `services.json`;
  - o bloqueio automático é verificado a cada 15 s.
