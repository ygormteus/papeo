# 30 — Tela dividida

- **Estado:** implementada
- **Versão:** 0.17.0
- **Relacionadas:** 01 (contas), 02 (não lidas), 09/29 (leitores), 12
  (hibernação)

## Problema

Quem atende por duas contas ao mesmo tempo (dois WhatsApp, dois e-mails,
WhatsApp e e-mail) precisa alternar na barra lateral a cada mensagem.

## Fora do escopo

- Três ou mais painéis; divisão em cima e embaixo.
- A mesma conta nos dois lados (o WhatsApp Web só aceita uma sessão).
- Guardar a divisão entre aberturas do app (só a proporção fica guardada).

## Requisitos

- **R1.** No menu da conta (barra lateral), "Abrir ao lado" deve mostrar a
  conta à direita da que está aberta; a escolhida fica em foco. Só aparece
  com outra conta aberta.
- **R2.** As duas contas ficam visíveis lado a lado, cada uma com o site ou
  com o leitor do Papeo (e-mail, Talk). Um divisor arrastável muda a
  proporção (entre 25% e 75%); a proporção fica guardada neste computador.
- **R3.** Uma das metades está em foco: a barra superior (zoom, recarregar,
  hibernar, site/leitor) e os atalhos do leitor agem nela. Clicar numa
  metade a põe em foco; a barra lateral marca as duas, a em foco mais forte.
- **R4.** Escolher outra conta na barra lateral troca a conta da metade em
  foco; escolher uma das duas só muda o foco. Ir para o Início, ou
  remover, hibernar ou apagar os dados de uma das duas, fecha a divisão.
- **R5.** "Fechar divisão" (barra superior) volta a uma conta só, a em
  foco.
- **R6.** As duas contas contam como "na tela": sem aviso da conversa aberta
  em nenhuma delas, sem hibernar, e o alarme de "não responde" vale para
  as duas.
- **R7.** Sem divisão, nada muda (nenhum custo de memória ou consulta).

## Design

- **Estado** (`state.rs`, só na memória): `Inner.split: Option<Split>`
  (`left`, `right`, `ratio`). `active` continua sendo a conta em foco e é
  sempre uma das duas. `Inner::on_screen(id)` substitui as comparações com
  `active` que significam "visível"; `Inner::pane_area(id)` dá o retângulo
  de cada metade (área do conteúdo dividida pela proporção, com divisor de
  `SPLIT_GAP` px). O `Snapshot` ganha `split: {left, right, ratio} | null`.
- **Comandos:** `split_open(id, ratio)`, `split_close()`,
  `split_ratio(ratio)` (capability `default`; `api.ts`, `devMock.ts`).
  `activate` troca a metade em foco quando a conta não é uma das duas.
- **Foco por clique no site:** o shim avisa `service_status` com
  `focus: true` quando a página ganha foco; com divisão, a conta vira a em
  foco (sem publicar se já era).
- **Interface:** `App.svelte` desenha as metades com o mesmo cálculo do
  backend (leitores dentro delas) e o divisor; `MailReader` recebe
  `focused` para os atalhos; `Sidebar` marca a conta da outra metade;
  `Topbar` ganha "Fechar divisão".
- **Concorrência:** nada novo; criar a webview segue fora do lock.
- **Peso:** a outra conta já costuma estar carregada; a divisão só deixa as
  duas visíveis (sem desaceleração de timers na metade de trás).

## Tarefas

1. [x] Estado, retângulos e comandos.
2. [x] Usos de `active` como "visível" passam por `on_screen`.
3. [x] Interface: metades, divisor, foco, menu, barra.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R2 | teste `split_panes_divide_the_area` | não verificado (app) | — |
| R1, R3–R6 | não verificado no app | — | — |
| Concorrência, R7 | revisão crítica independente: sem deadlock nem estado inconsistente; corrigidos o aviso de foco tratado como sinal de vida (agora só muda o foco), a metade de trás em branco ao tirar o IMAP, o leitor recarregando ao dividir/fechar, "apagar dados" na metade em foco e o arraste com moldura | — | — |
