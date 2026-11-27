# NN — Nome do recurso

- **Estado:** proposta
- **Versão:** —
- **Relacionadas:** —

## Problema

Quem sofre com o quê hoje, em duas ou três frases. Sem solução ainda.

## Fora do escopo

- O que este recurso deliberadamente não faz.

## Requisitos

- **R1.** Quando …, o Papeo deve …
- **R2.** Se …, então o Papeo deve …

## Perguntas

- [ ] Pergunta em aberto (quem responde, o que muda conforme a resposta).

## Design

- **Módulos:** arquivos e o papel de cada um na mudança.
- **Dados persistidos:** campos novos, valor padrão, leitura de arquivos
  antigos, entra ou não no backup e na sincronização.
- **Comandos e permissões:** comandos novos em `COMMANDS` (`build.rs`), em
  qual capability, no `api.ts` e no `devMock.ts`.
- **Concorrência:** o que roda fora da thread principal; nada de
  `AppState::lock()` durante `add_child`, `is_focused`, `with_webview` com
  espera ou métodos de bandeja/menu.
- **Segurança e privacidade:** o que um site remoto consegue fazer com isso;
  dados novos em disco ou na rede.
- **Plataformas:** diferenças entre macOS e Windows.

## Tarefas

1. [ ] Tarefa pequena (R1) — teste: …
2. [ ] …

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | teste `nome_do_teste` | — | — |
| R2 | não verificado | — | — |
