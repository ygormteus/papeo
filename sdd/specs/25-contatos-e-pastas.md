# 25 — Endereços conhecidos e pastas em português

- **Estado:** entregue
- **Versão:** —
- **Relacionadas:** 09, 18, 20

## Problema

No leitor de e-mail, os campos Para, Cc e Cco não sugerem nada: a pessoa
tem de lembrar o endereço inteiro de quem ela já escreveu ou de quem já
recebeu e-mail. E as pastas aparecem com o nome que o servidor manda, em
inglês na maioria dos provedores ("Drafts", "Sent Mail", "Trash"), mesmo
com o Papeo em português.

## Fora do escopo

- Ler a agenda de contatos do provedor (Google Contatos, CardDAV, Outlook).
- Editar ou apagar endereços conhecidos um a um.
- Renomear pastas no servidor.

## Requisitos

- **R1.** Enquanto a pessoa digita em Para, Cc ou Cco, o leitor deve
  sugerir até 8 endereços que a conta já usou e combinam com o pedaço
  digitado (começo do nome, de uma palavra do nome ou do endereço; com 3
  letras ou mais, qualquer parte), os mais usados primeiro.
- **R2.** Setas escolhem, Enter ou Tab completam com "Nome <endereço>, ",
  Esc fecha só a lista; o clique também completa.
- **R3.** O Papeo deve aprender os endereços dos e-mails enviados, dos
  abertos (remetente, Para, Cc) e dos listados (remetente; nos enviados
  pela conta, os destinatários); a primeira busca de cada conta lê também
  os destinatários dos 200 últimos enviados.
- **R4.** Endereços de robôs ("no-reply", "mailer-daemon" e parecidos) e o
  da própria conta não entram; quem a pessoa escreveu vem antes de quem só
  escreveu para ela.
- **R5.** Os endereços ficam só neste computador, fora do backup e da
  sincronização, até 3.000 por conta, e saem quando o IMAP da conta sai.
- **R6.** As pastas especiais (Caixa de Entrada, Rascunhos, Enviados,
  Arquivo, Todos os e-mails, Com estrela, Spam, Lixeira) e algumas comuns
  sem marcação (Importante, Caixa de Saída, Notas, Agendados, Adiados,
  Conversas) devem aparecer com o nome no idioma do Papeo; as outras, com
  o nome do servidor.

## Design

- **Núcleo (`papeo_core::address_book`):** `AccountBook::note(addr, Seen,
  quando, própria)` e `search(consulta, limite)`; `AddressBook` (contas por
  `id`). Testes de filtro, ordem e teto.
- **App (`address_book.rs`):** estado gerenciado com lock próprio (nunca
  junto do `AppState` nem da conexão IMAP), leitura preguiçosa,
  gravação a cada 15 s se mudou (`write_atomic`), `forget` chamado por
  `mail::forget`. `mailbox.rs` registra em `list`, `open` (sem `peek`) e
  `send`; `backfill_contacts` lê 4 páginas de Enviados numa thread, uma
  vez por conta.
- **Comando:** `mail_contacts(id, query) -> Addr[]` (assíncrono, dispara a
  leitura dos enviados na primeira vez). Em `COMMANDS`, capability
  `default`, `api.ts`, `devMock.ts`.
- **Interface:** `AddressInput.svelte` (combobox acessível) nos três
  campos do editor; `MailReader.folderName` traduz pelo papel da pasta.
- **Dados persistidos:** `mail-contacts.json` em `app_local_data_dir`.
- **Privacidade:** nada sai do computador; os endereços já estavam nos
  e-mails da própria conta.

## Tarefas

1. [x] Núcleo: caderno de endereços com testes.
2. [x] App: armazenamento, registro em listar/abrir/enviar, leitura dos
   enviados, comando.
3. [x] Interface: `AddressInput`, nomes das pastas.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R4 | testes `search_ranks_prefix_and_use`, `keeps_people_and_skips_robots_and_self` | não verificado | — |
| R2 | `AddressInput.svelte` | não verificado | — |
| R3 | `mailbox::{list, open, send, backfill_contacts}` | não verificado (servidor real) | — |
| R5 | teste `trims_the_least_used`; `mail::forget` | não verificado | — |
| R6 | `MailReader.folderName` | não verificado (Gmail, Outlook) | — |
