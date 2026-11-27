# 14 — Diagnóstico dos sites

- **Estado:** proposta
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

## Requisitos

- **R1.** O Papeo deve verificar, em cada conta carregada, quais pontos de
  apoio do site foram encontrados: lista de conversas, cabeçalho da
  conversa, campo de texto, botão Enviar, busca, contador da Caixa de
  Entrada, assunto aberto. Cada site verifica só os seus.
- **R2.** O shim deve enviar o resultado junto do `service_status`
  existente, sem comando novo para os sites.
- **R3.** Quando um ponto de apoio necessário a um recurso ligado ficar
  ausente por 10 min seguidos com a conta conectada e a página carregada,
  o Papeo deve marcar o recurso como "indisponível neste site" e
  desligá-lo para aquela conta (por exemplo, esconder "Responder" na
  notificação).
- **R4.** O Papeo deve mostrar em Configurações → Sistema → Diagnóstico uma
  tabela por conta com cada ponto (encontrado / não encontrado / não se
  aplica), a versão do Papeo e um botão "Copiar relatório" (texto sem
  conteúdo de mensagens nem nomes de conversas).
- **R5.** Se algum ponto estiver ausente, então a conta deve mostrar um
  aviso discreto na barra do topo com link para o diagnóstico, uma vez por
  versão do Papeo.
- **R6.** O diagnóstico não deve ler nem guardar texto das conversas, só a
  presença dos elementos.
- **R7.** Enquanto a conta estiver desconectada ou carregando, o Papeo não
  deve avaliar os pontos.

## Perguntas

- [ ] O aviso do R5 aparece para todos ou só com uma opção "modo de
  diagnóstico"?
- [ ] O R3 desliga o recurso ou só avisa?
- [ ] Incluir no Autoteste uma página de teste por site com o DOM conhecido
  (como os testes do shim) para pegar regressão no próprio shim?

## Design (rascunho)

- **Shim:** uma tabela `PROBES` por site com os seletores que já existem
  (mesmas constantes usadas pelas funções, para não duplicar). A cada
  `service_status`, um campo `probes: {nome: bool}` com os pontos do site.
- **Núcleo:** `papeo_core::probes` com a lista de pontos por
  `ServiceKind`, a regra de "ausente há 10 min" e qual recurso depende de
  qual ponto. Puro e testável.
- **App:** `notify::service_status` guarda `probes` e o instante da
  primeira ausência em `Inner` (só na memória). `Snapshot` ganha
  `health.probes` por conta. `notifications.rs` consulta antes de oferecer
  "Responder".
- **Interface:** aba Diagnóstico em `Settings.svelte`; aviso na `Topbar`.
- **Contratos:** formato do `service_status` (shim e `notify.rs`),
  `Snapshot` (`api.ts` e `devMock.ts`). Nenhum comando novo para os sites;
  a capability `services` não muda.
- **Privacidade:** só booleanos; o relatório copiado não leva nomes.

## Tarefas (rascunho)

1. [ ] `PROBES` no shim e campo `probes` no `service_status` (R1–R2, R6) —
   teste: `notification-shim.test.mjs` e `email-shim.test.mjs` com DOM
   completo e com DOM sem cada ponto.
2. [ ] `papeo_core::probes` com a regra de ausência (R3, R7) — teste de
   unidade.
3. [ ] Estado e `Snapshot` (R3) — teste em `state.rs`.
4. [ ] Aba Diagnóstico e aviso na barra do topo (R4–R5) — mock `?diagnostico`.
5. [ ] "Responder" escondido quando indisponível (R3) — teste em
   `notifications.rs`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R7 | — | — | — |
