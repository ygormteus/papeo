# 31 — Formatação no editor de e-mail

- **Estado:** implementada
- **Versão:** 0.17.0
- **Relacionadas:** 09 (leitor de e-mail), 16 (desfazer envio), 18
  (rascunhos no servidor), 22 (assinatura formatada)

## Problema

O editor do leitor de e-mail só escreve texto puro: sem negrito, itálico,
tamanho da letra, listas ou links, coisas que qualquer programa de e-mail
oferece.

## Fora do escopo

- Imagens dentro do texto (vão como anexo; a da assinatura continua).
- Colar com a formatação de fora (cola como texto: nada de HTML externo na
  janela do Papeo).
- Fonte (família), tabelas, recuo.

## Requisitos

- **R1.** O editor deve ter barra de formatação: tamanho da letra
  (pequeno, normal, grande, muito grande), negrito, itálico, sublinhado,
  tachado, cor da letra, lista, lista numerada, citação, alinhar à
  esquerda/centralizar, link e tirar formatação. `Cmd/Ctrl+B/I/U` e
  `Cmd/Ctrl+K` (link).
- **R2.** Com formatação, o e-mail vai com texto e HTML; sem formatação,
  só texto, como antes.
- **R3.** O HTML enviado é limpo no backend (lista fechada de tags,
  atributos, estilos e endereços; sem scripts, eventos, imagens ou
  `javascript:`), qualquer que seja o que a interface mandou.
- **R4.** A assinatura aparece no texto como bloco que não se edita (pode
  ser apagado); no envio vai a formatada (spec 22) ou a de texto no lugar
  do bloco.
- **R5.** Colar e soltar texto entram como texto puro.
- **R6.** Rascunho salvo no servidor e e-mail que volta ao editor
  (desfeito, falhou) mantêm a formatação; HTML que entra no editor passa
  antes por limpeza.

## Design

- **Interface:** `MailEditor.svelte` (`contenteditable` com
  `document.execCommand`, sem biblioteca): devolve o texto puro (`innerText`,
  vale para a parte de texto, a assinatura intacta e os rascunhos), o HTML
  com o bloco da assinatura trocado por `SIGNATURE_TOKEN` (U+E000) e se há
  formatação. `cleanHtml` (DOMParser, lista fechada) limpa o HTML que entra.
- **Envio:** `Draft.body_html` (opcional; `serde(default)`).
  `signature::html_body` limpa com `sanitize_body_html` (ammonia, a lista da
  assinatura sem `img`, mais `strike`; `class` só `papeo-signature` em
  `div`) e põe a assinatura no marcador. Teto: o mesmo do texto (2 MB).
- **Rascunho reaberto:** `DraftView.html` / `OpenedDraft.body_html` (parte
  HTML limpa); o `div.papeo-signature` volta a ser o bloco.
- **Peso:** nenhum pacote novo; o HTML só vai quando há formatação.

## Tarefas

1. [x] Limpeza e montagem do HTML no núcleo; envio com `body_html`.
2. [x] Editor com barra, colar como texto, bloco da assinatura.
3. [x] Rascunho e e-mail que volta com a formatação.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R3, R4 | testes `body_html_keeps_formatting_and_drops_the_rest`, `signature_goes_where_the_token_is`, `formatted_body_goes_as_clean_html_with_the_signature` | — | — |
| R1, R2, R5, R6 | não verificado no app (sem navegador automatizado nesta sessão) | não verificado | não verificado |
