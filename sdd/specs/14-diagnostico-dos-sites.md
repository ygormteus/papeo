# 14 — Diagnóstico dos sites

- **Estado:** em andamento (implementado; falta o teste no app)
- **Versão:** —
- **Relacionadas:** 02, 03, 09; arquitetura §7.3 e §14 (risco 1)

## Problema

Resposta rápida, contador de não lidas, abrir conversa ou e-mail e detecção
de "desconectado" dependem de seletores do DOM de cada site. Quando o
WhatsApp, o Telegram, o Gmail ou o Outlook mudam a página, o recurso para de
funcionar **em silêncio**: o shim devolve vazio e nada avisa. Hoje o
problema só aparece quando alguém repara e reclama.

## Fora do escopo

- Corrigir seletores sozinho ou baixá-los de um servidor.
- Telemetria: nada sai do computador.
- Páginas de teste por site no Autoteste (decisão: os testes do shim com
  DOM falso bastam).

## Requisitos

- **R1.** O Papeo deve verificar, em cada conta carregada de um site
  conhecido, quais pontos de apoio do site foram encontrados:

  | Site | Pontos |
  |---|---|
  | WhatsApp, Telegram | lista de conversas, busca; com uma conversa aberta: cabeçalho e campo de texto |
  | Gmail | caixa de entrada; com um e-mail aberto: assunto |
  | Outlook | caixa de entrada, busca; com um e-mail aberto: assunto |
  | Roundcube | caixa de entrada, busca |

  Um ponto só é julgado quando a tela em que ele vive está aberta; senão
  fica "não avaliado", e não "ausente": busca só com a lista de conversas
  à vista; campo de texto só com o nome da conversa no cabeçalho; Caixa de
  Entrada do Outlook só com a lista de pastas; assunto só com um e-mail
  aberto (no Outlook, fora de um rascunho); Roundcube só na lista de
  e-mails.
- **R2.** O shim deve enviar o resultado junto do `service_status`
  existente (campo `probes`), sem comando novo para os sites.
- **R3.** Quando o cabeçalho, o campo de texto ou a busca de uma conta de
  WhatsApp ou Telegram continuarem ausentes numa leitura feita 10 min ou
  mais depois da primeira ausência (sem nenhuma presença nesse meio-tempo,
  com pelo menos 3 ausências relatadas e, para o campo de texto, ausente
  em 3 conversas diferentes, porque canais e grupos só de avisos não têm
  campo), o Papeo deve esconder
  "Responder" daquela conta (notificação e mini janela) e recusar resposta
  rápida para ela. Quando o ponto voltar a ser encontrado, "Responder"
  volta.
- **R4.** O Papeo deve mostrar em Configurações → Sistema → Diagnóstico dos
  sites uma tabela por conta carregada com cada ponto (encontrado /
  ausente / ausente há 10 min ou mais / não avaliado) e um botão "Copiar
  relatório". O relatório não leva nome de conversa, assunto, endereço nem
  nome da conta (só tipo de site, pontos e a versão do Papeo).
- **R5.** Onde o "modo de diagnóstico" estiver ligado (desligado por
  padrão), quando a conta visível tiver um ponto ausente há 10 min ou mais,
  a barra do topo deve mostrar um aviso discreto que abre o diagnóstico.
  Fora desse modo, nenhum aviso aparece no uso normal.
- **R6.** O diagnóstico não deve ler nem guardar texto das páginas, só a
  presença dos elementos (booleanos).
- **R7.** Enquanto a conta estiver desconectada, ou nos primeiros 30 s da
  página, o Papeo não deve avaliar os pontos.
- **R8.** O estado dos pontos fica só na memória e recomeça quando a
  página da conta é recriada (hibernar, recarregar, reabrir o app).

## Limites conhecidos

- Campo de texto quebrado num site onde a pessoa só abre uma ou duas
  conversas: não é detectado (a resposta rápida falha com a mensagem de
  sempre). Abrir outras conversas, inclusive pela própria resposta rápida,
  completa a detecção.
- Se o próprio sinal de "conversa aberta" quebrar (`#main` no WhatsApp,
  endereço no Telegram), cabeçalho e campo ficam "não avaliado".

## Perguntas

- [x] O aviso do R5 aparece para todos? **Não: só no modo de diagnóstico.**
- [x] O R3 desliga o recurso ou só avisa? **Esconde "Responder".**
- [x] Páginas de teste no Autoteste? **Não, só os testes do shim.**

## Design

- **Shim (`notification_shim.js`):** `PROBES` por site reaproveita as
  mesmas constantes das funções (`SEARCH_BOX`, `CHAT_HEADER`, `COMPOSER`,
  `CHAT_TITLES`, `MAIL_SEARCH`, `MAIL_SUBJECT`, `MAIL_UNREAD`). Cada ponto
  devolve `true`, `false` ou `null` (não avaliado). Vai em
  `service_status.probes` só depois de 30 s da carga do shim e fora da
  tela de login. O `key` da verificação de 5 s inclui os pontos, para uma
  mudança ser enviada logo.
- **Núcleo (`papeo_core::probes`):** `Probe` (`list`, `search`, `header`,
  `composer`, `inbox`, `subject`), `probes_for(kind)`, `REPLY_NEEDS`,
  `ProbeTracker` (desde quando cada ponto está ausente, quantas ausências e
  se faltou com outra conversa aberta; `null` não muda nada; `true` zera),
  `MISSING_AFTER_MS` = 10 min, `MISSING_MIN_REPORTS` = 3,
  `COMPOSER_MIN_CHATS` = 3, `can_reply()` e `view()`. O estado só muda com
  um relatório (o tempo passando sozinho não confirma ausência). A conversa entra só como resumo (hash), nunca o nome. Puro e testável. Pontos fora da lista do tipo são ignorados.
- **App:** `Health.probes: ProbeTracker` (recriado com a página).
  `notify::service_status` atualiza o rastreador só com a página do próprio
  site, sem `logged_out` e com até 16 chaves. `Inner::can_reply(id)` junta
  `supports_chat()` e o rastreador e é usado em `notify.rs` (botão da
  notificação), `state.rs` (`RecentView.can_reply`) e `reply.rs` (recusa).
  `ServiceView.probes: Vec<ProbeView>` (vazio sem página ou sem pontos).
  Toda mudança visível vem de um `service_status`, que já publica.
- **Configuração:** `Settings.diagnostics: bool` (`serde(default)` →
  falso; arquivos antigos continuam válidos). Alterada por
  `set_preferences` (`diagnostics`). Entra no backup como as outras
  preferências.
- **Interface:** seção "Diagnóstico dos sites" na aba Sistema de
  `Settings.svelte` (interruptor, tabela, Copiar relatório); aviso na
  `Topbar` com o modo ligado; `api.ts` (tipos) e `devMock.ts`
  (`?diagnostico`).
- **Contratos:** formato do `service_status` (shim e `notify.rs`),
  `Snapshot` (`api.ts` e `devMock.ts`), `Settings` (persistido). Nenhum
  comando novo; capability `services` sem mudança.
- **Concorrência:** tudo dentro do lock já tomado por `service_status`; nada
  espera a thread principal.

## Tarefas

1. [x] `papeo_core::probes` + `Settings.diagnostics` (R1, R3, R7–R8) —
   testes de unidade (`probes.rs`) e leitura de configuração antiga.
2. [x] Shim: `PROBES` e campo `probes` (R1–R2, R6–R7) — teste
   `tests/probes-shim.test.mjs`.
3. [x] App: rastreador na saúde, `can_reply`, `ServiceView.probes`,
   publicação quando o estado muda (R3, R8).
4. [x] Interface: diagnóstico nas Configurações, aviso na barra do topo,
   mock `?demo&diagnostico` (R4–R5).
5. [x] Arquitetura §4.1, §5.2, §7.3, §14 e §15 atualizadas.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R2, R6, R7 | `probes-shim.test.mjs` (WhatsApp, aba Status, Telegram com e sem cabeçalho e canal, Gmail com busca, Outlook com pastas e rascunho, Roundcube escrevendo, Discord) | não verificado (sites reais) | não verificado |
| R3 | testes de `probes.rs` (leitura 10 min depois, tempo sozinho não confirma, campo de texto em 3 conversas, recuperação) | não verificado (uso real) | não verificado |
| R4, R5 | `pnpm check`; mock `?demo&diagnostico` | não verificado (visual) | não verificado |
| R8 | leitura do código (`Health::new` ao criar/recarregar, remoção ao hibernar) | — | — |
