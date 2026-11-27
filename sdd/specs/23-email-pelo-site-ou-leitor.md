# 23 — E-mail pelo site ou pelo leitor

- **Estado:** entregue
- **Versão:** 0.16.0
- **Relacionadas:** 01, 09

## Problema

Desde o leitor de e-mail (spec 09), toda conta de e-mail nova (Gmail,
Outlook, outro e-mail) abre direto na configuração do IMAP. Quem quer o
site, como antes, não encontra a saída ("Usar o site" fica escondido) e
acha que o login normal sumiu.

## Fora do escopo

- Mudar contas já adicionadas (cada uma continua como está).

## Requisitos

- **R1.** Ao adicionar Gmail, Outlook ou outro e-mail, o Papeo deve
  perguntar "Abrir como": Site ou Leitor do Papeo, sem nada marcado; só
  depois da escolha dá para adicionar (decisão do usuário, 2026-11-27).
- **R2.** Com "Site", a conta deve abrir o site do provedor, como os outros
  serviços, sem pedir IMAP.
- **R3.** Com "Leitor do Papeo", o Papeo deve seguir como antes: abre a
  configuração do IMAP em seguida.
- **R4.** "Adicionar outra conta" deve repetir a forma da conta de origem
  (site ou leitor).

## Design

- `add_service` ganha `reader: Option<bool>`: só com `Some(true)` a conta
  entra em `mail_setup`. `api.addService(…, reader)`, `devMock.ts`.
- `AddServiceDialog.svelte`: grupo "Abrir como", reiniciado ao trocar o
  tipo; botão "Adicionar" desligado até escolher. `App.addService` só abre
  o `MailDialog` com `reader`.
- **Comandos:** nenhum novo (argumento opcional; ausente = site).

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R4 | `AddServiceDialog.svelte`, `App.svelte`, `commands::add_service`; mock | não verificado | não verificado |
