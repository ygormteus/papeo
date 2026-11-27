# 24 — Baixar todos os anexos e ver os destinatários

- **Estado:** entregue
- **Versão:** —
- **Relacionadas:** 09, 21

## Problema

No leitor de e-mail, uma mensagem com vários anexos obriga a salvar um por
um. E o cabeçalho mostra só os nomes de quem recebeu (Para, Cc), sem como
ver o endereço completo, copiar ou escrever para uma dessas pessoas; com
muitos destinatários, a lista ocupa a tela.

## Fora do escopo

- Compactar os anexos num `.zip`.
- Cco (o servidor não guarda quem recebeu em cópia oculta).

## Requisitos

- **R1.** Quando a mensagem tiver mais de um anexo, o leitor deve mostrar
  "Baixar todos" acima da lista de anexos.
- **R2.** Ao baixar todos, o Papeo deve gravar os anexos numa pasta nova
  dentro da pasta de downloads, com o nome do assunto (ou "Anexos"), sem
  sobrescrever pasta nem arquivo que já exista, e avisar com "Mostrar na
  pasta".
- **R3.** Cada remetente e destinatário (De, Para, Cc) deve ser clicável,
  com o endereço completo ao passar o mouse, e abrir um menu com "Escrever
  para…" e "Copiar endereço".
- **R4.** Com mais de três destinatários numa linha, o leitor deve mostrar
  os três primeiros e "e mais N"; ao abrir, todos com nome e endereço, e
  "mostrar menos".

## Design

- **App (`mailbox.rs`):** `save_attachments(id, folder, uid, indices,
  subject)`: um download da mensagem (o mesmo cache de `save_attachment`),
  pasta com `unique_path` e `safe_file_name` (sem barras nem pontos, até 80
  caracteres), cada arquivo com `unique_path` e `mark_downloaded`. Devolve a
  pasta, que `mail_reveal_file` aceita (está dentro de Downloads).
- **Comando:** `mail_save_attachments` em `COMMANDS`, capability `default`,
  `api.ts`, `devMock.ts`.
- **Interface:** `AddressList.svelte` (lista de endereços com menu
  `ContextMenu` e "e mais N"); `MailReader` usa nos três campos e mostra
  "Baixar todos".
- **Dados persistidos:** nada novo.
- **Segurança:** nomes de pasta e de arquivo passam por `safe_file_name`;
  o caminho fica sempre dentro da pasta de downloads.

## Tarefas

1. [x] App: `save_attachments` e comando.
2. [x] Interface: "Baixar todos", `AddressList`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R2 | `mailbox::save_attachments`, `MailReader.saveAll` | não verificado | não verificado |
| R3, R4 | `AddressList.svelte` | não verificado | — |
