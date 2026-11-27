//! Assinatura formatada do leitor de e-mail (spec 22): limpeza do HTML,
//! montagem da versão HTML da mensagem e imagens dentro do e-mail.

use crate::i18n::tr;
use base64::Engine;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Teto da assinatura formatada (com as imagens em base64).
pub const SIGNATURE_HTML_MAX: usize = 600 * 1024;
/// Teto de cada imagem guardada na assinatura (bytes da imagem).
pub const SIGNATURE_IMAGE_MAX: usize = 200 * 1024;

const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

const TAGS: &[&str] = &[
    "a",
    "b",
    "strong",
    "i",
    "em",
    "u",
    "s",
    "br",
    "p",
    "div",
    "span",
    "font",
    "img",
    "ul",
    "ol",
    "li",
    "table",
    "tbody",
    "thead",
    "tr",
    "td",
    "th",
    "hr",
    "small",
    "big",
    "blockquote",
    "h1",
    "h2",
    "h3",
    "h4",
    "center",
    "sub",
    "sup",
];

const ATTRIBUTES: &[&str] = &[
    "style",
    "align",
    "valign",
    "width",
    "height",
    "dir",
    "bgcolor",
    "color",
    "size",
    "face",
    "border",
    "cellpadding",
    "cellspacing",
    "colspan",
    "rowspan",
];

/// Estilos que só mudam a aparência (nada de posição, fundo com imagem ou
/// endereço): o resto é tirado do `style` pelo ammonia.
const STYLES: &[&str] = &[
    "color",
    "background-color",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "text-decoration",
    "text-align",
    "line-height",
    "letter-spacing",
    "vertical-align",
    "white-space",
    "width",
    "height",
    "max-width",
    "min-width",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-color",
    "border-style",
    "border-width",
    "border-radius",
    "border-collapse",
    "display",
];

/// Imagem `data:` aceita: (tipo, bytes). `None` se não for imagem
/// permitida ou se o base64 for inválido.
fn data_image(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.trim().strip_prefix("data:")?;
    let (head, data) = rest.split_once(',')?;
    // `image/png;name=logo.png;base64`: o tipo é o primeiro, base64 o último.
    let head = head.to_ascii_lowercase();
    let mime = head
        .strip_suffix(";base64")?
        .split(';')
        .next()?
        .trim()
        .to_owned();
    if !IMAGE_TYPES.contains(&mime.as_str()) {
        return None;
    }
    // Base64 quebrado em linhas (exportado por programas de e-mail).
    let data: Vec<u8> = data.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .ok()?;
    Some((mime, bytes))
}

/// Limpa a assinatura formatada (lista fechada de tags, atributos, estilos
/// e endereços). Vazia se não sobrar texto nem imagem. Erro se uma imagem
/// ou o total passar do teto.
pub fn sanitize_signature_html(raw: &str) -> Result<String, String> {
    if raw.trim().is_empty() {
        return Ok(String::new());
    }
    let too_big = Arc::new(AtomicBool::new(false));
    let seen = too_big.clone();
    let mut builder = ammonia::Builder::default();
    builder
        .tags(TAGS.iter().copied().collect())
        .tag_attributes(
            [
                ("a", ["href", "title"].into_iter().collect()),
                ("img", ["src", "alt", "title"].into_iter().collect()),
            ]
            .into_iter()
            .collect(),
        )
        .generic_attributes(ATTRIBUTES.iter().copied().collect())
        .filter_style_properties(STYLES.iter().copied().collect::<HashSet<_>>())
        .url_schemes(
            ["http", "https", "mailto", "tel", "data"]
                .into_iter()
                .collect(),
        )
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(None)
        .attribute_filter(move |element, attribute, value| {
            let lower = value.trim_start().to_ascii_lowercase();
            if attribute == "src" || attribute == "href" {
                if lower.starts_with("data:") {
                    if element != "img" || attribute != "src" {
                        return None;
                    }
                    let (_, bytes) = data_image(value)?;
                    if bytes.len() > SIGNATURE_IMAGE_MAX {
                        seen.store(true, Ordering::Relaxed);
                        return None;
                    }
                }
                if attribute == "src"
                    && !(lower.starts_with("data:")
                        || lower.starts_with("https:")
                        || lower.starts_with("http:"))
                {
                    return None;
                }
            }
            Some(value.into())
        });
    let clean = builder.clean(raw).to_string();
    if too_big.load(Ordering::Relaxed) {
        return Err(tr("Uma imagem da assinatura passa de 200 KB. Use uma imagem menor.").into());
    }
    if clean.len() > SIGNATURE_HTML_MAX {
        return Err(tr("A assinatura passa de 600 KB. Use imagens menores.").into());
    }
    let has_text = strip_tags(&clean)
        .trim()
        .chars()
        .any(|c| !c.is_whitespace());
    if !has_text && !clean.contains("<img") {
        return Ok(String::new());
    }
    Ok(clean.trim().to_owned())
}

/// Texto sem as tags (só para saber se sobrou conteúdo).
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Linhas de texto em HTML; as que começam com `>` viram citação.
fn text_html(lines: &[&str], out: &mut String) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with('>') {
            let mut inner = Vec::new();
            while i < lines.len() && lines[i].starts_with('>') {
                let line = &lines[i][1..];
                inner.push(line.strip_prefix(' ').unwrap_or(line));
                i += 1;
            }
            out.push_str(
                "<blockquote style=\"margin:0 0 0 0.8ex;border-left:1px solid #ccc;padding-left:1ex\">",
            );
            text_html(&inner, out);
            out.push_str("</blockquote>");
        } else {
            // Espaços seguidos não somem no HTML.
            out.push_str(&escape(lines[i]).replace("  ", "&nbsp; "));
            out.push_str("<br>\n");
            i += 1;
        }
    }
}

fn block(text: &str, out: &mut String) {
    if text.is_empty() {
        return;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    text_html(&lines, out);
}

/// Versão HTML da mensagem: o texto, com a assinatura em texto trocada pela
/// formatada. `None` se não houver formatada ou se a assinatura em texto
/// não estiver no corpo sem mudanças (a pessoa apagou ou editou).
pub fn html_message(body: &str, signature_text: &str, signature_html: &str) -> Option<String> {
    if signature_text.is_empty() || signature_html.trim().is_empty() {
        return None;
    }
    let body = body.replace("\r\n", "\n");
    let marker = format!("-- \n{}", signature_text.replace("\r\n", "\n"));
    let mut from = 0;
    let at = loop {
        let at = from + body[from..].find(&marker)?;
        let end = at + marker.len();
        let starts_line = at == 0 || body.as_bytes()[at - 1] == b'\n';
        let ends_line = end == body.len() || body.as_bytes()[end] == b'\n';
        if starts_line && ends_line {
            break at;
        }
        from = at + 1;
    };
    let (before, rest) = body.split_at(at);
    let after = rest[marker.len()..]
        .strip_prefix('\n')
        .unwrap_or(&rest[marker.len()..]);
    let mut out =
        String::from("<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body><div>");
    block(before.strip_suffix('\n').unwrap_or(before), &mut out);
    out.push_str("<div class=\"papeo-signature\">--&nbsp;<br>\n");
    out.push_str(signature_html);
    out.push_str("</div>");
    block(after, &mut out);
    out.push_str("</div></body></html>");
    Some(out)
}

/// Marcador do bloco da assinatura no HTML do editor (spec 31): a interface
/// troca o bloco por ele e o envio põe a assinatura no lugar.
pub const SIGNATURE_TOKEN: &str = "\u{E000}papeo-signature\u{E000}";

/// Limpa o HTML do texto do editor (spec 31): a mesma lista fechada da
/// assinatura, sem imagens; `div.papeo-signature` marca a assinatura num
/// rascunho reaberto.
pub fn sanitize_body_html(raw: &str) -> String {
    let mut tags: HashSet<&str> = TAGS.iter().copied().collect();
    tags.remove("img");
    tags.insert("strike");
    let mut builder = ammonia::Builder::default();
    builder
        .tags(tags)
        .tag_attributes(
            [("a", ["href", "title"].into_iter().collect())]
                .into_iter()
                .collect(),
        )
        .generic_attributes(ATTRIBUTES.iter().copied().collect())
        .allowed_classes(
            [("div", ["papeo-signature"].into_iter().collect())]
                .into_iter()
                .collect(),
        )
        .filter_style_properties(STYLES.iter().copied().collect::<HashSet<_>>())
        .url_schemes(["http", "https", "mailto", "tel"].into_iter().collect())
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(None);
    builder.clean(raw).to_string()
}

/// Versão HTML de uma mensagem escrita com formatação (spec 31): o HTML do
/// editor limpo, com a assinatura no lugar do marcador (a formatada se a
/// de texto estiver intacta em `body_text`; senão, a de texto).
pub fn html_body(
    body_text: &str,
    body_html: &str,
    signature_text: &str,
    signature_html: &str,
) -> String {
    let clean = sanitize_body_html(body_html);
    let mut block = String::new();
    if !signature_text.is_empty() {
        block.push_str("<div class=\"papeo-signature\">--&nbsp;<br>\n");
        if html_message(body_text, signature_text, signature_html).is_some() {
            block.push_str(signature_html);
        } else {
            let text = signature_text.replace("\r\n", "\n");
            text_html(&text.split('\n').collect::<Vec<_>>(), &mut block);
        }
        block.push_str("</div>");
    }
    let mut out =
        String::from("<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body><div>");
    let mut parts = clean.splitn(2, SIGNATURE_TOKEN);
    out.push_str(parts.next().unwrap_or_default());
    if let Some(rest) = parts.next() {
        out.push_str(&block);
        out.push_str(&rest.replace(SIGNATURE_TOKEN, ""));
    }
    out.push_str("</div></body></html>");
    out
}

/// Imagem que vai dentro do e-mail (parte `inline` com `Content-ID`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    pub cid: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Troca as imagens `data:` do HTML por `cid:` e devolve as imagens (cada
/// imagem repetida vai uma vez). `tag` deixa o `Content-ID` único.
pub fn inline_images(html: &str, tag: &str) -> (String, Vec<InlineImage>) {
    const NEEDLE: &str = "src=\"data:";
    let mut out = String::with_capacity(html.len());
    let mut images: Vec<InlineImage> = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(NEEDLE) {
        let value_start = at + "src=\"".len();
        let Some(len) = rest[value_start..].find('"') else {
            break;
        };
        let url = &rest[value_start..value_start + len];
        out.push_str(&rest[..at]);
        match data_image(url) {
            Some((mime, bytes)) => {
                let cid = match images.iter().find(|i| i.bytes == bytes) {
                    Some(same) => same.cid.clone(),
                    None => {
                        let cid = format!("assinatura{}.{tag}@papeo", images.len() + 1);
                        images.push(InlineImage {
                            cid: cid.clone(),
                            mime,
                            bytes,
                        });
                        cid
                    }
                };
                out.push_str(&format!("src=\"cid:{cid}\""));
            }
            // Não deveria acontecer (já limpo): a imagem sai do e-mail.
            None => out.push_str("src=\"\""),
        }
        rest = &rest[value_start + len + 1..];
    }
    out.push_str(rest);
    (out, images)
}

#[cfg(test)]
mod body_tests {
    use super::*;

    #[test]
    fn body_html_keeps_formatting_and_drops_the_rest() {
        let raw = r#"<b>oi</b><font size="5" color="red" onclick="x()">grande</font><script>alert(1)</script><img src="https://x/y.png"><a href="javascript:x()">a</a><a href="https://ok.com">b</a><div class="papeo-signature other" style="position:fixed;color:blue">s</div><span class="z">t</span><strike>r</strike>"#;
        let clean = sanitize_body_html(raw);
        assert!(clean.contains("<b>oi</b>"), "{clean}");
        assert!(
            clean.contains(r#"size="5""#) && clean.contains(r#"color="red""#),
            "{clean}"
        );
        assert!(
            !clean.contains("onclick") && !clean.contains("script") && !clean.contains("<img"),
            "{clean}"
        );
        assert!(
            !clean.contains("javascript") && clean.contains("https://ok.com"),
            "{clean}"
        );
        assert!(
            clean.contains(r#"class="papeo-signature""#) && !clean.contains("other"),
            "{clean}"
        );
        assert!(
            !clean.contains("position") && clean.contains("color:blue"),
            "{clean}"
        );
        assert!(
            !clean.contains("class=\"z\"") && clean.contains("<strike>"),
            "{clean}"
        );
    }

    #[test]
    fn signature_goes_where_the_token_is() {
        let body = "Oi\n\n-- \nAna";
        let html =
            format!("<div><b>Oi</b></div>{SIGNATURE_TOKEN}<div>&gt; antes</div>{SIGNATURE_TOKEN}");
        let out = html_body(
            body,
            &html,
            "Ana",
            "<i>Ana</i><img src=\"data:image/png;base64,AA==\">",
        );
        assert!(
            out.contains("<b>Oi</b></div><div class=\"papeo-signature\">--&nbsp;<br>\n<i>Ana</i>"),
            "{out}"
        );
        assert!(!out.contains(SIGNATURE_TOKEN), "{out}");
        assert_eq!(out.matches("papeo-signature").count(), 1, "{out}");
        // Assinatura de texto mexida: vai a de texto.
        let out = html_body("Oi\n\n-- \nAna mudou", &html, "Ana", "<i>Ana</i>");
        assert!(
            out.contains("--&nbsp;<br>\nAna<br>") && !out.contains("<i>Ana</i>"),
            "{out}"
        );
        // Sem marcador (assinatura apagada): sem assinatura.
        let out = html_body("Oi", "<b>Oi</b>", "Ana", "<i>Ana</i>");
        assert!(!out.contains("papeo-signature"), "{out}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    #[test]
    fn signature_html_is_cleaned() {
        let raw = format!(
            r#"<p style="color:#c00;position:fixed;background:url(x)">Ana <b>Souza</b></p>
            <script>alert(1)</script><a href="javascript:alert(1)" onclick="x()">link</a>
            <a href="https://loja.com">Loja</a><a href="data:text/html,x">d</a>
            <img src="data:image/png;base64,{PNG}" onerror="x()">
            <img src="data:image/svg+xml;base64,PHN2Zz4=">
            <img src="https://loja.com/logo.png"><iframe src="https://x"></iframe>
            <span style="font-weight:bold;behavior:url(x)">ok</span>"#
        );
        let clean = sanitize_signature_html(&raw).unwrap();
        assert!(clean.contains("color:#c00"));
        assert!(!clean.contains("position"));
        assert!(!clean.contains("url("));
        assert!(!clean.contains("script") && !clean.contains("alert"));
        assert!(!clean.contains("onclick") && !clean.contains("onerror"));
        assert!(!clean.contains("javascript"));
        assert!(clean.contains(r#"href="https://loja.com""#));
        assert!(!clean.contains("data:text"));
        assert!(clean.contains("data:image/png;base64"));
        assert!(!clean.contains("svg"));
        assert!(clean.contains("https://loja.com/logo.png"));
        assert!(!clean.contains("iframe"));
        assert!(clean.contains("<b>Souza</b>"));
        assert_eq!(
            sanitize_signature_html("  <p> </p><script>x</script>").unwrap(),
            ""
        );
        assert_eq!(sanitize_signature_html("").unwrap(), "");
    }

    #[test]
    fn signature_limits() {
        let big =
            base64::engine::general_purpose::STANDARD.encode(vec![0u8; SIGNATURE_IMAGE_MAX + 1]);
        let raw = format!(r#"<img src="data:image/png;base64,{big}">"#);
        assert!(sanitize_signature_html(&raw).is_err());
        let ok = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 150 * 1024]);
        let raw = format!(r#"<img src="data:image/png;base64,{ok}">"#).repeat(4);
        assert!(
            sanitize_signature_html(&raw).is_err(),
            "total acima de 600 KB"
        );
    }

    #[test]
    fn html_message_puts_rich_signature_in_place() {
        let body = "Olá, <tudo> bem?\n\n-- \nAna Souza\nLoja\n\nEm ontem, Bia escreveu:\n> oi\n> > antes\n";
        let html = html_message(body, "Ana Souza\nLoja", "<b>Ana</b>").unwrap();
        assert!(html.contains("Olá, &lt;tudo&gt; bem?<br>"));
        assert!(html.contains("<div class=\"papeo-signature\">--&nbsp;<br>\n<b>Ana</b></div>"));
        assert!(!html.contains("Ana Souza"));
        assert!(html.contains("<blockquote"));
        assert_eq!(html.matches("<blockquote").count(), 2);
        // Assinatura apagada ou mudada: só texto.
        assert!(html_message("Olá\n\n-- \nAna", "Ana Souza\nLoja", "<b>Ana</b>").is_none());
        assert!(html_message("Olá", "Ana", "<b>Ana</b>").is_none());
        assert!(html_message("Olá\n-- \nAna", "Ana", "").is_none());
        // No meio de uma linha não conta.
        assert!(html_message("x -- \nAna", "Ana", "<b>A</b>").is_none());
        assert!(html_message("-- \nAna\n", "Ana", "<b>A</b>").is_some());
    }

    #[test]
    fn images_go_inside_the_email() {
        let html = format!(
            r#"<img src="data:image/png;base64,{PNG}"><img src="https://x/y.png"><img src="data:image/png;base64,{PNG}">"#
        );
        let (out, images) = inline_images(&html, "abc");
        let wrapped = format!(
            "data:image/png;name=logo.png;base64,{}\n{}",
            &PNG[..20],
            &PNG[20..]
        );
        assert!(
            data_image(&wrapped).is_some(),
            "base64 em linhas e parâmetros"
        );
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].mime, "image/png");
        assert_eq!(images[0].cid, "assinatura1.abc@papeo");
        assert_eq!(out.matches("cid:assinatura1.abc@papeo").count(), 2);
        assert!(out.contains("https://x/y.png"));
        assert!(!out.contains("data:"));
    }
}
