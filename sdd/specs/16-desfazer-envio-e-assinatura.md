# 16 — Desfazer envio e assinatura

- **Estado:** entregue
- **Versão:** 0.15.0
- **Relacionadas:** 09

## Problema

No leitor de e-mail (spec 09), "Enviar" sai na hora: um destinatário errado
ou um anexo esquecido não tem volta. E cada mensagem precisa da assinatura
digitada à mão.

## Fora do escopo

- Agendar o envio para mais tarde.
- Assinatura com HTML ou imagem (o leitor envia texto simples).
- Assinatura no webmail (o site tem a dele).

## Requisitos

### Desfazer envio

- **R1.** Onde a espera do envio for maior que zero, quando a pessoa
  enviar, o Papeo deve fechar o editor e só enviar depois da espera,
  mostrando no leitor "Enviando em N s" com o botão "Desfazer".
- **R2.** Quando a pessoa clicar em "Desfazer" durante a espera, o Papeo
  não deve enviar e deve reabrir o editor com o e-mail como estava
  (destinatários, assunto, texto, anexos e a mensagem respondida).
- **R3.** Se o envio falhar depois da espera, então o Papeo deve reabrir o
  editor daquela conta com o e-mail e o erro.
- **R4.** Se o app fechar ou cair durante a espera, então o Papeo não deve
  enviar o e-mail sozinho depois; quando o leitor daquela conta abrir de
  novo, deve reabrir o e-mail no editor com o aviso de que não foi enviado.
- **R5.** O Papeo deve conferir endereços, tamanho do texto e anexos antes
  da espera: erro aparece no editor, sem fechar.
- **R6.** O Papeo deve oferecer a espera em Configurações → E-mail:
  desligada, 5, 10 ou 20 s. Padrão: 5 s.
- **R7.** Enquanto a janela estiver escondida, em outra conta ou o app
  bloqueado, a espera deve continuar e o envio sair no fim dela (o envio já
  foi confirmado).
- **R8.** O Papeo deve guardar o e-mail em espera só no computador, num
  arquivo que só o usuário lê, e apagá-lo quando o envio der certo ou
  quando o e-mail voltar ao editor (desfeito, falha ou sobra). Sobra de
  conta removida, sem o leitor ou com mais de 30 dias é apagada ao abrir.
- **R11.** Se a conta de envio (usuário, servidor ou porta) mudar durante a
  espera, então o Papeo não deve enviar e deve devolver o e-mail ao editor.

### Assinatura

- **R9.** O Papeo deve guardar uma assinatura de texto por conta de e-mail
  (até 2000 caracteres), editada na configuração do IMAP da conta, no
  botão "Assinatura" do leitor de e-mail e em Configurações → Geral →
  E-mail → Assinaturas (lista das contas com IMAP).
- **R10.** Quando a pessoa começar uma mensagem nova, uma resposta ou um
  encaminhamento, o Papeo deve pôr a assinatura, depois de uma linha
  `-- `, entre o espaço do texto e a mensagem citada. A pessoa pode
  apagá-la antes de enviar.

## Perguntas

- [x] Tempo de espera: ajustável, padrão 5 s (usuário, 2026-11-27).
- [x] App fechado na espera: não envia; o e-mail volta ao editor
  (usuário, 2026-11-27).

## Design

- **Módulos:** `src-tauri/src/outbox.rs` (novo): fila de envios em espera.
  `mail_send_later` confere o e-mail com o mesmo `build` do envio (R5),
  grava `outbox/<id>.json` com `write_private` (R8) e agenda uma tarefa
  que dorme a espera. Quem tirar a entrada do mapa primeiro vence: a
  tarefa (envia com `mailbox::send`) ou `mail_send_cancel` (devolve o
  rascunho). A entrada vai para a fila antes de o arquivo ser gravado (uma
  leitura de sobras nunca a pega no meio). No fim da espera, a conta é
  conferida contra a de quando o envio foi pedido (R11); antes do SMTP o
  arquivo é regravado com `sending: true` e, se isso falhar, não envia.
  Sucesso apaga o arquivo; falha o regrava com o erro e emite
  `papeo://mail-send` com o rascunho (R3). Desfazer também deixa o
  arquivo. Ao abrir, nada é agendado: `mail_unsent` entrega ao leitor da
  conta os guardados que não estão na fila (R4), com o aviso "pode ter sido
  enviado" quando `sending` ficou marcado, e o leitor chama
  `mail_unsent_taken` depois de pôr o e-mail no editor, o que apaga o
  arquivo. Sobras de contas que não existem mais, sem o leitor ou com mais
  de 30 dias são apagadas ao abrir. A conexão do leitor compara a conta sem
  a assinatura (trocar a assinatura não reconecta).
  Interface: `outbox.svelte.ts` (estado global: envios em espera e
  rascunhos devolvidos, por conta, vivo mesmo com o leitor desmontado),
  barra "Enviando em N s · Desfazer" no `MailReader`, assinatura no
  `MailDialog` e no início do texto do `MailComposer`.
- **Dados persistidos:** `Settings.mail_undo_secs` (`u8`, padrão 5;
  `sanitized` aceita 0, 5, 10, 20, outro valor vira 5).
  `MailAccount.signature` (`String`, omitido se vazio; `services.json`
  antigo lê vazio; versão antiga do Papeo ignora o campo). Entra no backup
  junto com a conta; não entra na sincronização (só tarefas).
  `outbox/*.json` em `app_local_data_dir`, fora do backup.
- **Comandos e permissões:** `mail_send_later`, `mail_send_cancel`,
  `mail_unsent`, `mail_unsent_taken`, `set_mail_signature` em `COMMANDS` e na capability
  `default` (só a webview `ui`); `api.ts` e `devMock.ts`. `set_preferences`
  ganha `mailUndoSecs`. Evento `papeo://mail-send` só para a `ui`.
- **Concorrência:** a tarefa da espera roda no runtime assíncrono; o envio
  em `spawn_blocking` (como hoje). O mapa da fila tem `Mutex` próprio,
  nunca junto com `AppState::lock()`. `set_mail_signature` só atualiza o
  campo (o vigia compara `Options`, que não inclui a assinatura).
- **Segurança e privacidade:** comandos só para a `ui` (sites `svc-*` não
  alcançam). O arquivo em espera tem o texto e os anexos: 0600, apagado no
  fim; nada sai do computador além do envio já pedido. Envio exige
  desbloqueio no pedido; o fim da espera não (R7).
- **Plataformas:** iguais (Windows sem permissão Unix; a pasta é do
  usuário).

## Tarefas

1. [x] `Settings.mail_undo_secs` e `MailAccount.signature` (R6, R9) —
   teste: leitura de arquivo antigo e `sanitized`.
2. [x] `outbox.rs` e comandos (R1–R5, R7, R8) — teste: arquivo da fila
   (gravar, ler, apagar antigos/órfãos).
3. [x] Interface: barra de espera, Desfazer, rascunho devolvido, opção nas
   Configurações (R1–R4, R6).
4. [x] Assinatura no `MailDialog` e no editor (R9, R10).
5. [x] Assinatura também no leitor e nas Configurações (`SignatureDialog.svelte`, R9).

## Verificação

Revisão crítica independente: sem achado alto; 2 médios e 5 baixos
corrigidos; re-checagem confirmou os 7 e apontou 1 baixo novo (rascunho
devolvido duas vezes numa corrida), corrigido com o conjunto `inEditor`.

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R4, R7, R11 | fila em `outbox.rs` (corrida desfazer × envio sob o mesmo `Mutex`); mock `pnpm dev` (assunto com "falha", `?naoenviado`) | não verificado (envio real) | não verificado |
| R5 | `mailbox::validate` reusa o `build` do envio | não verificado | não verificado |
| R6 | `settings.rs` (padrão 5, valor fora da lista volta a 5) | não verificado | — |
| R8 | testes `file_round_trip_keeps_the_draft` (0600), `prune_drops_old_orphan_and_broken_files`, `ids_are_hex_only` | não verificado | não verificado (sem 0600) |
| R9 | teste `signature_is_cleaned_and_optional`; `SignatureDialog.svelte` (leitor e Configurações → E-mail → Assinaturas); mock | não verificado | — |
| R10 | `MailReader.svelte` (nova, resposta, encaminhar) | não verificado | — |
