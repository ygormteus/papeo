# 09 — E-mail

- **Estado:** entregue (0.8 webmail; 0.12–0.13 IMAP e leitor), retroativa
- **Relacionadas:** 02, 07, 12

## Problema

O webmail pesa tanto quanto um mensageiro e não avisa com o site fechado.
Gmail e Outlook não oferecem API de leitura sem auditoria paga.

## Fora do escopo

- API do Gmail ou do Microsoft Graph.
- POP3, Exchange (EWS), calendário e contatos.

## Requisitos

### Webmail

- **R1.** O Papeo deve abrir Gmail, Outlook (pessoal e trabalho/escola) e
  webmails (Roundcube e outros) como contas, com contador de não lidas e o
  assunto aberto para as tarefas.
- **R2.** Quando a pessoa abrir uma tarefa ligada a um e-mail, o Papeo deve
  buscar o assunto no site e abrir o resultado só se houver um único.

### Avisos por IMAP

- **R3.** Onde a conta tiver IMAP configurado, o Papeo deve avisar e-mails
  novos e manter o contador com o site hibernado (IDLE, ou consulta a cada
  2 min sem IDLE).
- **R4.** O Papeo deve aceitar senha (servidor, porta 993 TLS ou 143 com
  STARTTLS obrigatório) ou o login da Microsoft (OAuth com PKCE) no
  Outlook.
- **R5.** O Papeo deve preencher o servidor dos provedores conhecidos pelo
  domínio do endereço.
- **R6.** Se a conexão falhar, então o Papeo deve tentar de novo com espera
  crescente (5 s a 5 min). Com senha recusada, esperar 1 h. Depois de 3
  falhas, reabrir o site.
- **R7.** O Papeo deve guardar a senha presa a host, porta e usuário, em
  arquivo 0600 (macOS) ou no Gerenciador de Credenciais (Windows).
- **R8.** No Gmail, o contador deve considerar só a aba Principal, salvo a
  opção "Caixa de Entrada inteira".

### Leitor

- **R9.** Onde a conta tiver IMAP, o Papeo deve abri-la num leitor próprio
  (pastas, lista em páginas de 50, busca, mensagem, anexos, responder,
  responder a todos, encaminhar, arquivar, spam, excluir, estrela). O site
  só abre quando a pessoa pedir.
- **R10.** O Papeo deve mostrar a mensagem num quadro sem scripts, com HTML
  limpo e imagens de fora bloqueadas até "Mostrar imagens".
- **R11.** Se não houver Lixeira, então excluir deve pedir confirmação
  (exclusão definitiva).
- **R12.** O Papeo deve enviar por SMTP (465 TLS ou 587 STARTTLS; senha ou
  XOAUTH2), guardar cópia em Enviados quando o provedor não faz isso e
  marcar a original como respondida.
- **R13.** O rascunho deve pertencer à conta em que foi começado.

## Design

`papeo_core::mail` (cliente IMAP, `MailAccount`, provedores, SMTP derivado),
`papeo_core::mailbox` (leitor, `MessageView`), `mail.rs` (vigia),
`mailbox.rs` (leitor e envio), `microsoft.rs`, `secrets.rs`,
`MailReader.svelte`, `MailComposer.svelte`, `MailDialog.svelte`. Shim:
contador e assunto por site (spec 14).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | `email-shim.test.mjs` | parcial | não verificado |
| R3, R6 | servidor IMAP falso nos testes do núcleo | Outlook e Hostinger reais | não verificado |
| R4 (STARTTLS) | testes (sem rebaixamento) | não verificado (servidor real) | — |
| R9–R12 | testes do leitor e do envio; Hostinger real (ler, enviar, Enviados) | ok | não verificado |
| R12 (Outlook `SMTP.Send`), Gmail com senha de app | — | não verificado | não verificado |
