# 01 — Contas e sessões

- **Estado:** entregue (0.1–0.13), retroativa
- **Relacionadas:** 11 (backup), 12 (memória)

## Problema

Quem usa vários mensageiros, ou duas contas do mesmo, precisa de várias
abas ou navegadores e mistura logins.

## Fora do escopo

- Importar contas de outro app (Ferdium, Rambox).
- Sincronizar logins entre computadores.

## Requisitos

- **R1.** O Papeo deve abrir cada conta numa webview própria, com sessão
  isolada (cookies e armazenamento separados).
- **R2.** Quando a pessoa adicionar um tipo que já existe, o Papeo deve
  criar outra conta com sessão nova e nome único ("WhatsApp 2").
- **R3.** O Papeo deve aceitar os tipos WhatsApp, Telegram, Discord, Gmail,
  Outlook (pessoal ou trabalho/escola), Outro e-mail e Outro site (URL
  `http(s)`).
- **R4.** O Papeo deve permitir mudar nome (até 40 caracteres), cor
  (`#rrggbb`), emoji (ou até 2 letras), zoom (50–200 %), contexto
  (trabalho/pessoal), política de mídia e ordem na barra lateral.
- **R5.** Quando a pessoa remover uma conta, o Papeo deve apagar os dados
  da sessão dela. Se a pasta estiver presa, deve tentar de novo até 5
  vezes e outra vez na próxima abertura, sem nunca apagar uma sessão usada
  por uma conta atual.
- **R6.** Onde o sistema for macOS, o Papeo deve se identificar como Safari
  (o WhatsApp recusa o WKWebView sem esse token).
- **R7.** Quando o app abrir, o Papeo deve reabrir as contas que estavam
  carregadas, a primeira na hora e as demais espaçadas.
- **R8.** Se uma conta mostrar a tela de login, então o Papeo deve marcá-la
  como desconectada.
- **R9.** Se a conta visível, com a janela focada, ficar 75 s sem sinal do
  site, então o Papeo deve marcá-la como "não responde".
- **R10.** O Papeo deve aceitar `id` e `session` só no formato gerado pelo
  app (`svc-` + 12 hex; 32 hex), porque viram rótulo de webview e nome de
  pasta.

## Design

- `papeo_core::service` (modelo e validação), `webviews.rs` (criação,
  `data_directory` ou `data_store_identifier`), `sessions.rs` (remoção
  adiada em `pending_removal`), `notify.rs` (`service_status`) e
  `state::status_of`.
- Sinal de vida do shim a cada 20 s; a contagem recomeça ao ativar a conta
  ou focar a janela (`Health::grace`).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R2 | testes de `service.rs` (`unique_name`), uso real com 2 WhatsApp | ok | não verificado |
| R4 | testes de `normalize_color`/`normalize_emoji`/zoom | ok | — |
| R5 | `pending_removal` em teste; remoção real no Mac | ok | não verificado (pasta presa no WebView2) |
| R7 | uso real (2 contas reabertas) | ok | — |
| R8–R9 | testes do shim (sessão) | parcial | — |
