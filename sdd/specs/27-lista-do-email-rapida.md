# 27 — Lista do e-mail rápida e rolagem infinita

- **Estado:** implementada
- **Versão:** 0.17.0
- **Relacionadas:** 09 (e-mail), 15 (fluidez), 25 (contatos e pastas)

## Problema

Abrir o leitor de uma conta mostra a tela vazia até o IMAP conectar (TLS e
login) e responder a lista de pastas e a primeira página; trocar de pasta
espera de novo. Pastas longas são lidas em páginas de 50 com setas
("1–50 de 6803"), o que obriga a clicar para ver as mais antigas.

## Fora do escopo

- Guardar o corpo das mensagens ou anexos no disco (só a lista).
- Leitura sem rede: a lista guardada é só um adiantamento; ações
  (abrir, mover, excluir) continuam pedindo o servidor.
- Guardar resultados de busca.

## Requisitos

- **R1.** Quando o leitor de uma conta abre, o Papeo deve mostrar na hora
  as pastas e as mensagens mais novas da pasta guardadas da última vez, e
  trocá-las pelas do servidor assim que ele responder, sem piscar.
- **R2.** Ao trocar para uma pasta já vista, o Papeo deve mostrar na hora
  as mensagens guardadas dela enquanto busca as atuais.
- **R3.** O Papeo deve guardar só pastas e as 50 mensagens mais novas de
  até 8 pastas por conta (as usadas mais recentemente), sem busca, num
  arquivo só do usuário (0600), fora do backup e da sincronização.
- **R4.** Se a conta trocar de usuário, servidor ou porta, a lista
  guardada não deve ser mostrada; removida a conta ou o IMAP dela, o
  arquivo deve ser apagado.
- **R5.** Na lista de mensagens, ao rolar até perto do fim, o Papeo deve
  carregar as 50 seguintes sozinho (pasta ou busca), sem setas; uma
  mensagem que entrou no meio não aparece repetida.
- **R6.** Se a busca da continuação falhar, o Papeo deve mostrar o erro no
  fim da lista com "Tentar de novo", sem perder o que já está na tela.
- **R7.** Com mais de uma leva carregada, a atualização sozinha (contador
  mudou) deve trocar só as mais novas e manter as antigas já carregadas.

## Design

- **Módulos:**
  - `papeo-core/src/mail_cache.rs`: modelo do arquivo (`MailCache`),
    regras de limite e de troca de conta, testes.
  - `papeo-core/src/mailbox.rs`: `Reader::list_from(path, skip, query)`
    (a partir de `skip` mensagens; `list` vira atalho por página);
    `Folder`, `Role`, `Addr`, `Summary` e `Page` passam a ler JSON.
  - `src-tauri/src/mail_cache.rs`: arquivo por conta em
    `app_local_data_dir/mail-cache/<id>.json`, lock próprio, apagado por
    `mail::forget`.
  - `src-tauri/src/mailbox.rs`: `folders` e `list` (sem busca, desde o
    início) gravam a lista guardada depois de responder.
  - `src/lib/MailReader.svelte`: mostra o guardado, revalida, rolagem
    infinita com `IntersectionObserver`.
- **Dados persistidos:** `mail-cache/<id>.json` (`version` 1, `account`
  = `usuário@servidor:porta`, `folders`, `lists` com `path`, `total`,
  `messages`). Ilegível ou de outra versão: ignorado e regravado. Fora do
  backup e da sincronização.
- **Comandos e permissões:** `mail_cached(id)` novo (capability
  `default`, `api.ts`, `devMock.ts`). `mail_list` troca `page` por
  `skip` (quantas já estão na tela).
- **Concorrência:** leitura e gravação fora da thread principal
  (`spawn_blocking` do leitor), lock próprio do cache, nunca junto do
  `AppState`.
- **Segurança e privacidade:** remetentes, assuntos e o começo do texto
  das 50 mais novas ficam no disco sem cifra, como os contatos (spec 25),
  em arquivo 0600. O comando exige o app desbloqueado.
- **Plataformas:** iguais.

## Tarefas

1. [x] Modelo e regras do cache (R3, R4) — testes `mail_cache::*`.
2. [x] `list_from` com `skip` (R5) — teste `lists_from_offset`.
3. [x] Arquivo por conta e `mail_cached` (R1, R4).
4. [x] Leitor mostra o guardado e revalida (R1, R2, R7).
5. [x] Rolagem infinita com erro e repetição (R5, R6).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | não verificado no app | — | — |
| R2 | não verificado no app | — | — |
| R3 | testes `keeps_recent_folders`, `trims_messages` | — | — |
| R4 | teste `other_account_is_ignored`; `mail::forget` | — | — |
| R5 | teste `lists_from_offset`; rolagem não verificada no app | — | — |
| R6 | não verificado no app | — | — |
| R7 | não verificado no app | — | — |
