# 21 — Anexos do e-mail na tela

- **Estado:** entregue
- **Versão:** 0.16.0
- **Relacionadas:** 06, 09

## Problema

No leitor de e-mail (spec 09), clicar num anexo só o grava em Downloads:
para ler um PDF é preciso baixar, ir à pasta e abrir em outro programa.
Cada anexo visto vira um arquivo esquecido em Downloads.

## Fora do escopo

- Visualizar anexo de rascunho no editor (continua só a lista de anexos).
- Anotar ou editar o PDF dentro do Papeo.
- Visualizar vários anexos lado a lado (um por vez, como nos sites).

## Requisitos

- **R1.** Quando a pessoa clicar num anexo de um tipo que dá para ver
  (PDF, imagens, áudio, vídeo; no Mac também Office e outros que o Quick
  Look mostra), o Papeo deve abri-lo na janela de visualização de anexos
  (spec 06), com Salvar, Abrir no app e Mostrar na pasta.
- **R2.** O anexo aberto só deve ir para Downloads quando a pessoa salvar
  ou abrir no app; o temporário é apagado ao fechar a janela (spec 06 R5).
- **R3.** Se o tipo não der para ver (zip, executável…), então clicar deve
  salvar em Downloads, como antes.
- **R4.** Cada anexo deve ter também um botão só de salvar em Downloads.
- **R5.** Com uma mensagem aberta e o foco fora de um campo de texto, a
  barra de espaço deve abrir o primeiro anexo que dá para ver.
- **R6.** O arquivo do anexo deve ser marcado como vindo da internet
  (spec 06 R2), também na cópia salva a partir da visualização.
- **R7.** Enquanto o app estiver bloqueado, nada abre (como os outros
  comandos do leitor).

## Design

- **App (`attachment.rs`):** `show_bytes(app, service_id, name, bytes)`
  grava os bytes numa pasta nova dentro do temporário de anexos (nome por
  `safe_file_name`), marca como baixado e chama o mesmo `open` dos
  downloads dos sites. Devolve `false` sem gravar nada se o tipo não tiver
  visualização. Roda fora da thread principal (o `open` espera a janela).
- **App (`mailbox.rs`):** `preview_attachment` busca o anexo como
  `save_attachment` (mesma mensagem em cache) e chama `show_bytes`;
  `mark_downloaded` passa a `pub(crate)`. A cópia para Downloads (`fs::copy`)
  leva a marca junto (atributo estendido no Mac, `Zone.Identifier` no
  Windows).
- **Comando:** `mail_preview_attachment(id, folder, uid, index) -> bool`
  (`false`: tipo sem visualização), com `ensure_unlocked`; em `COMMANDS`, na
  capability `default`, `api.ts`, `devMock.ts`.
- **Interface (`MailReader.svelte`):** o anexo vira dois botões: o nome
  abre (`mailPreviewAttachment`; `false` → salva), o ícone salva. Espaço
  abre o primeiro visualizável (mesma regra de extensões de
  `attachment_view`, só para escolher qual; quem decide é o backend).
- **Dados persistidos:** nada novo.
- **Concorrência:** busca e gravação no `blocking` do leitor; o `open` pega
  o lock do estado só para ler configurações, sem segurá-lo ao criar a
  janela (como já é nos downloads).
- **Segurança:** o nome vem do e-mail: só `safe_file_name` dentro de uma
  pasta nova do temporário; a página do Windows mostra só o arquivo, sem
  comandos (spec 06).
- **Plataformas:** Mac com Quick Look (inclui Office); Windows com a
  webview para PDF/imagem/áudio/vídeo e a barra para Word/Excel.

## Tarefas

1. [x] `show_bytes` e `preview_attachment` (R1–R3, R6, R7).
2. [x] Comando, ACL, `api.ts`, `devMock.ts`.
3. [x] Leitor: dois botões e o atalho de espaço (R4, R5).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R2, R3 | `attachment::show_bytes` (mesma janela e temporário da spec 06); `mailbox::preview_attachment` | não verificado (anexo real) | não verificado |
| R4, R5 | `MailReader.svelte` (dois botões, `spaceKey`); mock | não verificado | não verificado |
| R6 | `mark_downloaded` no temporário; `fs::copy` leva a marca | não verificado | não verificado |
| R7 | `ensure_unlocked` em `mail_preview_attachment`; `open` salva sem mostrar se bloqueou no meio | — | — |
