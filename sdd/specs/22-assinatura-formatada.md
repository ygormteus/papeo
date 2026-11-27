# 22 — Assinatura formatada (HTML)

- **Estado:** entregue
- **Versão:** 0.16.0
- **Relacionadas:** 09, 16, 18

## Problema

A assinatura do leitor de e-mail (spec 16) é só texto: quem usa logo,
cores, negrito ou links na assinatura (o comum em empresas) não consegue
reproduzi-la, e o e-mail sai diferente do que sai pelo webmail.

## Fora do escopo

- Formatar o texto da mensagem (o editor continua de texto simples; só a
  assinatura tem formatação).
- Fontes da web, vídeos, formulários, scripts e qualquer coisa além de
  texto formatado, links, listas, tabelas simples e imagens.
- Baixar para dentro do e-mail imagens que estão em endereços da internet
  (ficam como link: aparecem conforme o programa de quem recebe).

## Requisitos

- **R1.** O editor de assinatura (leitor de e-mail, Configurações →
  Assinaturas e configuração do IMAP) deve oferecer negrito, itálico,
  sublinhado, cor do texto, tamanho (pequeno, normal, grande), link,
  imagem e "limpar formatação", com a assinatura mostrada como vai sair.
- **R2.** Quando a pessoa inserir uma imagem (arquivo PNG, JPG, GIF ou
  WebP), o Papeo deve guardá-la dentro da assinatura, reduzida para no
  máximo 600 px de largura; cada imagem até 200 KB depois de reduzida, a
  assinatura inteira até 600 KB.
- **R3.** Quando a pessoa colar uma assinatura (do Gmail, Outlook, site),
  o Papeo deve manter a formatação permitida e descartar o resto.
- **R4.** O Papeo deve guardar, para cada conta, a assinatura formatada e
  uma versão em texto dela (gerada sozinha), que continua sendo a que
  aparece no editor da mensagem (spec 16 R10).
- **R5.** Quando a mensagem enviada contiver a versão em texto da
  assinatura sem mudanças, o Papeo deve enviá-la em duas versões: texto
  puro e HTML, com o texto da mensagem e a assinatura formatada no lugar
  da de texto; as imagens guardadas vão dentro do e-mail (não como link).
  Se a pessoa apagou ou mudou a assinatura no editor, o e-mail sai só em
  texto, como antes.
- **R6.** O texto citado (linhas com `>`) deve sair no HTML como citação.
- **R7.** Rascunho no servidor (spec 18) e envio com espera (spec 16)
  devem sair iguais ao envio direto.
- **R8.** O Papeo deve limpar a assinatura formatada no backend antes de
  guardar (lista fechada de tags, atributos, estilos e endereços: `https`,
  `http`, `mailto`, `tel`; imagens `data:` só de PNG/JPG/GIF/WebP).
- **R9.** A assinatura formatada deve ir para o backup junto com a conta.

## Design

- **Núcleo (`papeo_core::mail`):** `MailAccount.signature_html` (`String`,
  vazia = só texto; `skip_serializing_if`; arquivo antigo lê vazia; versão
  antiga ignora). `sanitize_signature_html` (ammonia com lista fechada e
  filtro de estilos; confere tamanho e imagens). `html_message(body,
  texto_da_assinatura, html_da_assinatura) -> Option<String>` (texto
  escapado, `>` em `<blockquote>`, a assinatura no lugar).
  `inline_images(html) -> (html_com_cid, imagens)`.
- **App (`mailbox.rs`):** `build_as` com assinatura formatada encontrada:
  `mixed(alternative(texto, related(html, imagens inline)), anexos…)`.
- **Comandos:** `set_mail_signature(id, signature, html?)` (sem `html`,
  a formatada é apagada); `mail_signature(id) -> {text, html}` novo (a
  formatada não vai no `Snapshot`, que é publicado o tempo todo:
  `ServiceView` leva só `signatureRich: bool`). Em `COMMANDS`, capability
  `default`, `api.ts`, `devMock.ts`.
- **Interface:** `SignatureEditor.svelte` (`contenteditable` com barra de
  formatação via `execCommand`; colar passa por um limpador local num
  documento inerte, `DOMParser`; imagem pelo seletor de arquivo, reduzida
  num `canvas`); `SignatureDialog.svelte` usa o editor e manda HTML +
  texto (`innerText`, links como "texto <endereço>"). `MailDialog` troca a
  caixa de texto por um resumo e o botão "Editar assinatura". No editor da
  mensagem, com assinatura formatada, um aviso curto ("A assinatura sai
  com a formatação").
- **Dados persistidos:** `signature_html` em `services.json` (e no
  backup).
- **Segurança:** o HTML só aparece na interface depois de limpo pelo
  backend (ou pelo limpador local ao colar); a CSP da interface já impede
  scripts e atributos `on*`. O HTML vai para servidores de terceiros só
  dentro do e-mail que a própria pessoa envia.
- **Concorrência:** nada novo (tudo no `blocking` do envio).
  `signature_html` é `Arc<str>`: o estado copiado a cada publicação não
  duplica centenas de KB com o lock seguro.
- **Riscos aceitos:** imagens da assinatura com endereço da internet
  (coladas do Gmail, por exemplo) são carregadas ao abrir o editor, como
  acontece com quem recebe o e-mail.

## Tarefas

1. [x] Núcleo: campo, limpeza, HTML da mensagem, imagens inline — testes.
2. [x] App: `build_as` multipart, comandos, `ServiceView.signatureRich`.
3. [x] Interface: editor, diálogo, `MailDialog`, aviso no editor.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
Revisão crítica independente: aprovada, sem alto nem médio; 5 baixos,
4 corrigidos (arrastar texto passa pelo limpador; salvar sem mexer não
reescreve a assinatura antiga; base64 em linhas e com parâmetros; `Arc` no
estado) e 1 aceito (imagens remotas no editor, em "Riscos aceitos").

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R1, R2, R3 | `SignatureEditor.svelte`, `signatureHtml.ts` (`cleanPasted`, `prepareImage`) | não verificado | não verificado |
| R4 | `signatureText`; `set_mail_signature` (sem texto, sem formatada) | não verificado | — |
| R5, R6 | testes `html_message_puts_rich_signature_in_place`, `images_go_inside_the_email`, `rich_signature_goes_as_html_with_inline_images` | não verificado (Gmail/Outlook reais) | não verificado |
| R7 | `build_as` comum ao envio, ao rascunho e à espera | não verificado | — |
| R8 | teste `signature_html_is_cleaned`, `signature_limits` | — | — |
| R9 | `MailAccount.signature_html` serializado (backup); `sanitized` ao carregar | não verificado | — |
