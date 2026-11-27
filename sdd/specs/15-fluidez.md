# 15 — Fluidez (segunda leva de desempenho)

- **Estado:** em andamento (implementado; falta o uso no app)
- **Versão:** —
- **Relacionadas:** 06, 08, 11, 12; arquitetura §9 e §14 (risco 6)

## Problema

A revisão geral de 2026-11-26 deixou quatro pontos em que o Papeo segura o
lock do estado, a thread principal ou um trabalhador do runtime `async`
durante uma operação lenta (disco, espera da thread principal). Com muitas
contas, ou num disco lento, isso aparece como pequenas travadas da
interface. Não muda memória: o peso continua sendo o dos sites.

## Fora do escopo

- Uso de memória dos sites (limite de recarga, hibernação).
- Mudança de comportamento visível: só o tempo e a ordem do trabalho
  mudam.

## Requisitos

- **R1.** Quando o Papeo medir a memória das contas, deve pedir o processo
  de todas as webviews à thread principal de uma vez e esperar no máximo 2 s
  no total (hoje: até 2 s por conta, uma depois da outra), fora de um
  trabalhador do runtime `async`.
- **R2.** Quando um site começar um download, o Papeo não deve consultar o
  disco (nome livre na pasta) com o lock do estado tomado.
- **R3.** Exportar, conferir e restaurar backup devem ler e gravar o arquivo
  fora dos trabalhadores do runtime `async`.
- **R4.** A sincronização deve juntar as tarefas fora do lock do estado. Se
  as tarefas mudarem enquanto junta, deve juntar de novo com o estado novo
  (até 3 vezes; depois, tenta na próxima rodada), sem perder a mudança
  feita no meio.

## Design

- **R1 (`memory.rs`):** `report` dispara `with_webview` para todas as
  webviews e depois recebe as respostas com um prazo único de 2 s.
  `get_memory` roda `report` em `spawn_blocking`.
- **R2 (`downloads.rs`):** copia os caminhos pendentes sob o lock, solta o
  lock, calcula `unique_path` e toma o lock de novo para registrar. Pedidos
  de download dos sites não correm entre si (thread principal), mas salvar
  um anexo (`attachment::save_to_downloads`) pode: se o nome escolhido foi
  pego nesse meio-tempo, escolhe de novo, aí com o lock (raro).
- **R3 (`commands.rs`):** `export_backup` e `read_backup` em
  `spawn_blocking`.
- **R4 (`sync.rs`):** cópia das tarefas e das contas sob o lock, junção
  fora, e aplicação só se `inner.organizer` ainda for igual à cópia; senão
  repete. Na terceira mudança seguida, desiste da rodada sem erro na tela e
  marca para sincronizar de novo (nada aplicado nem enviado).
- **Concorrência:** nada novo segura o lock durante espera da thread
  principal ou disco.

## Tarefas

1. [x] R1 — medição em lote.
2. [x] R2 — nome do download fora do lock, com nova escolha se um anexo
   pegou o mesmo nome no meio.
3. [x] R3 — backup fora do runtime (e restaurar confere de novo o bloqueio
   depois de ler o arquivo).
4. [x] R4 — junção fora do lock com nova tentativa.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1–R4 | clippy, testes do núcleo e do app, revisão crítica (concorrência) | não verificado (app) | clippy no CI |
| R1 (ganho medido) | — | não medido | não medido |
