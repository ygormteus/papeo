# 20 — Interface em inglês

- **Estado:** entregue (sem versão)
- **Versão:** —
- **Relacionadas:** 02, 05, 07, 09, 17

## Problema

O Papeo só fala português. Quem usa o sistema em outro idioma vê tudo em
português: telas, bandeja, menu do macOS, notificações e erros.

## Fora do escopo

- Outros idiomas além de português e inglês (o formato do catálogo deixa
  espaço para eles).
- Datas escritas em inglês no texto da tarefa ("tomorrow 3pm"): o leitor
  de datas continua entendendo só português (spec 07).
- Texto que vem dos sites e dos servidores (páginas, mensagens de erro do
  IMAP/SMTP, nomes de pastas do servidor).
- Notas da versão ("Novidades") e textos de diagnóstico do autoteste.
- Nomes padrão de contas já criadas (a pessoa pode ter renomeado).
- Textos já guardados (marca de conflito da sincronização nas anotações).

## Requisitos

- **R1.** O Papeo deve usar inglês quando o idioma do sistema não for
  português, e português quando for (`pt`, `pt-BR`, `pt-PT`…).
- **R2.** O Papeo deve oferecer em Configurações → Geral a opção
  "Idioma": Sistema (padrão), Português, English.
- **R3.** Quando a pessoa trocar o idioma, a interface, a bandeja, a mini
  janela, as barras de prévia e de anexos e o menu do macOS devem mudar
  na hora, sem reiniciar.
- **R4.** As notificações do próprio Papeo (lembretes, resumo do silêncio,
  envio de e-mail, atualizações), os erros mostrados na tela e os diálogos
  nativos devem seguir o idioma.
- **R5.** Datas e horas devem seguir o idioma escolhido (formato `en-US`
  ou `pt-BR`).
- **R6.** O idioma do documento (`<html lang>`) deve acompanhar a escolha.
- **R7.** Todo texto da interface marcado para tradução deve ter versão
  em inglês; um teste deve falhar se faltar alguma.

## Design

- **Catálogo (estilo gettext):** a frase em português é a chave; o
  catálogo inglês mapeia português → inglês. Sem tradução, mostra o
  português. Parâmetros nomeados: `"{n} mensagens"`.
- **Interface:** `src/lib/i18n.svelte.ts` com `t(pt, vars?)`, idioma em
  `$state` e `locale()` para `toLocale*`; catálogos em `src/lib/i18n/`.
  O idioma vem do snapshot (`settings.language` + `lang` resolvido);
  antes do snapshot, `navigator.language`.
- **Rust:** `papeo_core::i18n` (sem Tauri) com `Lang`, idioma atual em
  atômico, `tr(pt)`, `trf(pt, vars)` e `tk(pt)`; catálogo em
  `crates/papeo-core/src/i18n/en_app.rs` (app) e `en_core.rs` (núcleo). Idioma do sistema pelo crate
  `sys-locale`.
- **Troca:** `set_preferences` ganha `language`; ao trocar, atualiza o
  atômico, refaz os botões das notificações (macOS), o título e a barra
  das janelas de prévia e de anexo abertas (`window.__papeoSetLang`),
  reconstrói o menu do macOS na thread principal (fora do lock do estado)
  e publica o snapshot (a bandeja entra o idioma na assinatura do menu e
  na dica).
- **Dados persistidos:** `Settings.language` (`"system" | "pt" | "en"`,
  padrão `"system"`; arquivo antigo lê `"system"`; versão antiga ignora
  o campo).
- **Comandos:** nenhum novo; `Snapshot` ganha `lang` (`"pt" | "en"`).
- **Comparações:** o erro "sem pasta Rascunhos" continua em português no
  backend (o editor compara) e é traduzido na tela.
- **Módulos puros testados no Node** (`tasks.ts`, `settingsIndex.ts`)
  não importam o `$state`: recebem o tradutor (`setTranslator`, terceiro
  argumento de `searchSettings`).
- **Teste (R7):** script em `tests/` que extrai os `t("…")`/`tr("…")` do
  código e confere que todos estão no catálogo inglês (e que o catálogo
  não tem chaves órfãs).

## Tarefas

1. [x] Núcleo: `i18n`, `Settings.language`, idioma do sistema (R1, R2).
2. [x] Rust: bandeja, menu, notificações, erros e diálogos (R3, R4).
3. [x] Interface: `t()` em todas as telas, datas, `<html lang>` (R3, R5,
   R6); opção em Configurações (R2).
4. [x] Teste de cobertura do catálogo (R7).

## Verificação

Revisão independente: sem bloqueante; 1 médio (bandeja e dica não
mudavam ao trocar o idioma) e 2 baixos (botões das notificações do macOS e
barras de prévia/anexo abertas ficavam no idioma antigo), corrigidos.

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | teste `resolves_locale` (`i18n/mod.rs`) | não verificado (idioma real do sistema) | não verificado |
| R2 | `Settings.svelte` (linha Idioma); teste de arquivo antigo em `settings.rs`; `tests/settings-index.test.mjs` | não verificado | — |
| R3 | `$effect(setLang)` em `App`/`Popover`; `menu::rebuild`; assinatura da bandeja; `lang::set_script` | não verificado | não verificado |
| R4 | `tr`/`trf` nas mensagens do Rust; `fills_named_values` | não verificado | não verificado (clippy Windows só no CI) |
| R5, R6 | `locale()` nas datas; `stamp` em `i18n.svelte.ts` | não verificado | — |
| R7 | `tests/i18n.test.mjs` (896 frases na interface, 256 no Rust) | — | — |
