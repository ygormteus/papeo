# 18 — Rascunhos do leitor no servidor

- **Estado:** entregue
- **Versão:** 0.15.0
- **Relacionadas:** 09, 16

## Problema

No leitor de e-mail (spec 09), fechar o editor perde o texto, e um
rascunho começado no celular ou no webmail não abre para continuar.

## Fora do escopo

- Rascunho em HTML (o editor é de texto; um rascunho HTML de outro
  programa abre pelo texto dele).
- Guardar rascunho só no computador (vai para a pasta Rascunhos do
  servidor, como em qualquer cliente).
- Marcar como respondida a mensagem original de uma resposta reaberta da
  pasta Rascunhos (o rascunho não guarda a pasta e o UID dela).

## Requisitos

- **R1.** Enquanto o editor tiver mudanças não salvas, o Papeo deve salvar
  o rascunho na pasta Rascunhos do servidor 10 s depois da última mudança.
- **R2.** Quando a pessoa fechar o editor com mudanças não salvas, o Papeo
  deve salvar o rascunho antes de fechar. Se não der, o editor deve ficar
  aberto com o erro e a opção de descartar.
- **R3.** Cada salvamento deve substituir a versão anterior do mesmo
  rascunho: a nova é gravada antes de a antiga ser apagada.
- **R4.** O Papeo deve oferecer "Descartar", que fecha o editor e apaga o
  rascunho salvo no servidor.
- **R5.** Quando o e-mail for enviado (na hora ou depois da espera da spec
  16), o Papeo deve apagar o rascunho dele.
- **R6.** Quando a pessoa abrir uma mensagem da pasta Rascunhos, o Papeo
  deve abri-la no editor (destinatários, Cco, assunto, texto, anexos e a
  mensagem respondida); salvar ou enviar substitui esse rascunho.
- **R7.** Rascunho sem destinatário, ou com um endereço ainda incompleto,
  deve poder ser salvo; o endereço incompleto volta ao campo ao reabrir.
- **R8.** Se a conta não tiver pasta Rascunhos, então o Papeo deve avisar no
  editor e não tentar salvar de novo sozinho.
- **R9.** Abrir uma resposta ou mensagem nova e fechar sem mudar nada não
  deve criar rascunho.
- **R10.** O Papeo só deve apagar um rascunho de outro programa se ele
  estiver na pasta Rascunhos e o UID ainda for dele (mesmo `Message-ID`
  ou, sem um, ainda marcado como rascunho).
- **R11.** Quando o app bloquear com o editor aberto, o Papeo deve salvar o
  que faltava antes de esconder o editor.

## Design

- **Núcleo (`papeo_core::mailbox`):** `Reader::save_draft` (procura as
  versões com o mesmo `Message-ID` por `UID SEARCH HEADER`, faz `APPEND`
  com `\Draft \Seen` e expurga as antigas), `discard_drafts`, `discard`
  (expurgo direto de UIDs, para substituir rascunho de outro programa) e
  `draft_view` (texto, endereços, Cco, `In-Reply-To`, referências e
  anexos de um rascunho bruto).
- **App (`mailbox.rs`):** `Draft` ganha `draft_token` (24 hex, vira
  `Message-ID` `<papeo-draft-<token>@papeo.draft>`) e `replaces`
  (pasta + UID de um rascunho de outro programa). `build` aceita modo
  rascunho: sem exigir destinatário (envelope do próprio remetente), mantém
  o Cco e fixa o `Message-ID`. `save_draft`, `discard_draft`,
  `open_draft`; `send` apaga o rascunho depois do envio (melhor esforço).
- **Comandos e permissões:** `mail_save_draft`, `mail_discard_draft`,
  `mail_open_draft` em `COMMANDS` e na capability `default`; `api.ts`,
  `devMock.ts`.
- **Dados persistidos:** nada novo no computador; o `outbox/` (spec 16)
  passa a levar `draftToken`/`replaces` junto do e-mail (campos opcionais).
- **Concorrência:** tudo pela conexão de leitura da conta (`with_reader`),
  fora do lock do estado; procurar a pasta e salvar numa ida só (dois
  salvamentos não se cruzam). O token nasce no editor (uma falha no meio
  não cria cópias); os salvamentos do editor vão em fila; enviar e
  descartar esperam o salvamento em curso e desligam os seguintes.
  `mail_save_draft` não exige desbloqueio (só grava o que já estava
  escrito, quando o bloqueio esconde o editor); esse salvamento pode
  apagar o rascunho de outro programa substituído, sempre com a conferência
  do R10.
- **Endereço incompleto:** vai em `X-Papeo-Unparsed-To/Cc/Bcc` (base64) e
  volta ao campo ao reabrir; o envio continua recusando.
- **Segurança:** pasta e UID vindos da tela passam por `check_path`; o
  token só no formato gerado.

## Tarefas

1. [x] Núcleo: `save_draft`, `discard_drafts`, `discard`, `draft_view` —
   testes com o servidor IMAP falso (R3, R6, R7).
2. [x] App: montagem em modo rascunho, comandos, apagar ao enviar (R5, R7).
3. [x] Editor: salvar sozinho, ao fechar, Descartar; abrir da pasta
   Rascunhos (R1, R2, R4, R6, R8, R9).

## Verificação

Revisão crítica independente: sem bloqueante; 1 médio-alto (salvamento ao
sair da tela fora da fila), 3 médios e 3 baixos corrigidos (1 documentado
em "Fora do escopo"); re-checagem aprovada, com 2 baixos novos corrigidos
(salvamento enfileirado durante o envio; base64 longo em pedaços).

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R3 | teste `draft_is_saved_before_old_versions_go` (APPEND antes do expurgo) | não verificado (servidor real) | não verificado |
| R5, R7 | testes `draft_keeps_bcc_and_may_have_no_recipient`, `draft_view_reads_what_the_editor_needs` | não verificado | não verificado |
| R6 | teste `draft_view_reads_what_the_editor_needs`; mock (pasta Rascunhos) | não verificado | não verificado |
| R10 | teste `foreign_draft_is_only_discarded_if_still_there` | não verificado | não verificado |
| R1, R2, R4, R8, R9, R11 | `MailComposer.svelte`; mock `?semrascunhos` | não verificado | não verificado |
| Gmail (`[Gmail]/Drafts`, expurgo) | — | não verificado | — |
