# 06 — Arquivos, anexos e links

- **Estado:** entregue (0.1, 0.5, 0.8, 0.11), retroativa
- **Relacionadas:** 05

## Problema

Baixar e abrir arquivos e links dentro de webviews exige caminhos que o site
não controla, sem abrir brecha para arquivos maliciosos.

## Requisitos

- **R1.** Quando um site baixar um arquivo, o Papeo deve gravá-lo na pasta
  escolhida (ou em Downloads), com nome seguro e sem sobrescrever outro
  arquivo.
- **R2.** O Papeo deve marcar o arquivo baixado como vindo da internet
  (quarentena no macOS, `Zone.Identifier` no Windows).
- **R3.** Onde "Visualizar antes" estiver ligado, quando o arquivo for de um
  tipo visualizável (PDF, imagens, Office; nunca HTML/SVG), o Papeo deve
  abri-lo numa janela própria com Salvar, Abrir no app e Fechar
  (descarta).
- **R4.** "Abrir no app" deve salvar uma cópia nos downloads antes de abrir,
  para edições não ficarem no temporário.
- **R5.** O Papeo deve apagar os temporários de anexos ao fechar a janela e
  ao abrir o app.
- **R6.** Se o app estiver bloqueado, então o Papeo deve mandar o anexo
  direto para os downloads.
- **R7.** Quando a pessoa clicar num link externo dentro de uma conta, o
  Papeo deve abri-lo no navegador ou numa prévia própria (sem dados em
  disco), conforme a opção. Links internos do site continuam no site.
- **R8.** O Papeo deve deixar o site receber arquivos arrastados para ele.

## Design

`downloads.rs` (`safe_file_name`, `unique_path`), `attachment.rs`
(Quick Look no macOS; webview `file://` no Windows; capability
`attachment`), `preview.rs` (janela `preview` com página `incognito`;
capability `preview`), `disable_drag_drop_handler()`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1 | testes de `util.rs` (`safe_file_name`, `unique_path`) | ok | — |
| R3, R5 | Autoteste (anexo salvo, temporário apagado) | ok | ok (CI) |
| R7 | Autoteste (links: âncora, WhatsApp, `_blank`, direto, interno) | ok | ok (CI) |
| R3 (planilha real no Quick Look) | — | não verificado | não verificado |
