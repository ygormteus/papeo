# Papeo

Seus mensageiros num só lugar, leve e rápido. Feito em Rust com [Tauri 2](https://tauri.app).

O Papeo é uma alternativa enxuta ao Ferdium. Cada serviço (WhatsApp, Telegram ou qualquer site) abre na webview nativa do sistema — WKWebView no macOS, WebView2 no Windows — em vez de um Chromium embutido. Cada conta tem sessão isolada, e os serviços parados podem hibernar para economizar memória.

> **Status:** em desenvolvimento inicial (versão 0.1). Funciona no dia a dia, mas ainda pode mudar bastante entre versões.

- [Instalação](#instalação)
- [Primeiros passos](#primeiros-passos)
- [Atalhos de teclado](#atalhos-de-teclado)
- [Recursos](#recursos)
- [Privacidade e segurança](#privacidade-e-segurança)
- [Onde ficam os dados](#onde-ficam-os-dados)
- [Problemas comuns](#problemas-comuns)
- [Desenvolvimento](#desenvolvimento)

## Instalação

### Requisitos

| Sistema | Versão | Observação |
|---|---|---|
| macOS | 14 (Sonoma) ou mais novo | Apple Silicon (`aarch64`) ou Intel (`x86_64`) |
| Windows | 10 ou 11 | Precisa do WebView2, que já vem no Windows 11 e na maioria dos Windows 10 atualizados ([download](https://developer.microsoft.com/microsoft-edge/webview2/)) |

Linux ainda não é suportado.

### Baixar

Os instaladores são gerados pelo GitHub Actions (workflow **Build**):

- **Versões publicadas:** página **Releases** do repositório.
- **Builds manuais:** aba **Actions** → execução do workflow **Build** → seção **Artifacts**.

| Sistema | Arquivo |
|---|---|
| macOS Apple Silicon (M1 ou mais novo) | `Papeo_<versão>_aarch64.dmg` |
| macOS Intel | `Papeo_<versão>_x64.dmg` |
| Windows | `Papeo_<versão>_x64-setup.exe` |

Baixe apenas deste repositório. Os instaladores **ainda não têm assinatura de desenvolvedor**: no macOS a assinatura é ad-hoc (sem conta Apple Developer, sem notarização) e no Windows não há assinatura. Por isso o sistema mostra um aviso na primeira abertura.

#### Conferir se o instalador é original

Com o repositório público, cada instalador gerado pelo GitHub Actions tem uma atestação de procedência, que prova que ele saiu deste repositório e deste workflow, sem alterações. Com o [GitHub CLI](https://cli.github.com) instalado:

```sh
gh attestation verify Papeo_0.1.0_aarch64.dmg -R ygormteus/papeo
```

Se o comando não terminar com sucesso, não instale o arquivo.

### macOS

1. Abra o `.dmg` e arraste o **Papeo** para a pasta **Aplicativos**.
2. Abra o Papeo. O macOS vai bloquear a abertura por ser de um desenvolvedor não identificado.
3. Vá em **Ajustes do Sistema → Privacidade e Segurança**, role até o aviso sobre o Papeo e clique em **Abrir Mesmo Assim**. Confirme com sua senha ou Touch ID.

Se aparecer *"Papeo está danificado e não pode ser aberto"*, é a quarentena do download. Confira o instalador como descrito acima e rode no Terminal:

```sh
xattr -dr com.apple.quarantine /Applications/Papeo.app
```

Na primeira mensagem de voz ou chamada, o macOS pede permissão de microfone e câmera. Você pode mudar depois em **Ajustes do Sistema → Privacidade e Segurança → Microfone/Câmera**. Com a assinatura ad-hoc, o macOS pode pedir essas permissões de novo depois de cada atualização.

### Windows

1. Execute o `Papeo_<versão>_x64-setup.exe`. Ele instala só para o seu usuário, sem pedir senha de administrador.
2. Se o SmartScreen mostrar *"O Windows protegeu o computador"*, clique em **Mais informações → Executar assim mesmo**.

### Atualizar e desinstalar

Na primeira abertura, o Papeo pergunta se pode verificar atualizações. Com a verificação ligada (o padrão), ele consulta o GitHub uma vez por dia. Quando há versão nova, aparece **Atualizar e reiniciar** na barra lateral e em **Configurações → Sistema**. Nada é instalado sem esse clique.

- O pacote baixado só é instalado se a assinatura digital dele bater com a chave do Papeo. Um arquivo alterado no caminho é recusado.
- Configurações e logins são mantidos.
- No Windows, o instalador roda com uma barra de progresso e o app reabre sozinho.
- Para atualizar sem a verificação automática, use **Verificar agora** em **Configurações → Sistema** ou instale a versão nova por cima da antiga.

Desinstalar (arrastar para o Lixo no macOS ou **Configurações → Aplicativos** no Windows) **não apaga** seus logins e dados de sessão. Para apagar tudo, antes de desinstalar remova cada serviço ou use **Sair da conta e apagar dados**; depois apague a pasta indicada em [Onde ficam os dados](#onde-ficam-os-dados).

## Primeiros passos

1. **Adicione um serviço:** botão **Adicionar serviço** na barra lateral (ou `⌘N`/`Ctrl+N`). Escolha WhatsApp, Telegram ou informe o endereço de qualquer site.
2. **Faça login** normalmente dentro do serviço (no WhatsApp, leia o QR code pelo celular).
3. **Outra conta do mesmo mensageiro:** clique com o botão direito no serviço → **Adicionar outra conta**. Cada conta fica com sessão separada.
4. **Personalize:** botão direito → **Personalizar…** para mudar nome, cor, emoji e permissão de microfone/câmera.
5. **Ajuste em Configurações** (`⌘,`/`Ctrl+,`): aparência, não perturbe e horário silencioso, privacidade e bloqueio, abrir com o sistema, pasta de downloads e backup.

Dica: a tela **Início** (`⌘0`/`Ctrl+0`) mostra quanta memória cada serviço usa e sugere hibernar os mais pesados que estão parados.

## Atalhos de teclado

| Ação | macOS | Windows |
|---|---|---|
| Ir para o Início | `⌘0` | `Ctrl+0` |
| Ir para o serviço 1 a 9 | `⌘1` … `⌘9` | `Ctrl+1` … `Ctrl+9` |
| Buscar serviço | `⌘K` | `Ctrl+K` |
| Adicionar serviço | `⌘N` | `Ctrl+N` |
| Recarregar serviço | `⌘R` | — |
| Configurações | `⌘,` | `Ctrl+,` |
| Aumentar / diminuir zoom | `⌘+` / `⌘-` | `Ctrl++` / `Ctrl+-` |
| Zoom original | `⇧⌘0` | `Ctrl+Shift+0` |
| Bloquear agora (com bloqueio ligado) | `⌘L` | `Ctrl+L` |

No Windows, os atalhos só funcionam com o foco na interface do Papeo (barra lateral, Início, Configurações), não dentro da página do serviço. No macOS, os atalhos do menu (`⌘0`–`⌘9`, `⌘R`, `⌘K`, `⌘,` e zoom) funcionam em qualquer lugar.

## Recursos

**Serviços e contas**
- WhatsApp e Telegram prontos, mais qualquer site `https://`.
- Várias contas do mesmo mensageiro, cada uma com sessão isolada.
- Nome, cor e emoji por serviço; reordenar arrastando na barra lateral (ou **Mover para cima/baixo**).
- Zoom por serviço, lembrado entre sessões.

**Notificações**
- Notificações nativas do sistema, com interruptor e silêncio temporário (1 h, 8 h) por serviço.
- **Não perturbe** global e horário silencioso.
- Contador de não lidas na barra lateral, no ícone do app e na bandeja/menu bar.
- Ao voltar ao app logo após uma notificação, o serviço dela é aberto.

**Chamadas, arquivos e downloads**
- Mensagens de voz e chamadas, com microfone e câmera liberados por serviço: **Permitir**, **Perguntar** ou **Bloquear**.
- Arrastar arquivos para dentro da conversa.
- Downloads para uma pasta escolhida, com aviso e **Mostrar na pasta**.

**Memória e estabilidade**
- Hibernação manual e automática por serviço.
- Uso de memória por serviço na tela Início.
- Aviso quando um serviço desconecta (volta à tela de login) ou para de responder, com botão de recarregar.

**Janela e sistema**
- Bandeja/menu bar com contador; fechar a janela mantém o app ativo.
- Abrir com o sistema, já escondido.
- Tema do sistema, claro, escuro ou OLED (botão na barra superior); barra lateral recolhível.
- Links externos abrem no navegador padrão.
- Backup e restauração da configuração (sem conversas nem logins).

## Privacidade e segurança

**O que o Papeo faz por você**

- **Nada sai do computador por conta do Papeo,** exceto a verificação de atualizações. Sem telemetria nem relatório de erros. A verificação consulta o GitHub uma vez por dia (o GitHub vê o seu endereço IP) e pode ser desligada em **Configurações → Sistema**. Fora isso, a rede vem só dos sites que você adicionou.
- **Sessões isoladas** por conta. Remover um serviço ou usar **Sair da conta e apagar dados** apaga cookies, cache e conversas guardadas. O que não puder ser apagado na hora entra numa fila e é apagado na próxima abertura. **Configurações → Privacidade → Sessões antigas** remove logins de serviços que não estão mais na lista.
- **Prévia nas notificações:** completa, só o remetente ou oculta. Com o app bloqueado, o conteúdo nunca aparece.
- **Bloqueio** com Touch ID/senha do Mac ou Windows Hello: ao abrir, ao esconder a janela ou após um tempo sem uso. Enquanto bloqueado, nenhuma conversa aparece.
- **Proteção contra captura de tela:** opção para a janela aparecer vazia em gravações e compartilhamento de tela.
- **Sites isolados do app:** as páginas dos serviços não acessam os comandos do Papeo. Elas só podem pedir uma notificação e informar que estão ativas, e sempre em nome do próprio serviço.
- **Câmera e microfone por origem:** só o site do próprio serviço usa a permissão que você definiu. Outra página aberta dentro dele tem câmera e microfone negados.

**Limites que você deve conhecer**

- Os dados das sessões e a configuração não têm criptografia própria. Use a criptografia de disco do sistema (FileVault no macOS, BitLocker no Windows).
- O backup exportado é um arquivo JSON legível, com nomes e endereços dos serviços (sem logins nem conversas). Guarde-o como qualquer arquivo pessoal.
- Sites adicionados com `http://` não têm criptografia. O Papeo avisa ao adicionar e na barra superior; prefira sempre `https://`.
- O bloqueio protege contra quem usa o computador pela interface. Quem consegue editar os arquivos da sua conta de usuário pode desligá-lo. O bloqueio por inatividade é verificado a cada 15 s.
- A verificação de "mesmo site" é aproximada: em hospedagens compartilhadas (`*.github.io`, `*.vercel.app`), outro projeto do mesmo domínio conta como o mesmo site. Um iframe de terceiro dentro do serviço recebe a política de câmera/microfone do serviço, porque a webview não informa a origem do frame.
- No Windows, o WebView2 segue as configurações de diagnóstico do Windows e o SmartScreen da Microsoft.
- Os instaladores ainda não têm assinatura de desenvolvedor. Confira a atestação antes de instalar (ver [Instalação](#instalação)).

## Onde ficam os dados

| Sistema | Pasta |
|---|---|
| macOS | `~/Library/Application Support/io.github.ygormteus.papeo/` (sessões também no armazenamento do WebKit, identificadas por conta) |
| Windows | `%APPDATA%\io.github.ygormteus.papeo\` |

Dentro dela:

- `services.json` — serviços e configurações;
- `sessions/<id>/` — logins, cookies e cache de cada conta.

Para levar a configuração para outro computador, use **Configurações → Backup**. Os logins não vão junto: entre de novo em cada serviço.

## Problemas comuns

| Problema | O que fazer |
|---|---|
| Serviço mostra "desconectado" | A sessão expirou. Abra o serviço e faça login de novo. |
| Serviço "não responde" | Clique em **Recarregar** no aviso (ou `⌘R` no macOS). |
| Não chegam notificações | Confira o interruptor do serviço, o silêncio temporário, o não perturbe, o horário silencioso e as permissões de notificação do sistema para o Papeo. |
| Microfone ou câmera não funcionam | Confira **Personalizar… → microfone/câmera** do serviço e a permissão do sistema (macOS: **Privacidade e Segurança**; Windows: **Privacidade → Microfone/Câmera**). |
| Atalho não funciona no Windows | Clique na barra lateral do Papeo e tente de novo (ver [Atalhos](#atalhos-de-teclado)). |
| App consome muita memória | Use a tela Início para ver os serviços pesados e hibernar os parados, ou ligue a hibernação automática. |

**Limitações conhecidas:** o clique na notificação só traz o app para frente (o serviço é aberto porque o app detecta o retorno do foco); notificações que o site dispara pelo próprio service worker (push) não passam pelo Papeo.

## Desenvolvimento

### Pré-requisitos

- [Rust](https://rustup.rs) estável
- Node 24 e pnpm 10
- macOS: Xcode Command Line Tools (`xcode-select --install`)
- Windows: [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) e WebView2

Detalhes por sistema em [Tauri — Prerequisites](https://tauri.app/start/prerequisites/).

### Rodar

```sh
pnpm install
pnpm tauri dev      # app nativo com hot reload
pnpm dev            # só a interface no navegador, com backend simulado (http://localhost:1420)
```

### Checagens

Rode antes de abrir um PR:

```sh
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test -p papeo-core        # modelo e persistência (roda em qualquer SO)
pnpm check                      # tipos do frontend (svelte-check)
pnpm test                       # shim de notificações injetado nos sites
pnpm build                      # build da interface
```

### Gerar instaladores

- **Local:** `pnpm build:local` (gera para o sistema em uso, sem os pacotes de atualização). `pnpm tauri build` também gera os pacotes de atualização e exige a chave privada em `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- **GitHub Actions:** rode o workflow **Build** manualmente (instaladores em **Artifacts**) ou crie uma tag `v*`, que gera um release em rascunho com `.dmg` (Apple Silicon e Intel) e `.exe` (Windows). Não há `.msi`: um único instalador por sistema evita que a atualização misture tipos de instalação.
- **Assinatura:** macOS com assinatura ad-hoc (`bundle.macOS.signingIdentity: "-"`); Windows sem assinatura. Com o repositório público, um job separado (o único com permissão para isso) gera a atestação de procedência dos instaladores e dos pacotes de atualização. Actions fixadas por SHA; o workflow não tem permissões por padrão e cada job pede só as que usa; o cache do Rust não é usado em builds de tag.

### Publicar uma versão

1. Atualize `version` em `package.json`, `src-tauri/tauri.conf.json` e nos dois `Cargo.toml`.
2. Crie e envie a tag: `git tag v0.2.0 && git push origin v0.2.0`.
3. O workflow **Build** cria o release em rascunho com os instaladores, os pacotes de atualização e o `latest.json`, e confere se o `latest.json` tem as três plataformas (macOS Apple Silicon, macOS Intel e Windows). Se essa conferência falhar, não publique: na execução do workflow, use **Re-run** só no job de build da plataforma que faltou (ele reenvia para o mesmo rascunho) e depois reexecute o job `check-updater-json`.
4. Revise e **publique** o rascunho. Só depois disso os apps instalados enxergam a versão nova (o endereço consultado é `releases/latest/download/latest.json`).

A chave privada das atualizações fica nos secrets `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; a pública, em `plugins.updater.pubkey` de `tauri.conf.json`. **Guarde uma cópia da chave privada fora do GitHub:** sem ela, os apps instalados deixam de aceitar atualizações e precisam ser reinstalados à mão.

### Estrutura

| Caminho | Conteúdo |
|---|---|
| `src/` | Interface (Svelte 5 + Vite): barra lateral, topbar, Início, Configurações, diálogos |
| `src-tauri/src/` | App nativo: `webviews` (serviços), `commands` (interface), `notify` (sites), `downloads`, `memory`, `tray`, `menu` (macOS), `lock`, `updater`, `state` |
| `src-tauri/src/notification_shim.js` | Script injetado nos sites: notificações e sinal de vida |
| `src-tauri/capabilities/` | Permissões da interface (`default`) e dos sites (`services`) |
| `crates/papeo-core/` | Modelo, configurações, `services.json`, backup e regras puras, sem dependência do Tauri |

### Modelo de segurança

- **ACL de comandos.** Todos os comandos passam pelo manifesto de `src-tauri/build.rs`. A interface (`ui`) recebe os comandos pela capability `default`. Os sites (`svc-*`, origem remota) recebem **apenas** `service_notify` e `service_status` pela capability `services`. O backend identifica o serviço pelo rótulo da webview, então um site não fala em nome de outro nem aciona outros comandos. Comando novo precisa entrar em `COMMANDS` e na capability `default`.
- **Origem.** Notificações, "desconectado" e microfone/câmera só valem quando a página aberta na webview é do próprio site do serviço (`same_site`).
- **Arquivos.** Nomes de download passam por `safe_file_name` (sem pastas nem nomes reservados) e nunca sobrescrevem arquivos existentes. `id`/`session` vindos de backup ou de `services.json` editado à mão só são aceitos no formato gerado pelo app, porque viram rótulo de webview e nome de pasta.
- **CSP** da interface em `src-tauri/tauri.conf.json`: só recursos locais e IPC.

Regras para contribuir (invariantes, consulta de impacto e gates) estão em [`AGENTS.md`](AGENTS.md); contexto técnico em [`docs/CONTEXTO.md`](docs/CONTEXTO.md).

## Licença

[GPL-3.0-or-later](LICENSE). Você pode usar, estudar, modificar e redistribuir o Papeo. Quem distribuir uma versão modificada precisa publicar o código-fonte sob a mesma licença.
