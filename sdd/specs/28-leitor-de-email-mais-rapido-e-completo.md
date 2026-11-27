# 28 — Leitor de e-mail mais rápido e mais completo

- **Estado:** implementada
- **Versão:** —
- **Relacionadas:** 09 (e-mail), 21 (anexos na tela), 22 (assinatura
  formatada), 27 (lista rápida)

## Problema

Abrir um e-mail com anexo grande baixava a mensagem inteira só para
mostrar o texto. A conexão do leitor fechava depois de 5 min parada, e
reabrir custa TLS e login. O clique no aviso de e-mail novo procurava o
assunto (lento, podia achar outro). Faltavam funções básicas de um
leitor: agir em várias mensagens, mover para qualquer pasta, filtros,
marcar todas como lidas, desfazer, descadastrar, atalhos e imagens de
remetentes confiáveis.

## Fora do escopo

- Conversas agrupadas, contador de não lidas por pasta, leitura sem rede
  e envio agendado (pesam ou precisam do app aberto; ver a análise).
- Descadastro de um clique (`List-Unsubscribe-Post`): o Papeo abre o
  endereço no navegador ou um e-mail pronto; não faz pedidos sozinho.

## Requisitos

- **R1.** Se o e-mail tiver mais de 512 KB e mais de uma parte, o Papeo
  deve baixar só a estrutura, o cabeçalho de cada parte, o texto e as
  imagens embutidas do corpo; os anexos aparecem com nome e tamanho e são
  baixados ao abrir ou salvar um deles. Pedaço de texto que não vier abre
  a mensagem inteira.
- **R12.** No editor, a assinatura formatada (logo, cores) deve aparecer
  como prévia abaixo do texto, num quadro sem scripts, enquanto estiver
  intacta.
- **R2.** A conexão de leitura deve fechar só depois de 20 min parada.
- **R3.** O clique no aviso de e-mail novo deve abrir aquela mensagem no
  leitor pelo UID; se ela não estiver mais na Caixa de Entrada, procurar
  pelo assunto.
- **R4.** Com Cmd/Ctrl ou Shift no clique (ou Shift com as setas, Cmd+A
  na lista), o Papeo deve selecionar várias mensagens e oferecer
  arquivar, spam, excluir, mover, marcar como lidas/não lidas e estrela
  para todas de uma vez.
- **R5.** "Mover para…" deve listar as outras pastas; arrastar mensagens
  da lista para uma pasta deve movê-las.
- **R6.** Filtros rápidos (todas, não lidas, com estrela, com anexo)
  devem ser feitos pelo servidor (`UNSEEN`, `FLAGGED`; anexo pelo
  `has:attachment` do Gmail ou `multipart/mixed` nos outros).
- **R7.** "Marcar todas como lidas" deve marcar a pasta inteira com um
  comando só; com busca ou filtro ativo, o botão some (não marca o que
  não está à vista).
- **R8.** Depois de arquivar, mover ou levar à Lixeira, o aviso deve
  oferecer "Desfazer" quando o servidor informa os UIDs novos (UIDPLUS);
  exclusão definitiva não tem desfazer.
- **R9.** Com `List-Unsubscribe`, o Papeo deve mostrar "Descadastrar
  desta lista": `https:` abre no navegador; `mailto:` abre o editor
  pronto. Outros esquemas são ignorados.
- **R10.** Atalhos fora de campos de texto: `e` arquivar, `#` excluir,
  `r` responder, `a` responder a todos, `f` encaminhar, `s` estrela, `u`
  não lida, `/` buscar, `Esc` limpa a seleção. Valem com o foco no corpo
  do e-mail também.
- **R11.** "Sempre mostrar de {endereço}" deve guardar o remetente (por
  conta, até 500, só neste computador) e mostrar as imagens de fora dele
  ao abrir; "Parar de mostrar" desfaz.

## Design

- **Módulos:**
  - `papeo-core/src/mail_parts.rs`: lê a `BODYSTRUCTURE`, escolhe as
    partes, remonta a mensagem com a mesma árvore (os anexos ficam com o
    mesmo índice da mensagem inteira) e estima o tamanho dos que vieram
    vazios.
  - `papeo-core/src/mailbox.rs`: `open_message` (inteira ou aos
    pedaços), `Filter` em `list_from`/`search`, `move_to`/`delete`
    devolvem os UIDs do `[COPYUID]`, `mark_all_seen`, `unsubscribe` na
    `MessageView`.
  - `papeo-core/src/mail.rs`: `Imap::last_ok` guarda a linha do OK com
    tag (o `COPYUID` do `UID COPY` vem nela).
  - `src-tauri/src/mailbox.rs`: `IDLE_CLOSE` 20 min; anexo de mensagem
    aberta aos pedaços baixa a inteira e a guarda para o próximo.
  - `src-tauri/src/mail.rs`/`webviews.rs`: últimos 100 avisos (conta,
    assunto, UID) só na memória; `papeo://mail-reveal` leva o `uid`.
  - `MailReader.svelte`, `ContextMenu.svelte` (rola com muitas pastas).
- **Dados persistidos:** remetentes confiáveis em `localStorage` da
  interface (`papeo.mail.trustedImages`), fora do backup e da
  sincronização. Nada novo no disco do backend.
- **Comandos e permissões:** `mail_mark_all_read(id, folder)` novo
  (capability `default`). `mail_open` ganha `size` opcional; `mail_list`
  ganha `filter` opcional; `mail_move` e `mail_delete` devolvem os UIDs
  novos. `api.ts` e `devMock.ts` atualizados.
- **Concorrência:** tudo na conexão de leitura (`spawn_blocking`); a
  lista de avisos tem `Mutex` próprio, nunca junto do `AppState`.
- **Segurança e privacidade:** a estrutura e os pedaços vêm do servidor
  e são lidos com limites (200 partes, 12 níveis, mesmos tetos de
  literal). O endereço de descadastro só passa se for `http(s):` ou
  `mailto:`, sem espaços nem controles, até 2000 caracteres; abre no
  navegador por clique. Remetente confiável só libera imagens de fora
  (a CSP e o quadro sem scripts continuam). Risco aceito: o `From` pode
  ser forjado, e um e-mail falso em nome de um remetente confiável carrega
  as imagens (e o rastreador) dele.
- **Limites aceitos:** fora do Gmail, "com anexo" é `multipart/mixed` (o
  mesmo critério do clipe da lista): erra anexos dentro de
  `multipart/related` ou assinados. Imagem citada maior que 3 MB
  (decodificada) chega vazia e aparece como anexo. O aviso abre pelo UID
  por 30 min e uma vez só; depois, procura o assunto.
- **Plataformas:** iguais.

## Tarefas

1. [x] Abrir aos pedaços (R1) — testes `reads_structure`,
   `assembles_without_attachments`, `empty_parts_keep_attachment_indices`,
   `reader_opens_big_message_in_parts_and_more`.
2. [x] Conexão por 20 min (R2).
3. [x] Aviso abre pelo UID (R3).
4. [x] Seleção, mover, arrastar (R4, R5).
5. [x] Filtros e marcar todas (R6, R7) — teste do servidor falso.
6. [x] Desfazer (R8) — teste `reads_copied_uids`.
7. [x] Descadastrar (R9) — teste `finds_unsubscribe`.
8. [x] Atalhos e remetentes confiáveis (R10, R11).
9. [x] Prévia da assinatura formatada no editor (R12).

## Verificação

Revisão: Sonnet (geral, achados corrigidos) e crítica independente (privacidade e dados do servidor: aninhamento, separador forjável, tipo da parte, imagens, prévia), fechada após as correções.


| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | testes do núcleo (estrutura, remontagem, índices com partes vazias, separador não forjável, aninhamento) | não verificado no app | — |
| R2 | constante `IDLE_CLOSE` | — | — |
| R3 | não verificado no app | — | — |
| R4, R5 | não verificado no app | — | — |
| R6, R7 | servidor falso (`UNSEEN`, `STORE 1:*`) | não verificado no app | — |
| R8 | `reads_copied_uids`, servidor falso (`MOVE` → UID novo) | não verificado no app | — |
| R9 | `finds_unsubscribe` | não verificado no app | — |
| R10, R11 | não verificado no app | — | — |
| R12 | não verificado no app | — | — |
