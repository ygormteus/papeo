# 32 — Conta em janela separada

- **Estado:** implementada
- **Versão:** —
- **Relacionadas:** 30 (tela dividida), 05 (bloqueio), 09/29 (leitores),
  12 (hibernação)

## Problema

Quem usa dois monitores, ou quer o WhatsApp de um lado e o e-mail do outro
em janelas que se movem à parte, só tem a janela principal (e a tela
dividida dentro dela).

## Fora do escopo

- Lembrar as janelas separadas entre aberturas do app (cada uma volta para
  a janela principal ao fechar o Papeo).
- Barra superior (zoom, recarregar) dentro da janela separada: ficam no
  menu da conta na janela principal.
- Arrastar a conta para fora da janela.

## Requisitos

- **R1.** "Abrir em janela separada" (menu da conta e botão da barra de
  cima) deve abrir a conta numa janela própria do sistema, com o nome da
  conta no título e o tamanho da área de conteúdo atual.
- **R2.** Conta pelo site: a mesma página passa para a janela nova, sem
  recarregar (a conversa aberta continua). Conta pelo leitor do Papeo
  (e-mail, Talk): a janela tem a própria tela do leitor, criada ao abrir e
  desfeita ao fechar.
- **R3.** Fechar a janela separada, ou "Voltar para a janela principal",
  devolve a conta à janela principal (pelo site, a página continua
  carregada).
- **R4.** Na janela principal, escolher a conta separada (barra lateral,
  aviso, busca) traz a janela dela para a frente; a conta separada sai da
  tela dividida e não entra nela.
- **R5.** A conta separada conta como "na tela" (não hiberna sozinha); com
  a janela dela em foco, não há aviso da conversa aberta nela.
- **R6.** Com o Papeo bloqueado, as janelas separadas somem e voltam ao
  desbloquear. Proteção de tela e tema valem para elas também.
- **R7.** Remover, hibernar ou apagar os dados da conta fecha a janela
  separada antes.
- **R8.** Sem janela separada, nada muda (nenhum custo).

## Design

- **Estado** (`state.rs`, só na memória): `Inner.detached: HashSet<id>`,
  `Inner.detached_focus` (conta cuja janela está em foco) e
  `detached_hidden` (escondidas pelo bloqueio). `on_screen` inclui as
  separadas; `in_main` é a janela principal (ativa ou tela dividida); o
  aviso some com a principal em foco (`in_main`) ou a janela separada da
  conta em foco (`notify.rs`). `ServiceView.detached`.
- **Módulo** `src-tauri/src/detach.rs`: janela `win-<id>-<n>` (número
  novo a cada abertura); site: a webview da conta passa para ela com `Webview::reparent` (e volta antes de a
  janela fechar); leitor: webview local `rd-<id>` com a interface montando
  só o leitor (`DetachedReader.svelte`, `__PAPEO_VIEW = "reader"`).
  `create_service_webview` entrega a webview nova (recarregar, acordar) à
  janela separada (`adopt`).
- **Comandos:** `detach_service(id)`, `attach_service(id)` (capability
  `default`, que passa a valer também para `rd-*`; `api.ts`, `devMock.ts`).
- **Concorrência:** janelas e webviews criadas, movidas e fechadas sempre
  sem `AppState::lock()`.
- **Peso:** site: só a janela (a página é a mesma). Leitor: uma segunda
  interface (~30–60 MB) enquanto a janela existir.

## Tarefas

1. [x] Estado, módulo e comandos.
2. [x] Interface: menu, botão, marca na barra lateral, leitor separado.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R8 | não verificado no app | — | — |
| Concorrência, ACL | revisão crítica independente: sem deadlock; `rd-*` não fica ao alcance de sites (ids de serviço são `svc-` + hex). Corrigidos: restaurar backup e "Esqueci o PIN" fecham as janelas separadas; trocar site/leitor (IMAP, Talk) volta à principal; rótulo novo a cada janela (o fechamento atrasado de uma não desfaz a nova); limite de memória não recarrega conta separada; bloqueio durante a abertura; site do leitor escondido; desfaz a abertura se falhar; janela principal fechada de vez fecha as separadas | — | — |
