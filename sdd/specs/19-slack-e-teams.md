# 19 — Slack e Microsoft Teams

- **Estado:** entregue (sem versão)
- **Versão:** —
- **Relacionadas:** 01, 02, 14

## Problema

Slack e Teams só entram como "Outro site": sem ícone nem cor próprios, sem
endereço pronto, sem aviso de conta desconectada, e no Teams sem a escolha
entre conta pessoal e de trabalho.

## Fora do escopo

- Resposta rápida, conversas recentes e abrir conversa (como no Discord:
  só WhatsApp e Telegram).
- Contador por seletores da página (só o do título, como nos outros sites).
- Chamadas de vídeo além do que o site faz na webview.

## Requisitos

- **R1.** O Papeo deve oferecer Slack e Microsoft Teams ao adicionar uma
  conta, com nome, ícone, cor e endereço próprios.
- **R2.** Ao adicionar o Teams, o Papeo deve perguntar o tipo de conta:
  pessoal (`teams.live.com`) ou trabalho/escola (`teams.microsoft.com`);
  "Adicionar outra conta" mantém o endereço.
- **R3.** O Papeo deve liberar microfone e câmera nesses sites por padrão
  (chamadas e mensagens de voz), como nos mensageiros.
- **R4.** Quando a página do Slack for a de entrar (`/signin`,
  `/sign_in_with_password`, `/workspace-signin`, `/get-started`,
  `/ssb/signin`, `/checkcookie`, ou a raiz com o formulário de entrar) ou
  a do Teams/Outlook cair no login da Microsoft
  (`login.microsoftonline.com`, `login.live.com`), o Papeo deve marcar a
  conta como desconectada. O login da Microsoft não libera avisos nem
  microfone/câmera.
- **R5.** O contador deve vir do título da página ("(N)"), como nos outros
  sites.
- **R6.** Onde o Teams ou o Outlook de trabalho passarem para o endereço
  novo do mesmo produto (`teams.cloud.microsoft`,
  `outlook.cloud.microsoft`), o Papeo deve continuar aceitando avisos,
  "desconectado" e microfone/câmera da página (mesmo site). Outro produto
  da Microsoft (`forms.cloud.microsoft`, Teams ↔ Outlook) não conta.

## Design

- **Núcleo:** `ServiceKind::Slack` (`https://app.slack.com/client`) e
  `ServiceKind::Teams` (`https://teams.microsoft.com/v2/`), nome padrão
  e mídia "permitir" (regra atual: só e-mail e personalizados perguntam).
- **Privacidade (`same_site`):** apelidos exatos (`MICROSOFT_ALIASES`):
  `teams.microsoft.com` ↔ `teams.cloud.microsoft`, e `outlook.office.com`
  / `outlook.office365.com` ↔ `outlook.cloud.microsoft`; `live.com`
  (pessoal) continua à parte. `is_microsoft_login` vale só para o
  "desconectado" de Teams/Outlook em `service_status`.
- **Shim:** páginas de entrar do Slack em `LOGGED_OUT`; o Teams já cai em
  `MICROSOFT_LOGIN`.
- **Interface:** `api.ts` (`ServiceKind`, `KIND_META`, `TEAMS_PERSONAL_URL`,
  `keepsUrl`, `mediaPolicy`), `AddServiceDialog.svelte` (tipos e conta do
  Teams), `ServiceAvatar.svelte` (ícones), `devMock.ts`.
- **Dados persistidos:** dois valores novos de `ServiceKind` em
  `services.json`. **Compatibilidade:** uma versão anterior do Papeo não
  lê o valor novo e, por isso, não lê o `services.json` inteiro: o arquivo
  vai para `.json.corrompido` e o app abre com as contas padrão. Restaurar
  numa versão anterior um backup com Slack ou Teams também é recusado.
  Avisar nas notas da versão (voltar de versão depois de adicionar Slack
  ou Teams perde a lista de contas, que pode ser recuperada do
  `.json.corrompido` ao atualizar de novo).
- **Comandos:** nenhum novo.

## Tarefas

1. [x] Núcleo: tipos novos (R1, R3) — teste de nome, endereço, mídia e
   JSON.
2. [x] Shim: desconectado do Slack (R4) — teste do shim.
3. [x] Interface: tipos, conta do Teams, ícones (R1, R2).

## Verificação

Revisão crítica independente: sem bloqueante; 1 alto (agrupamento largo
dos domínios da Microsoft), 2 médios (texto de compatibilidade invertido;
Teams/Outlook de trabalho nunca "desconectado") e 1 baixo (páginas de
entrar do Slack faltando), todos corrigidos. Re-checagem aprovada; acrescentados
testes da raiz do Slack. Aceitos: outros logins (ADFS, Okta,
`login.microsoft.com`) não marcam "desconectado"; `/get-started` e
`/checkcookie` podem marcar por um instante.

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R3 | teste de tipos em `service.rs` (nome, endereço, mídia, JSON) | não verificado | não verificado |
| R2 | `AddServiceDialog.svelte`; mock | não verificado | — |
| R4 | `tests/email-shim.test.mjs` (Slack); `is_microsoft_login` em `privacy.rs`; `notify::service_status` | não verificado (login real) | não verificado |
| R5 | contador do título (comum a todos os sites) | não verificado | não verificado |
| R6 | testes de `same_site` em `privacy.rs` | não verificado | não verificado |
