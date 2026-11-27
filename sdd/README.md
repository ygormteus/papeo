# SDD do Papeo

Esta pasta junta duas coisas com a mesma sigla:

- **Software Design Document** — [`arquitetura.md`](arquitetura.md): como o
  Papeo é construído hoje (componentes, dados, interfaces, fluxos,
  segurança, decisões e riscos). É o mapa para quem chega ao código.
- **Spec-Driven Development** — [`specs/`](specs/): uma especificação por
  recurso, escrita **antes** do código, que vira a fonte de verdade do
  comportamento. O código e os testes seguem a especificação; quando um
  dos dois muda, o outro acompanha no mesmo commit.

## Como um recurso nasce

1. **Requisitos.** Copie [`specs/_modelo.md`](specs/_modelo.md) para
   `specs/NN-nome.md` (próximo número livre). Escreva o problema, quem
   usa, o que fica de fora e os requisitos no formato EARS (abaixo), cada
   um com um ID (`R1`, `R2`…). Estado: `proposta`.
2. **Aprovação.** Quem decide o produto revisa só os requisitos. Dúvidas
   abertas ficam na seção "Perguntas" até serem respondidas; nada de
   código com pergunta aberta que muda comportamento.
3. **Design.** Como os requisitos viram código: módulos tocados, dados
   persistidos (e compatibilidade com arquivos antigos), comandos e
   capabilities, concorrência (locks, thread principal), segurança.
   Confira o impacto nos contratos listados em `arquitetura.md` §12.
   Estado: `aprovada`.
4. **Tarefas.** Lista curta e ordenada, cada uma pequena o bastante para um
   commit, com o teste que prova o requisito (`R3 → teste x`).
5. **Implementação.** Commits citam a especificação
   (`feat: … (spec 14, R2–R4)`). Mudou de ideia no meio? Atualize a
   especificação primeiro.
6. **Verificação.** Preencha a tabela de verificação: cada requisito com a
   evidência (teste automático, autoteste, conferência manual em macOS e
   Windows) ou marcado como **não verificado**. Estado: `entregue` com a
   versão em que saiu.

As especificações dos recursos que já existiam antes desta pasta foram
escritas depois do código (estado `entregue`, "retroativa"): descrevem o
comportamento atual e servem de base para mudanças.

## Formato dos requisitos (EARS)

Uma frase por requisito, testável, com um destes moldes:

| Molde | Quando usar |
|---|---|
| O Papeo deve … | sempre vale |
| Quando *gatilho*, o Papeo deve … | reação a um evento |
| Enquanto *estado*, o Papeo deve … | vale durante um estado |
| Se *condição indesejada*, então o Papeo deve … | erro, falha, abuso |
| Onde *opção/plataforma*, o Papeo deve … | só com a opção ligada ou num sistema |

Ruim: "as notificações devem ser rápidas". Bom: "Quando um serviço
notificar menos de 1,5 s depois da notificação anterior dele, o Papeo deve
descartar a nova."

## Estados

`proposta` → `aprovada` → `em andamento` → `entregue (vX.Y)`; também
`em espera` (decisão de produto) e `descartada` (com o motivo).

## Índice

Ver [`specs/README.md`](specs/README.md).
