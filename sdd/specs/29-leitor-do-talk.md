# 29 — Leitor do Nextcloud Talk

- **Estado:** implementada
- **Versão:** —
- **Relacionadas:** 09 (leitor de e-mail, mesmo padrão), 26 (Nextcloud
  Talk pelo site), 11 (backup)

## Problema

O Talk no Papeo é a página do Nextcloud inteira: barra e menus da nuvem,
pesada para ficar aberta, e só avisa enquanto a página está carregada.
Quem quer só conversar precisa de algo direto, leve e com o visual do
Papeo, como já existe para o e-mail (IMAP).

## Fora do escopo

- Chamadas de voz e vídeo: abrem a página do Talk, sob demanda.
- Criar e administrar conversas (participantes, permissões, enquetes,
  salas de espera), editar e apagar mensagens, ler o histórico de chamadas.
- Copiar a interface ou o código do Talk Desktop (só a referência de como
  ele entra: Login Flow v2 e API OCS).

## Requisitos

### Entrar

- **R1.** Ao adicionar um Nextcloud Talk, a pessoa escolhe abrir pelo site
  ou pelo leitor do Papeo; uma conta já existente pode passar para o
  leitor pelo botão da barra.
- **R2.** "Conectar ao Talk" deve usar o Login Flow v2: o Papeo pede o
  login ao servidor, abre a página de aprovação no navegador e espera (até
  20 min, cancelável) a senha de app. Login, segundo fator e SSO ficam no
  navegador; o Papeo nunca vê a senha da pessoa.
- **R3.** O servidor deve ser `https:`; a página de aprovação e o servidor
  devolvido devem ser do mesmo endereço informado.
- **R4.** A senha de app fica no cofre das senhas de e-mail (arquivo 0600
  no macOS, Gerenciador de Credenciais no Windows), presa ao servidor e ao
  usuário. Remover a conta ou desconectar revoga a senha de app no
  servidor (melhor esforço) e apaga do cofre.

### Conversas e mensagens

- **R5.** O leitor deve listar as conversas (nome, foto, última mensagem,
  não lidas, menções), as com não lidas e favoritas primeiro, depois por
  atividade.
- **R6.** Ao abrir uma conversa, mostrar as últimas 50 mensagens, carregar
  as anteriores ao rolar para cima, e receber as novas por espera longa
  (`lookIntoFuture`) só enquanto a conversa estiver aberta.
- **R7.** Enviar mensagem (Enter envia, Shift+Enter quebra linha),
  responder citando, reagir com emoji (e tirar a reação), e marcar como
  lida até a última vista.
- **R8.** Menções: `@` no editor sugere participantes; mensagens mostram
  menções, arquivos e o ator no lugar dos marcadores (`{mention-user1}`,
  `{file}`). Mensagens de sistema aparecem discretas; as de reação e de
  apagar não aparecem.
- **R9.** Anexos: imagem mostra miniatura; clicar abre na visualização do
  Papeo (spec 21) ou salva em Downloads. Enviar arquivo grava na pasta de
  anexos do Talk da pessoa e compartilha na conversa.
- **R10.** Chamada: o botão abre a conversa no navegador (WebRTC fica com
  o navegador; o Papeo não carrega a página).

### Avisos e contador

- **R11.** Sem a página aberta, o Papeo deve consultar a lista de
  conversas a cada 30 s (60 s com outra conta na tela) e manter o
  contador (soma das não lidas) e avisar mensagens novas de outras
  pessoas, com o nome da conversa; o clique abre a conversa no leitor.
- **R12.** Senha de app recusada (revogada no servidor) deve marcar a conta
  para conectar de novo, sem insistir.

## Design

- **Módulos:**
  - `papeo-core/src/talk.rs`: endereço base, conferência do servidor,
    leitura das respostas OCS (conversas, mensagens, sugestões), texto com
    os marcadores trocados, ordem da lista, detecção de mensagem nova.
  - `papeo-core/src/service.rs`: `Service.talk: Option<TalkAccount>`
    (`server`, `user`, `uid`), só para Nextcloud.
  - `src-tauri/src/talk.rs`: cliente HTTP (reqwest), Login Flow v2,
    comandos, vigia (um thread por conta), cache curto de fotos e
    miniaturas na memória.
  - `src-tauri/src/state.rs`: `reader()` inclui `talk`; o status e o
    contador reusam `MailLive` (mesmo mapa).
  - `src/lib/TalkReader.svelte`; `App.svelte` escolhe o leitor pelo tipo.
- **Dados persistidos:** `services.json` ganha `talk` (opcional,
  `serde(default)`, arquivo antigo lê igual). Entra no backup e na
  sincronização (sem a senha: em outro computador pede conectar de novo).
  Senha de app em `mail-secrets.json`/Gerenciador de Credenciais.
- **Comandos e permissões:** `talk_login`, `talk_login_cancel`,
  `talk_logout`, `talk_rooms`, `talk_messages`, `talk_wait`, `talk_send`,
  `talk_read`, `talk_react`, `talk_mentions`, `talk_avatar`,
  `talk_preview`, `talk_file`, `talk_upload` (capability `default`;
  `api.ts`, `devMock.ts`). Eventos `papeo://talk-login`,
  `papeo://talk-reveal` e `papeo://talk-rooms` (a lista que o vigia
  trouxe; a tela não consulta de novo).
- **Restaurar backup:** a senha de app continua se a cópia restaurada tem a
  mesma conta do Talk (servidor e usuário); se a conta some, a senha é
  revogada no servidor e apagada daqui.
- **Concorrência:** HTTP assíncrono fora da thread principal; o vigia é
  um thread com `Control` (geração) como o do IMAP; nunca segura
  `AppState::lock()` durante rede.
- **Segurança e privacidade:** só `https:`; senha de app só em cabeçalho
  `Authorization` para o próprio servidor; `token` de conversa e caminho de
  arquivo conferidos antes de virar URL; HTML nunca é usado (mensagens são
  texto; links viram clicáveis e abrem no navegador). Miniaturas são
  baixadas pelo backend com a senha e entregues como `data:`. Cliente só
  `https:` (sem redirecionar para `http:`); corpo lido aos pedaços até o
  teto (2 MB imagem, 200 MB arquivo), mesmo sem tamanho anunciado; cache de
  imagens até 20 MB. Cancelar o login vale desde o primeiro pedido; se a
  aprovação chega depois do cancelar, a senha nova é revogada.
- **Plataformas:** iguais (o cofre já cobre as duas).

## Tarefas

1. [x] Núcleo: tipos, leitura das respostas, marcadores, ordem — testes.
2. [x] Conta e cofre; Login Flow v2; desconectar.
3. [x] Comandos de conversa e mensagem; vigia e avisos.
4. [x] Leitor na interface (lista, chat, editor, menções, reações).
5. [x] Anexos (miniatura, abrir, salvar, enviar).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R2, R3 | testes `talk_base`, `login_urls_must_match` | não verificado (servidor real) | — |
| R5, R8 | testes de leitura das conversas e dos marcadores | não verificado | — |
| R6, R7, R9–R12 | não verificado no app | — | — |
| Segurança | revisão crítica independente (2 médios e 6 leves corrigidos: leitura com teto, backup que apagava a senha sem revogar, `https_only`, cancelar o login, marcar como lida na conversa errada, cache por bytes, `talk:` só no Nextcloud, botão e "desconectar" no leitor) | — | — |
