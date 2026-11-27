//! E-mail grande aberto aos pedaços (spec 28): a estrutura (`BODYSTRUCTURE`)
//! diz quais partes são o texto e as imagens do corpo; só elas são baixadas.
//! Os anexos entram vazios (só o cabeçalho da parte) e são baixados inteiros
//! quando a pessoa pede um deles.
//!
//! A mensagem é remontada com a mesma árvore de partes (mesma ordem), para
//! `message_view` ler texto, HTML e anexos como leria a mensagem inteira.

use crate::mailbox::Tok;

/// Partes demais: abre inteira (estrutura estranha ou abusiva).
const LEAVES_MAX: usize = 200;
/// Profundidade máxima da árvore de partes.
const DEPTH_MAX: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Leaf {
    /// Número da parte no IMAP (`1`, `2.1`…).
    pub part: String,
    /// `tipo/subtipo` em minúsculas.
    pub mime: String,
    pub cid: Option<String>,
    /// Bytes como estão no servidor (codificados).
    pub size: u32,
    /// Marcada como anexo (`Content-Disposition: attachment`).
    pub attachment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Multi { sub: String, children: Vec<Node> },
    Leaf(Leaf),
}

fn lower(tok: Option<&Tok>) -> String {
    match tok {
        Some(Tok::Atom(a)) => a.to_ascii_lowercase(),
        Some(Tok::Str(b)) => String::from_utf8_lossy(b).to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn number(tok: Option<&Tok>) -> u32 {
    match tok {
        Some(Tok::Atom(a)) => a.parse().unwrap_or(0),
        _ => 0,
    }
}

/// Lê a `BODYSTRUCTURE`. `None`: estrutura que não dá para usar.
pub(crate) fn parse(tok: &Tok) -> Option<Node> {
    let mut leaves = 0;
    let node = node(tok, "", 0, &mut leaves)?;
    (leaves <= LEAVES_MAX).then_some(node)
}

fn node(tok: &Tok, prefix: &str, depth: usize, leaves: &mut usize) -> Option<Node> {
    let Tok::List(items) = tok else {
        return None;
    };
    if depth > DEPTH_MAX {
        return None;
    }
    let child = |i: usize| {
        if prefix.is_empty() {
            (i + 1).to_string()
        } else {
            format!("{prefix}.{}", i + 1)
        }
    };
    if matches!(items.first(), Some(Tok::List(_))) {
        let mut children = Vec::new();
        let mut rest = items.iter();
        for (i, item) in rest
            .by_ref()
            .take_while(|t| matches!(t, Tok::List(_)))
            .enumerate()
        {
            children.push(node(item, &child(i), depth + 1, leaves)?);
        }
        // `take_while` consumiu o primeiro item que não é lista: o subtipo.
        let sub = lower(items.get(children.len()));
        if sub.is_empty() || !sub.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return None;
        }
        return Some(Node::Multi { sub, children });
    }
    *leaves += 1;
    let ctype = lower(items.first());
    let sub = lower(items.get(1));
    let token = |t: &str| {
        !t.is_empty()
            && t.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'+'))
    };
    // Vira cabeçalho se o servidor não mandar o da parte: só caracteres de
    // tipo MIME (nada de quebra de linha).
    let mime = if token(&ctype) && token(&sub) {
        format!("{ctype}/{sub}")
    } else {
        "application/octet-stream".to_owned()
    };
    let cid = match items.get(3) {
        Some(Tok::Str(b)) => {
            let c = String::from_utf8_lossy(b);
            let c = c.trim().trim_start_matches('<').trim_end_matches('>');
            (!c.is_empty()).then(|| c.to_ascii_lowercase())
        }
        _ => None,
    };
    let size = number(items.get(6));
    // Onde fica a disposição depende do tipo (texto tem linhas; mensagem
    // anexada tem envelope, estrutura e linhas).
    let disposition = match (ctype.as_str(), sub.as_str()) {
        ("text", _) => 9,
        ("message", "rfc822" | "global") => 11,
        _ => 8,
    };
    let attachment = match items.get(disposition) {
        Some(Tok::List(d)) => lower(d.first()) == "attachment",
        _ => false,
    };
    let part = if prefix.is_empty() {
        "1".to_owned()
    } else {
        prefix.to_owned()
    };
    Some(Node::Leaf(Leaf {
        part,
        mime,
        cid,
        size,
        attachment,
    }))
}

/// Folhas na ordem da mensagem.
pub(crate) fn leaves(node: &Node) -> Vec<&Leaf> {
    let mut out = Vec::new();
    fn walk<'a>(n: &'a Node, out: &mut Vec<&'a Leaf>) {
        match n {
            Node::Multi { children, .. } => children.iter().for_each(|c| walk(c, out)),
            Node::Leaf(l) => out.push(l),
        }
    }
    walk(node, &mut out);
    out
}

/// O que vale baixar já: o texto do corpo e as imagens embutidas pequenas.
/// `image_max` vale para os bytes decodificados (como na mensagem inteira).
pub(crate) fn wanted(leaf: &Leaf, image_max: u32) -> bool {
    // Imagem citada no HTML (`cid:`) entra mesmo marcada como anexo: a
    // mensagem inteira também a mostraria no corpo.
    if leaf.cid.is_some()
        && leaf.mime.starts_with("image/")
        && !leaf.mime.contains("svg")
        && decoded_size(leaf.size) <= u64::from(image_max)
    {
        return true;
    }
    !leaf.attachment && (leaf.mime == "text/plain" || leaf.mime == "text/html")
}

/// Cabeçalho de parte sem o `Content-ID`: a parte que veio vazia não pode
/// virar imagem embutida vazia; aparece como anexo.
fn without_content_id(head: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(head.len());
    let mut skipping = false;
    for line in head.split_inclusive(|&b| b == b'\n') {
        let continuation = line.first().is_some_and(|b| *b == b' ' || *b == b'\t');
        if !continuation {
            skipping = line.len() >= 11 && line[..11].eq_ignore_ascii_case(b"content-id:");
        }
        if !skipping {
            out.extend_from_slice(line);
        }
    }
    out
}

/// Cabeçalho da mensagem sem o `Content-Type` (e suas continuações).
fn without_content_type(header: &[u8]) -> Vec<u8> {
    let text = header.strip_suffix(b"\r\n\r\n").unwrap_or(header);
    let mut out = Vec::with_capacity(text.len() + 80);
    let mut skipping = false;
    for line in text.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let continuation = line.first().is_some_and(|b| *b == b' ' || *b == b'\t');
        if !continuation {
            skipping = line.len() >= 13 && line[..13].eq_ignore_ascii_case(b"content-type:");
        }
        if skipping || line.is_empty() {
            continue;
        }
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn boundary(salt: &str, part: &str) -> String {
    format!("{salt}-{}", if part.is_empty() { "0" } else { part })
}

/// Números `n` já usados em `papeo-<n hex>` num pedaço (sem diferenciar
/// maiúsculas), numa passada só.
fn taken_salts(piece: &[u8], taken: &mut std::collections::HashSet<u32>) {
    const PREFIX: &[u8] = b"papeo-";
    let mut i = 0;
    while i + PREFIX.len() <= piece.len() {
        if piece[i..i + PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
            let digits = &piece[i + PREFIX.len()..];
            let len = digits
                .iter()
                .take(9)
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            if len > 0 && len <= 8 {
                if let Ok(n) = u32::from_str_radix(&String::from_utf8_lossy(&digits[..len]), 16) {
                    taken.insert(n);
                }
            }
            i += PREFIX.len();
        } else {
            i += 1;
        }
    }
}

/// Prefixo dos separadores (`papeo-<n>`) que não aparece em nenhum pedaço
/// vindo do servidor: um texto do remetente não consegue abrir uma parte
/// falsa. Custo de uma leitura dos pedaços, qualquer que seja o conteúdo.
fn salt(taken: &std::collections::HashSet<u32>) -> String {
    let n = (0u32..).find(|n| !taken.contains(n)).unwrap_or(u32::MAX);
    format!("papeo-{n:x}")
}

/// Remonta a mensagem: cabeçalho, árvore de partes com os separadores do
/// Papeo, cabeçalho de cada folha e o conteúdo das baixadas (`body`). Folha
/// sem conteúdo entra vazia.
pub(crate) fn assemble(
    header: &[u8],
    root: &Node,
    mime: &dyn Fn(&str) -> Option<Vec<u8>>,
    body: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Vec<u8> {
    let mut taken = std::collections::HashSet::new();
    taken_salts(header, &mut taken);
    for leaf in leaves(root) {
        // Um pedaço por vez (sem guardar cópias de todos).
        if let Some(m) = mime(&leaf.part) {
            taken_salts(&m, &mut taken);
        }
        if let Some(b) = body(&leaf.part) {
            taken_salts(&b, &mut taken);
        }
    }
    let salt = salt(&taken);
    let mut out = without_content_type(header);
    write_node(&mut out, root, "", &salt, mime, body);
    out
}

fn write_node(
    out: &mut Vec<u8>,
    node: &Node,
    part: &str,
    salt: &str,
    mime: &dyn Fn(&str) -> Option<Vec<u8>>,
    body: &dyn Fn(&str) -> Option<Vec<u8>>,
) {
    match node {
        Node::Multi { sub, children } => {
            let b = boundary(salt, part);
            out.extend_from_slice(
                format!("Content-Type: multipart/{sub}; boundary=\"{b}\"\r\n\r\n").as_bytes(),
            );
            for (i, child) in children.iter().enumerate() {
                let p = if part.is_empty() {
                    (i + 1).to_string()
                } else {
                    format!("{part}.{}", i + 1)
                };
                out.extend_from_slice(format!("--{b}\r\n").as_bytes());
                write_node(out, child, &p, salt, mime, body);
                out.extend_from_slice(b"\r\n");
            }
            out.extend_from_slice(format!("--{b}--\r\n").as_bytes());
        }
        Node::Leaf(leaf) => {
            // Mensagem de uma parte só não passa por aqui (abre inteira).
            let head = mime(&leaf.part)
                .unwrap_or_else(|| format!("Content-Type: {}\r\n\r\n", leaf.mime).into_bytes());
            let content = body(&leaf.part);
            let head = if content.is_some() {
                head
            } else {
                without_content_id(&head)
            };
            out.extend_from_slice(&head);
            if !head.ends_with(b"\r\n\r\n") && !head.ends_with(b"\n\n") {
                out.extend_from_slice(b"\r\n");
            }
            if let Some(bytes) = content {
                out.extend_from_slice(&bytes);
            }
        }
    }
}

/// Bytes reais de um anexo que veio vazio, aproximados do tamanho
/// codificado (base64 cresce um terço).
pub(crate) fn decoded_size(encoded: u32) -> u64 {
    u64::from(encoded) * 3 / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mailbox::{message_view, tokens_of_line};

    const STRUCTURE: &str = r#"* 1 FETCH (BODYSTRUCTURE ((("TEXT" "PLAIN" ("CHARSET" "utf-8") NIL NIL "QUOTED-PRINTABLE" 12 1 NIL NIL NIL)("TEXT" "HTML" ("CHARSET" "utf-8") NIL NIL "7BIT" 60 2 NIL NIL NIL) "ALTERNATIVE" ("BOUNDARY" "b2") NIL NIL)("IMAGE" "PNG" ("NAME" "logo.png") "<logo@x>" NIL "BASE64" 8 NIL ("INLINE" ("FILENAME" "logo.png")) NIL)("APPLICATION" "PDF" ("NAME" "grande.pdf") NIL NIL "BASE64" 4000000 NIL ("ATTACHMENT" ("FILENAME" "grande.pdf")) NIL) "MIXED" ("BOUNDARY" "b1") NIL NIL))"#;

    fn root() -> Node {
        let toks = tokens_of_line(STRUCTURE);
        let Some(Tok::List(items)) = toks.get(3) else {
            panic!("sem itens");
        };
        parse(&items[1]).expect("estrutura")
    }

    #[test]
    fn reads_structure() {
        let root = root();
        let all = leaves(&root);
        let parts: Vec<_> = all.iter().map(|l| l.part.as_str()).collect();
        assert_eq!(parts, ["1.1", "1.2", "2", "3"]);
        assert_eq!(all[1].mime, "text/html");
        assert_eq!(all[2].cid.as_deref(), Some("logo@x"));
        assert!(!all[2].attachment);
        assert!(all[3].attachment);
        assert_eq!(all[3].size, 4_000_000);
        let fetch: Vec<_> = all
            .iter()
            .filter(|l| wanted(l, 1000))
            .map(|l| l.part.as_str())
            .collect();
        assert_eq!(fetch, ["1.1", "1.2", "2"]);
        // Teto da imagem pelo tamanho decodificado (8 codificados = 6).
        assert!(wanted(all[2], 6));
        assert!(!wanted(all[2], 5));
        let mut cid_attached = all[2].clone();
        cid_attached.attachment = true;
        assert!(wanted(&cid_attached, 1000));
        // Estrutura sem subtipo: não usa.
        assert!(parse(&Tok::List(vec![Tok::List(vec![])])).is_none());
    }

    #[test]
    fn assembles_without_attachments() {
        let root = root();
        let header = b"From: Ana <ana@x.com>\r\nSubject: Oi\r\nContent-Type: multipart/mixed;\r\n boundary=\"b1\"\r\nMIME-Version: 1.0\r\n\r\n";
        let mime = |p: &str| -> Option<Vec<u8>> {
            Some(match p {
                "1.1" => b"Content-Type: text/plain; charset=utf-8\r\n\r\n".to_vec(),
                "1.2" => b"Content-Type: text/html; charset=utf-8\r\n\r\n".to_vec(),
                "2" => b"Content-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: inline; filename=\"logo.png\"\r\n\r\n".to_vec(),
                _ => b"Content-Type: application/pdf; name=\"grande.pdf\"\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: attachment; filename=\"grande.pdf\"\r\n\r\n".to_vec(),
            })
        };
        let body = |p: &str| -> Option<Vec<u8>> {
            match p {
                "1.1" => Some(b"Ol\xc3\xa1 texto".to_vec()),
                "1.2" => Some(b"<p>Ol\xc3\xa1 <img src=\"cid:logo@x\"></p>".to_vec()),
                "2" => Some(b"iVBORw0KGgo=".to_vec()),
                _ => None,
            }
        };
        let raw = assemble(header, &root, &mime, &body);
        let view = message_view(7, &raw);
        assert_eq!(view.subject, "Oi");
        assert!(view.text.contains("Olá texto"), "{}", view.text);
        assert!(view.html.expect("html").contains("Olá"));
        assert_eq!(view.attachments.len(), 1);
        assert_eq!(view.attachments[0].name, "grande.pdf");
        assert_eq!(view.attachments[0].size, 0);
        // O anexo tem o mesmo índice que teria na mensagem inteira.
        let full_body = |p: &str| body(p).or_else(|| Some(b"JVBERi0=".to_vec()));
        let full = message_view(7, &assemble(header, &root, &mime, &full_body));
        assert_eq!(full.attachments.len(), 1);
        assert_eq!(full.attachments[0].index, view.attachments[0].index);
        assert!(full.attachments[0].size > 0);
        assert!(!String::from_utf8_lossy(&raw).contains("boundary=\"b1\""));
    }

    #[test]
    fn salt_skips_used_numbers_in_one_pass() {
        let mut taken = std::collections::HashSet::new();
        taken_salts(b"x PAPEO-0-1 papeo-1 papeo-2a papeo- papeo-zz", &mut taken);
        assert!(taken.contains(&0) && taken.contains(&1) && taken.contains(&0x2a));
        assert_eq!(salt(&taken), "papeo-2");
        // Muitos números listados: ainda uma passada só.
        let long: String = (0..50_000).map(|n| format!("papeo-{n:x}\n")).collect();
        let mut taken = std::collections::HashSet::new();
        taken_salts(long.as_bytes(), &mut taken);
        assert_eq!(salt(&taken), format!("papeo-{:x}", 50_000));
    }

    #[test]
    fn sender_cannot_forge_a_part() {
        let root = root();
        let header =
            b"From: a@x.com\r\nSubject: s\r\nContent-Type: multipart/mixed; boundary=b1\r\n\r\n";
        let forged = b"<p>oi</p>\r\n--papeo-0-0\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"falso.pdf\"\r\n\r\nx\r\n".to_vec();
        let mime = |p: &str| -> Option<Vec<u8>> {
            Some(if p == "3" {
                b"Content-Type: application/pdf\r\nContent-Disposition: attachment; filename=real.pdf\r\n\r\n".to_vec()
            } else {
                b"Content-Type: text/html; charset=utf-8\r\n\r\n".to_vec()
            })
        };
        let body = |p: &str| -> Option<Vec<u8>> { (p == "1.2").then(|| forged.clone()) };
        let raw = assemble(header, &root, &mime, &body);
        let view = message_view(1, &raw);
        let names: Vec<_> = view.attachments.iter().map(|a| a.name.as_str()).collect();
        assert!(!names.contains(&"falso.pdf"), "{names:?}");
        assert!(names.contains(&"real.pdf"), "{names:?}");
    }

    #[test]
    fn empty_parts_keep_attachment_indices() {
        // Mensagem anexada, texto anexado, convite e imagem grande citada no
        // HTML: vazios, os índices batem com os da mensagem inteira.
        let line = r#"* 1 FETCH (BODYSTRUCTURE (("TEXT" "HTML" ("CHARSET" "utf-8") NIL NIL "7BIT" 30 1 NIL NIL NIL)("MESSAGE" "RFC822" NIL NIL NIL "7BIT" 900 ("x" "y" NIL NIL NIL NIL NIL NIL NIL NIL) ("TEXT" "PLAIN" NIL NIL NIL "7BIT" 2 1 NIL NIL NIL) 20 NIL ("ATTACHMENT" ("FILENAME" "fw.eml")) NIL)("TEXT" "PLAIN" ("NAME" "notas.txt") NIL NIL "7BIT" 700 9 NIL ("ATTACHMENT" ("FILENAME" "notas.txt")) NIL)("TEXT" "CALENDAR" ("METHOD" "REQUEST") NIL NIL "7BIT" 800 9 NIL NIL NIL)("IMAGE" "PNG" NIL "<big@x>" NIL "BASE64" 9000000 NIL ("INLINE" NIL) NIL) "MIXED" NIL NIL NIL))"#;
        let toks = tokens_of_line(line);
        let Some(Tok::List(items)) = toks.get(3) else {
            panic!("sem itens");
        };
        let root = parse(&items[1]).expect("estrutura");
        let all = leaves(&root);
        assert_eq!(all.len(), 5);
        assert!(all[1].attachment && all[2].attachment && !all[3].attachment);
        let wanted_parts: Vec<&str> = all
            .iter()
            .filter(|l| wanted(l, 3 * 1024 * 1024))
            .map(|l| l.part.as_str())
            .collect();
        assert_eq!(wanted_parts, ["1"]);
        let header =
            b"From: a@x.com\r\nSubject: s\r\nContent-Type: multipart/mixed; boundary=z\r\n\r\n";
        let mime = |p: &str| -> Option<Vec<u8>> {
            Some(match p {
                "1" => b"Content-Type: text/html; charset=utf-8\r\n\r\n".to_vec(),
                "2" => b"Content-Type: message/rfc822\r\nContent-Disposition: attachment; filename=fw.eml\r\n\r\n".to_vec(),
                "3" => b"Content-Type: text/plain; name=notas.txt\r\nContent-Disposition: attachment; filename=notas.txt\r\n\r\n".to_vec(),
                "4" => b"Content-Type: text/calendar; method=REQUEST\r\n\r\n".to_vec(),
                _ => b"Content-Type: image/png\r\nContent-ID: <big@x>\r\nContent-Transfer-Encoding: base64\r\nContent-Disposition: inline\r\n\r\n".to_vec(),
            })
        };
        let partial_body = |p: &str| -> Option<Vec<u8>> {
            (p == "1").then(|| b"<p>oi <img src=\"cid:big@x\"></p>".to_vec())
        };
        let full_body = |p: &str| -> Option<Vec<u8>> {
            Some(match p {
                "1" => b"<p>oi <img src=\"cid:big@x\"></p>".to_vec(),
                "2" => b"From: b@x.com\r\nSubject: fw\r\n\r\nok\r\n".to_vec(),
                "3" => b"linha\r\n".to_vec(),
                "4" => b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".to_vec(),
                _ => b"iVBORw0KGgo=".to_vec(),
            })
        };
        let partial = message_view(1, &assemble(header, &root, &mime, &partial_body));
        let full = message_view(1, &assemble(header, &root, &mime, &full_body));
        let listed = |v: &crate::mailbox::MessageView| {
            v.attachments
                .iter()
                .filter(|a| a.mime != "image/png")
                .map(|a| (a.index, a.name.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(listed(&partial), listed(&full));
        // A imagem que veio vazia não vira `data:` vazio no HTML.
        let html = partial.html.unwrap_or_default();
        assert!(!html.contains("data:image/png;base64,\""), "{html}");
    }
}
