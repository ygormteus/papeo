//! Leitor de e-mail por IMAP: pastas, lista de mensagens, leitura, busca,
//! marcações, mover/apagar e guardar enviados. A conexão e o login ficam em
//! [`crate::mail`]; aqui só os comandos de leitura e a montagem do que a tela
//! mostra (HTML limpo, anexos, imagens embutidas).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mail_parser::{AddressList, MessageParser};
use serde::{Deserialize, Serialize};

use crate::i18n::{tr, trf};
use crate::mail::{clip, push_string, Imap, ImapError, ImapResult, Part, Response, Transport};

/// Mensagens por página da lista.
pub const PAGE_SIZE: u32 = 50;
/// Maior e-mail que o leitor baixa inteiro.
pub const MESSAGE_MAX: usize = 40 * 1024 * 1024;
/// Prévia de texto na lista (caracteres).
const PREVIEW_CHARS: usize = 160;
/// Imagem embutida (`cid:`) que vira `data:` no HTML.
const INLINE_IMAGE_MAX: usize = 3 * 1024 * 1024;
const INLINE_TOTAL_MAX: usize = 12 * 1024 * 1024;
/// Termo de busca.
pub const QUERY_MAX: usize = 200;
/// E-mail maior que isto abre aos pedaços, sem baixar os anexos (spec 28).
pub const PARTIAL_FROM: u32 = 512 * 1024;

// ---------------------------------------------------------------------------
// Leitura de respostas IMAP

/// Pedaço de uma resposta: átomo, texto (entre aspas ou literal), NIL ou
/// lista entre parênteses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    Atom(String),
    Str(Vec<u8>),
    Nil,
    List(Vec<Tok>),
}

impl Tok {
    fn text(&self) -> Option<Cow<'_, str>> {
        match self {
            Tok::Atom(a) => Some(Cow::Borrowed(a)),
            Tok::Str(bytes) => Some(String::from_utf8_lossy(bytes)),
            _ => None,
        }
    }

    fn bytes(&self) -> &[u8] {
        match self {
            Tok::Str(bytes) => bytes,
            Tok::Atom(a) => a.as_bytes(),
            _ => &[],
        }
    }

    fn number(&self) -> Option<u32> {
        match self {
            Tok::Atom(a) => a.parse().ok(),
            _ => None,
        }
    }
}

/// Lê a linha de uma resposta (com os literais no lugar dos `{n}`).
pub(crate) fn tokens(response: &Response) -> Vec<Tok> {
    let mut parser = Tokens {
        depth: 0,
        line: response.line.as_bytes(),
        pos: 0,
        literals: response.literals.iter(),
    };
    parser.list(None)
}

struct Tokens<'a> {
    line: &'a [u8],
    pos: usize,
    literals: std::slice::Iter<'a, Vec<u8>>,
    /// Listas abertas agora: aninhamento absurdo (estrutura de e-mail
    /// hostil) para a leitura em vez de estourar a pilha.
    depth: usize,
}

/// Listas dentro de listas numa resposta.
const TOKENS_DEPTH_MAX: usize = 64;

impl Tokens<'_> {
    fn list(&mut self, close: Option<u8>) -> Vec<Tok> {
        let mut out = Vec::new();
        while self.pos < self.line.len() {
            let c = self.line[self.pos];
            match c {
                b' ' => self.pos += 1,
                b')' | b']' if Some(c) == close => {
                    self.pos += 1;
                    return out;
                }
                b'(' => {
                    self.pos += 1;
                    if self.depth >= TOKENS_DEPTH_MAX {
                        self.pos = self.line.len();
                        return out;
                    }
                    self.depth += 1;
                    out.push(Tok::List(self.list(Some(b')'))));
                    self.depth -= 1;
                }
                b'"' => out.push(self.quoted()),
                b'{' => match self.literal() {
                    Some(tok) => out.push(tok),
                    None => out.push(self.atom()),
                },
                _ => {
                    let tok = self.atom();
                    if tok == Tok::Atom(String::new()) {
                        // Caractere solto (`)` sem par): pula.
                        self.pos += 1;
                    } else {
                        out.push(tok);
                    }
                }
            }
        }
        out
    }

    fn quoted(&mut self) -> Tok {
        self.pos += 1;
        let mut out = Vec::new();
        while self.pos < self.line.len() {
            let c = self.line[self.pos];
            self.pos += 1;
            match c {
                b'\\' if self.pos < self.line.len() => {
                    out.push(self.line[self.pos]);
                    self.pos += 1;
                }
                b'"' => break,
                _ => out.push(c),
            }
        }
        Tok::Str(out)
    }

    /// `{n}` ou `{n+}`: o próximo literal guardado.
    fn literal(&mut self) -> Option<Tok> {
        let rest = &self.line[self.pos..];
        let end = rest.iter().position(|&b| b == b'}')?;
        let inner = &rest[1..end];
        let inner = inner.strip_suffix(b"+").unwrap_or(inner);
        if inner.is_empty() || !inner.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.pos += end + 1;
        Some(Tok::Str(self.literals.next().cloned().unwrap_or_default()))
    }

    /// Átomo; `[...]` entra inteiro (ex.: `BODY[HEADER.FIELDS (FROM)]<0>`).
    fn atom(&mut self) -> Tok {
        let start = self.pos;
        let mut depth = 0usize;
        while self.pos < self.line.len() {
            let c = self.line[self.pos];
            match c {
                b'[' => depth += 1,
                b']' if depth > 0 => depth -= 1,
                b' ' | b'(' | b')' | b'"' | b'{' if depth == 0 => break,
                _ => {}
            }
            self.pos += 1;
        }
        let text = String::from_utf8_lossy(&self.line[start..self.pos]).into_owned();
        if text.eq_ignore_ascii_case("NIL") {
            Tok::Nil
        } else {
            Tok::Atom(text)
        }
    }
}

#[cfg(test)]
pub(crate) fn tokens_of_line(line: &str) -> Vec<Tok> {
    tokens(&Response {
        line: line.to_owned(),
        literals: Vec::new(),
    })
}

/// Resposta sem tag do tipo `* <word> ...` (ex.: `LIST`, `SEARCH`): o resto.
fn untagged<'a>(toks: &'a [Tok], word: &str) -> Option<&'a [Tok]> {
    match toks {
        [Tok::Atom(star), Tok::Atom(w), rest @ ..]
            if star == "*" && w.eq_ignore_ascii_case(word) =>
        {
            Some(rest)
        }
        _ => None,
    }
}

/// `* <n> FETCH (...)`: os pares chave/valor.
fn fetch_items(toks: &[Tok]) -> Option<HashMap<String, &Tok>> {
    let [Tok::Atom(star), Tok::Atom(_), Tok::Atom(word), Tok::List(items)] = toks else {
        return None;
    };
    if star != "*" || !word.eq_ignore_ascii_case("FETCH") {
        return None;
    }
    let mut map = HashMap::new();
    let mut iter = items.iter();
    while let (Some(Tok::Atom(key)), Some(value)) = (iter.next(), iter.next()) {
        map.insert(fetch_key(key), value);
    }
    Some(map)
}

/// Normaliza a chave de um item de FETCH: `BODY[HEADER.FIELDS (...)]` vira
/// `BODY[HEADER`, `BODY[TEXT]<0>` vira `BODY[TEXT]`.
fn fetch_key(key: &str) -> String {
    let upper = key.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("BODY[") {
        if rest.starts_with("HEADER") {
            return "BODY[HEADER".into();
        }
        let end = rest.find(']').unwrap_or(rest.len());
        return format!("BODY[{}]", &rest[..end]);
    }
    upper
}

/// Valor de um campo do cabeçalho principal (linhas de continuação
/// juntadas), sem decodificar.
fn header_field(raw: &[u8], name: &str) -> Option<String> {
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .or_else(|| raw.windows(2).position(|w| w == b"\n\n"))
        .unwrap_or(raw.len().min(256 * 1024));
    let header = String::from_utf8_lossy(&raw[..end]);
    let mut found: Option<String> = None;
    for line in header.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with([' ', '\t']) {
            if let Some(v) = found.as_mut() {
                v.push(' ');
                v.push_str(line.trim());
            }
            continue;
        }
        if found.is_some() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case(name) {
                found = Some(value.trim().to_owned());
            }
        }
    }
    found
}

/// Melhor destino do `List-Unsubscribe`: o primeiro `https:` (ou `http:`),
/// senão o primeiro `mailto:`. Endereços longos ou com espaços são
/// recusados.
fn unsubscribe_target(value: &str) -> Option<String> {
    let items: Vec<&str> = value
        .split('<')
        .skip(1)
        .filter_map(|p| p.split_once('>').map(|(u, _)| u.trim()))
        .filter(|u| u.len() <= 2000 && !u.chars().any(|c| c.is_whitespace() || c.is_control()))
        .collect();
    let lower = |u: &str| u.to_ascii_lowercase();
    items
        .iter()
        .find(|u| lower(u).starts_with("https://") || lower(u).starts_with("http://"))
        .or_else(|| items.iter().find(|u| lower(u).starts_with("mailto:")))
        .map(|u| (*u).to_owned())
}

/// UIDs novos de uma cópia ou movimentação (`[COPYUID v origem destino]`),
/// na ordem dos de origem. Vazio se o servidor não informa.
fn copied_uids(lines: &[&str]) -> Vec<(u32, u32)> {
    fn expand(set: &str) -> Option<Vec<u32>> {
        let mut out = Vec::new();
        for piece in set.split(',') {
            match piece.split_once(':') {
                Some((a, b)) => {
                    let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
                    let (lo, hi) = (a.min(b), a.max(b));
                    if hi - lo > 10_000 || out.len() > 10_000 {
                        return None;
                    }
                    out.extend(lo..=hi);
                }
                None => out.push(piece.parse().ok()?),
            }
        }
        Some(out)
    }
    let mut pairs = Vec::new();
    for line in lines {
        let upper = line.to_ascii_uppercase();
        let Some(start) = upper.find("[COPYUID ") else {
            continue;
        };
        let rest = &line[start + 9..];
        let rest = rest.split(']').next().unwrap_or("");
        let mut words = rest.split_whitespace();
        let (Some(_), Some(from), Some(to)) = (words.next(), words.next(), words.next()) else {
            continue;
        };
        if let (Some(from), Some(to)) = (expand(from), expand(to)) {
            if from.len() == to.len() {
                pairs.extend(from.into_iter().zip(to));
            }
        }
    }
    pairs
}

/// Filtro rápido da lista (spec 28).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Filter {
    #[default]
    All,
    Unread,
    Flagged,
    Attachments,
}

/// Mensagem aberta: a vista e, se veio inteira, os bytes (para os anexos).
pub struct Opened {
    pub view: MessageView,
    pub raw: Option<Vec<u8>>,
}

/// Números de sequência (de, até) das até `PAGE_SIZE` mensagens depois das
/// `skip` mais novas de uma pasta com `exists`; `None` se já passou do fim.
fn seq_range(exists: u32, skip: u32) -> Option<(u32, u32)> {
    if skip >= exists {
        return None;
    }
    let high = exists - skip;
    Some((high.saturating_sub(PAGE_SIZE - 1).max(1), high))
}

// ---------------------------------------------------------------------------
// Pastas

/// Papel de uma pasta especial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Inbox,
    Drafts,
    Sent,
    Archive,
    All,
    Flagged,
    Junk,
    Trash,
}

impl Role {
    fn order(role: Option<Role>) -> u8 {
        match role {
            Some(Role::Inbox) => 0,
            Some(Role::Flagged) => 1,
            Some(Role::Drafts) => 2,
            Some(Role::Sent) => 3,
            Some(Role::Archive) => 4,
            Some(Role::All) => 5,
            Some(Role::Junk) => 6,
            Some(Role::Trash) => 7,
            None => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    /// Nome no servidor (mUTF-7), usado nos comandos.
    pub path: String,
    /// Nome para mostrar (último pedaço, decodificado).
    pub name: String,
    /// Nível na árvore (0 = raiz).
    pub depth: u32,
    pub role: Option<Role>,
}

/// Papel pela marcação do servidor (RFC 6154) ou, na falta, pelo nome.
fn folder_role(path: &str, flags: &[String]) -> Option<(Role, bool)> {
    if path.eq_ignore_ascii_case("INBOX") {
        return Some((Role::Inbox, true));
    }
    for flag in flags {
        let role = match flag.to_ascii_lowercase().as_str() {
            "\\sent" => Role::Sent,
            "\\trash" => Role::Trash,
            "\\junk" => Role::Junk,
            "\\drafts" => Role::Drafts,
            "\\archive" => Role::Archive,
            "\\all" => Role::All,
            "\\flagged" => Role::Flagged,
            _ => continue,
        };
        return Some((role, true));
    }
    let name = decode_mutf7(path).to_lowercase();
    let last = name.rsplit(['/', '.']).next().unwrap_or(&name).trim();
    let guess = match last {
        "sent" | "sent items" | "sent mail" | "sent messages" | "enviados" | "enviadas"
        | "itens enviados" | "mensagens enviadas" => Some(Role::Sent),
        "trash" | "deleted" | "deleted items" | "deleted messages" | "lixeira"
        | "itens excluídos" | "itens excluidos" => Some(Role::Trash),
        "spam" | "junk" | "junk e-mail" | "junk email" | "lixo eletrônico" | "lixo eletronico" => {
            Some(Role::Junk)
        }
        "drafts" | "rascunhos" => Some(Role::Drafts),
        "archive" | "archives" | "arquivo" | "arquivos" | "arquivados" => Some(Role::Archive),
        _ => None,
    };
    guess.map(|role| (role, false))
}

/// Lê as respostas de `LIST "" "*"`.
pub(crate) fn parse_list(responses: &[Response]) -> Vec<Folder> {
    let mut folders = Vec::new();
    let mut taken: Vec<Role> = Vec::new();
    let mut candidates: Vec<Option<(Role, bool)>> = Vec::new();
    for response in responses {
        let toks = tokens(response);
        let Some([Tok::List(flags), delimiter, name, ..]) = untagged(&toks, "LIST") else {
            continue;
        };
        let flags: Vec<String> = flags
            .iter()
            .filter_map(|f| f.text().map(Cow::into_owned))
            .collect();
        if flags.iter().any(|f| {
            f.eq_ignore_ascii_case("\\Noselect") || f.eq_ignore_ascii_case("\\NonExistent")
        }) {
            continue;
        }
        let Some(path) = name.text().map(Cow::into_owned) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let delimiter = delimiter.text().and_then(|d| d.chars().next());
        let decoded = decode_mutf7(&path);
        let (depth, last) = match delimiter {
            Some(d) => {
                let pieces: Vec<&str> = decoded.split(d).collect();
                (
                    pieces.len() as u32 - 1,
                    pieces.last().copied().unwrap_or(""),
                )
            }
            None => (0, decoded.as_str()),
        };
        let candidate = folder_role(&path, &flags);
        let name = last.to_owned();
        folders.push(Folder {
            path,
            name,
            depth,
            role: None,
        });
        candidates.push(candidate);
    }
    // Um papel por conta: marcados pelo servidor primeiro (na ordem da
    // lista), depois os palpites pelo nome para os papéis que sobraram.
    for by_flag in [true, false] {
        for (folder, candidate) in folders.iter_mut().zip(&candidates) {
            let Some((role, flagged)) = *candidate else {
                continue;
            };
            if flagged == by_flag && folder.role.is_none() && !taken.contains(&role) {
                taken.push(role);
                folder.role = Some(role);
            }
        }
    }
    for folder in &mut folders {
        if folder.role == Some(Role::Inbox) {
            folder.name = "Caixa de Entrada".to_owned();
        }
    }
    // Pastas especiais primeiro; o resto em ordem do servidor.
    folders.sort_by_key(|f| (Role::order(f.role), f.role.is_none()));
    folders
}

/// Decodifica um nome de pasta em UTF-7 modificado (RFC 3501, 5.1.3).
pub fn decode_mutf7(raw: &str) -> String {
    use base64::Engine;
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let encoded = &after[..end];
        if encoded.is_empty() {
            out.push('&');
        } else {
            let b64 = encoded.replace(',', "/");
            let engine = base64::engine::general_purpose::STANDARD_NO_PAD;
            match engine.decode(b64.trim_end_matches('=')) {
                Ok(bytes) if bytes.len() % 2 == 0 => {
                    let units: Vec<u16> = bytes
                        .chunks(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect();
                    out.push_str(&String::from_utf16_lossy(&units));
                }
                _ => {
                    out.push('&');
                    out.push_str(encoded);
                    out.push('-');
                }
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Lista de mensagens

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Addr {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Summary {
    pub uid: u32,
    pub from: Addr,
    pub to: Vec<Addr>,
    pub subject: String,
    /// Segundos desde 1970 (UTC).
    pub date: i64,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub size: u32,
    pub preview: String,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Page {
    pub messages: Vec<Summary>,
    /// Total na pasta (ou encontrados na busca).
    pub total: u32,
}

fn addresses(list: Option<&AddressList<'_>>) -> Vec<Addr> {
    let Some(list) = list else {
        return Vec::new();
    };
    list.mailboxes()
        .filter_map(|m| {
            let email = m.address().unwrap_or("").trim().to_owned();
            let name = m.name().unwrap_or("").trim().to_owned();
            (!email.is_empty() || !name.is_empty()).then_some(Addr { name, email })
        })
        .take(100)
        .collect()
}

fn has_flag(flags: Option<&&Tok>, flag: &str) -> bool {
    matches!(flags, Some(Tok::List(list)) if list.iter().any(|f| f.text().is_some_and(|t| t.eq_ignore_ascii_case(flag))))
}

/// Monta o resumo de uma resposta de FETCH.
fn summary(items: &HashMap<String, &Tok>) -> Option<Summary> {
    let uid = items.get("UID")?.number()?;
    // Aviso solto do servidor (`* n FETCH (UID x FLAGS ...)`, marcação
    // mudada por outro programa) não é resumo.
    let header = items.get("BODY[HEADER")?.bytes();
    // Marcada para apagar (expurgo pendente): fica fora da lista.
    if has_flag(items.get("FLAGS"), "\\Deleted") {
        return None;
    }
    let text = items.get("BODY[TEXT]").map(|t| t.bytes()).unwrap_or(&[]);
    let mut raw = Vec::with_capacity(header.len() + text.len() + 2);
    raw.extend_from_slice(header);
    if !raw.ends_with(b"\r\n\r\n") && !raw.ends_with(b"\n\n") {
        raw.extend_from_slice(b"\r\n");
    }
    raw.extend_from_slice(text);
    let message = MessageParser::default().parse(&raw);
    let flags = items.get("FLAGS");
    let mut out = Summary {
        uid,
        seen: has_flag(flags, "\\Seen"),
        flagged: has_flag(flags, "\\Flagged"),
        answered: has_flag(flags, "\\Answered"),
        size: items
            .get("RFC822.SIZE")
            .and_then(|t| t.number())
            .unwrap_or(0),
        ..Default::default()
    };
    if let Some(message) = &message {
        out.from = addresses(message.from().as_ref())
            .into_iter()
            .next()
            .unwrap_or_default();
        out.to = addresses(message.to().as_ref());
        out.subject = crate::clip(message.subject().unwrap_or("").trim(), 300);
        out.date = message.date().map(|d| d.to_timestamp()).unwrap_or(0);
        out.preview = message
            .body_preview(PREVIEW_CHARS * 2)
            .map(|p| one_line(&p, PREVIEW_CHARS))
            .unwrap_or_default();
        out.has_attachments = message.content_type().is_some_and(|ct| {
            ct.ctype().eq_ignore_ascii_case("multipart")
                && ct
                    .subtype()
                    .is_some_and(|s| s.eq_ignore_ascii_case("mixed"))
        });
    }
    if out.date == 0 {
        out.date = items
            .get("INTERNALDATE")
            .and_then(|t| t.text())
            .and_then(|t| parse_internal_date(&t))
            .unwrap_or(0);
    }
    Some(out)
}

/// Texto numa linha só, sem espaços repetidos, até `max` caracteres.
fn one_line(text: &str, max: usize) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::clip(&joined, max)
}

/// `INTERNALDATE` ("17-Jul-1996 02:44:25 -0700") em segundos UTC.
fn parse_internal_date(text: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let mut words = text.split_whitespace();
    let date = words.next()?;
    let time = words.next()?;
    let zone = words.next().unwrap_or("+0000");
    let mut d = date.split('-');
    let day: i64 = d.next()?.parse().ok()?;
    let month_name = d.next()?.to_ascii_lowercase();
    let month = MONTHS.iter().position(|m| *m == month_name)? as i64 + 1;
    let year: i64 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let (h, m, s): (i64, i64, i64) = (
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    );
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let digits = zone.trim_start_matches(['+', '-']);
    let zh: i64 = digits.get(..2)?.parse().ok()?;
    let zm: i64 = digits.get(2..4)?.parse().ok()?;
    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + h * 3600 + m * 60 + s - sign * (zh * 3600 + zm * 60))
}

/// Dias desde 1970-01-01 (algoritmo de Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// ---------------------------------------------------------------------------
// Mensagem aberta

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// Posição entre os anexos (para baixar depois).
    pub index: u32,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub uid: u32,
    pub subject: String,
    pub from: Vec<Addr>,
    pub to: Vec<Addr>,
    pub cc: Vec<Addr>,
    pub reply_to: Vec<Addr>,
    pub date: i64,
    pub message_id: String,
    pub references: Vec<String>,
    /// HTML já limpo (sem scripts, formulários nem endereços relativos).
    pub html: Option<String>,
    /// O HTML pede imagens ou estilos de fora (ficam bloqueados até o
    /// usuário pedir).
    pub has_remote: bool,
    pub text: String,
    pub attachments: Vec<Attachment>,
    pub seen: bool,
    pub flagged: bool,
    /// Endereço para sair da lista (`List-Unsubscribe`): `https:` ou
    /// `mailto:` (spec 28).
    pub unsubscribe: Option<String>,
}

/// Monta a mensagem para a tela a partir do e-mail inteiro.
pub fn message_view(uid: u32, raw: &[u8]) -> MessageView {
    let mut view = MessageView {
        uid,
        ..Default::default()
    };
    let Some(message) = MessageParser::default().parse(raw) else {
        view.text = String::from_utf8_lossy(raw).into_owned();
        return view;
    };
    view.subject = crate::clip(message.subject().unwrap_or("").trim(), 1000);
    view.from = addresses(message.from().as_ref());
    view.to = addresses(message.to().as_ref());
    view.cc = addresses(message.cc().as_ref());
    view.reply_to = addresses(message.reply_to().as_ref());
    view.date = message.date().map(|d| d.to_timestamp()).unwrap_or(0);
    view.message_id = message.message_id().unwrap_or("").trim().to_owned();
    let mut references: Vec<String> = message
        .references()
        .map(|list| list.iter().map(|r| r.trim().to_owned()).collect())
        .unwrap_or_default();
    if references.is_empty() {
        if let Some(list) = message.in_reply_to() {
            references.extend(list.iter().map(|r| r.trim().to_owned()));
        }
    }
    references.retain(|r| !r.is_empty() && r.len() < 1000);
    references.truncate(50);
    view.references = references;
    view.text = message
        .body_text(0)
        .map(Cow::into_owned)
        .unwrap_or_default();
    view.unsubscribe = header_field(raw, "List-Unsubscribe").and_then(|v| unsubscribe_target(&v));

    // Imagens embutidas: `cid:` vira `data:` (só imagem, com teto).
    let mut inline: HashMap<String, String> = HashMap::new();
    let mut total = 0usize;
    for (index, part) in message.attachments().enumerate() {
        let mime = part
            .content_type()
            .map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.ctype(), sub),
                None => ct.ctype().to_owned(),
            })
            .unwrap_or_else(|| "application/octet-stream".into())
            .to_ascii_lowercase();
        let cid = part.content_id().map(|c| {
            c.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_owned()
        });
        let body = part.decoded();
        let size = body.len();
        if let Some(cid) = cid.filter(|c| !c.is_empty()) {
            if mime.starts_with("image/")
                && !mime.contains("svg")
                && size <= INLINE_IMAGE_MAX
                && total + size <= INLINE_TOTAL_MAX
            {
                use base64::Engine;
                total += size;
                let data = base64::engine::general_purpose::STANDARD.encode(&body);
                inline.insert(
                    cid.to_ascii_lowercase(),
                    format!("data:{mime};base64,{data}"),
                );
                if part.is_inline() && part.attachment_name().is_none() {
                    continue;
                }
            }
        }
        view.attachments.push(Attachment {
            index: index as u32,
            name: part
                .attachment_name()
                .map(|n| crate::clip(n.trim(), 200))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "anexo".to_owned()),
            mime,
            size: size as u64,
        });
    }
    if let Some(html) = message.body_html(0) {
        // Mensagem só com texto: o mail-parser monta um HTML; a tela usa o
        // texto puro nesse caso.
        if message
            .html_body()
            .next()
            .is_some_and(|p| p.is_content_type("text", "html"))
        {
            let (clean, remote) = sanitize_html(&html, inline);
            view.html = Some(clean);
            view.has_remote = remote;
        }
    }
    view
}

/// Rascunho aberto para continuar no editor (spec 18).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftView {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text: String,
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    /// (nome, tipo, bytes).
    pub attachments: Vec<(String, String, Vec<u8>)>,
}

/// Cabeçalhos do Papeo com endereços ainda incompletos de um rascunho
/// (valor: base64 das linhas, para não depender de codificação de
/// cabeçalho).
pub const UNPARSED_TO: &str = "X-Papeo-Unparsed-To";
pub const UNPARSED_CC: &str = "X-Papeo-Unparsed-Cc";
pub const UNPARSED_BCC: &str = "X-Papeo-Unparsed-Bcc";

pub fn encode_unparsed(list: &[String]) -> String {
    use base64::Engine;
    let text: String = list
        .iter()
        .map(|a| a.replace(['\r', '\n'], " "))
        .collect::<Vec<_>>()
        .join("\n");
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    // Pedaços de 76 com espaço entre eles: a linha pode ser dobrada dentro
    // do limite de 998 caracteres do cabeçalho.
    encoded
        .as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lê um cabeçalho do Papeo do bloco de cabeçalhos bruto (com dobras).
fn unparsed(raw: &[u8], name: &str) -> Vec<String> {
    use base64::Engine;
    let text = String::from_utf8_lossy(raw);
    let head = text.split("\r\n\r\n").next().unwrap_or("");
    let head = head.split("\n\n").next().unwrap_or("");
    let mut value: Option<String> = None;
    for line in head.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(v) = value.as_mut() {
            if line.starts_with([' ', '\t']) {
                v.push_str(line.trim());
                continue;
            }
            break;
        }
        if let Some((key, rest)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case(name) {
                value = Some(rest.trim().to_owned());
            }
        }
    }
    let Some(bytes) = value.and_then(|v| {
        let compact: String = v.split_whitespace().collect();
        base64::engine::general_purpose::STANDARD
            .decode(compact)
            .ok()
    }) else {
        return Vec::new();
    };
    String::from_utf8_lossy(&bytes)
        .split('\n')
        .map(|a| crate::clip(a.trim(), 300))
        .filter(|a| !a.is_empty())
        .take(100)
        .collect()
}

/// Teto da soma dos anexos de um rascunho aberto (igual ao do envio).
pub const DRAFT_ATTACHMENTS_MAX: usize = 25 * 1024 * 1024;

/// Endereço como o editor escreve: `Nome <email>` (nome entre aspas se
/// tiver caracteres especiais).
fn editable(a: &Addr) -> String {
    if a.name.is_empty() {
        return a.email.clone();
    }
    let name = if a.name.contains([',', ';', '<', '>', '@', '"']) {
        format!("\"{}\"", a.name.replace(['"', '\\'], ""))
    } else {
        a.name.clone()
    };
    if a.email.is_empty() {
        name
    } else {
        format!("{name} <{}>", a.email)
    }
}

/// Lê um rascunho inteiro para o editor. `None` se os anexos passam do teto.
pub fn draft_view(raw: &[u8]) -> Option<DraftView> {
    let Some(message) = MessageParser::default().parse(raw) else {
        return Some(DraftView {
            text: String::from_utf8_lossy(raw).into_owned(),
            ..Default::default()
        });
    };
    let list = |l: Option<&AddressList<'_>>| addresses(l).iter().map(editable).collect();
    let ids = |v: Option<Vec<String>>| -> Vec<String> {
        v.unwrap_or_default()
            .into_iter()
            .map(|r| r.trim().to_owned())
            .filter(|r| !r.is_empty() && r.len() < 1000)
            .take(50)
            .collect()
    };
    let mut view = DraftView {
        to: list(message.to().as_ref()),
        cc: list(message.cc().as_ref()),
        bcc: list(message.bcc().as_ref()),
        subject: crate::clip(message.subject().unwrap_or("").trim(), 1000),
        text: message
            .body_text(0)
            .map(Cow::into_owned)
            .unwrap_or_default(),
        message_id: message.message_id().unwrap_or("").trim().to_owned(),
        in_reply_to: ids(message
            .in_reply_to()
            .map(|l| l.iter().map(|r| r.to_string()).collect()))
        .into_iter()
        .next(),
        references: ids(message
            .references()
            .map(|l| l.iter().map(|r| r.to_string()).collect())),
        attachments: Vec::new(),
    };
    view.to.extend(unparsed(raw, UNPARSED_TO));
    view.cc.extend(unparsed(raw, UNPARSED_CC));
    view.bcc.extend(unparsed(raw, UNPARSED_BCC));
    let mut total = 0usize;
    for part in message.attachments() {
        let mime = part
            .content_type()
            .map(|ct| match ct.subtype() {
                Some(sub) => format!("{}/{}", ct.ctype(), sub),
                None => ct.ctype().to_owned(),
            })
            .unwrap_or_else(|| "application/octet-stream".into())
            .to_ascii_lowercase();
        let body = part.decoded().into_owned();
        total += body.len();
        if total > DRAFT_ATTACHMENTS_MAX {
            return None;
        }
        let name = part
            .attachment_name()
            .map(|n| crate::clip(n.trim(), 200))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "anexo".to_owned());
        view.attachments.push((name, mime, body));
    }
    Some(view)
}

/// Bytes e nome do anexo `index` (posição em [`MessageView::attachments`]).
pub fn attachment(raw: &[u8], index: u32) -> Option<(String, Vec<u8>)> {
    let message = MessageParser::default().parse(raw)?;
    let part = message.attachments().nth(index as usize)?;
    let name = part
        .attachment_name()
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "anexo".to_owned());
    Some((name, part.decoded().into_owned()))
}

/// Limpa o HTML de um e-mail para mostrar num quadro isolado: tira scripts,
/// formulários, eventos e endereços relativos; troca `cid:` pela imagem
/// embutida. Diz se sobrou algo de fora (imagem, fonte ou estilo remoto).
pub fn sanitize_html(html: &str, inline: HashMap<String, String>) -> (String, bool) {
    let remote = Arc::new(AtomicBool::new(false));
    let seen_remote = remote.clone();
    let mut builder = ammonia::Builder::default();
    builder
        .rm_clean_content_tags(["style"])
        .add_tags([
            "style", "font", "center", "big", "small", "u", "s", "strike", "tt", "caption", "col",
            "colgroup",
        ])
        .add_clean_content_tags(["title"])
        .add_generic_attributes([
            "style",
            "class",
            "align",
            "valign",
            "bgcolor",
            "color",
            "width",
            "height",
            "border",
            "cellpadding",
            "cellspacing",
            "dir",
            "face",
            "size",
            "id",
            "nowrap",
        ])
        .add_url_schemes(["cid", "data"])
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(move |element, attribute, value| {
            let lower = value.trim_start().to_ascii_lowercase();
            if attribute == "src" || attribute == "href" {
                if lower.starts_with("cid:") {
                    if element != "img" {
                        return None;
                    }
                    let key = value.trim_start()[4..].trim().to_ascii_lowercase();
                    return inline.get(&key).map(|d| Cow::Owned(d.clone()));
                }
                if lower.starts_with("data:") {
                    let ok = element == "img"
                        && attribute == "src"
                        && lower.starts_with("data:image/")
                        && !lower.starts_with("data:image/svg");
                    return ok.then_some(Cow::Borrowed(value));
                }
                if element != "a"
                    && (lower.starts_with("http:")
                        || lower.starts_with("https:")
                        || lower.starts_with("//"))
                {
                    seen_remote.store(true, Ordering::Relaxed);
                }
            }
            if attribute == "style" && lower.contains("url(") {
                seen_remote.store(true, Ordering::Relaxed);
            }
            Some(Cow::Borrowed(value))
        });
    let clean = builder.clean(html).to_string();
    let lower = clean.to_ascii_lowercase();
    // `<style>` passa inteiro: `url(...)` ou `@import` contam como remoto.
    let styled_remote =
        lower.contains("<style") && (lower.contains("url(") || lower.contains("@import"));
    (clean, remote.load(Ordering::Relaxed) || styled_remote)
}

// ---------------------------------------------------------------------------
// Sessão de leitura

/// Pasta aberta (SELECT).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Selected {
    path: String,
    exists: u32,
}

/// Conexão IMAP já autenticada, usada pelo leitor.
pub struct Reader<S: Transport> {
    imap: Imap<S>,
    caps: Vec<String>,
    gmail: bool,
    selected: Option<Selected>,
    folders: Option<Vec<Folder>>,
}

impl<S: Transport> Reader<S> {
    /// Recebe uma sessão autenticada. `gmail` liga a busca do Gmail.
    pub fn new(mut imap: Imap<S>, gmail: bool) -> ImapResult<Self> {
        imap.allow_large(MESSAGE_MAX);
        let caps = imap.capabilities()?;
        Ok(Self {
            imap,
            caps,
            gmail,
            selected: None,
            folders: None,
        })
    }

    fn has_cap(&self, cap: &str) -> bool {
        self.caps.iter().any(|c| c == cap)
    }

    pub fn noop(&mut self) -> ImapResult<()> {
        self.imap.noop()
    }

    pub fn logout(&mut self) {
        self.imap.logout();
    }

    /// Pastas da conta (guardadas até `refresh`).
    pub fn folders(&mut self, refresh: bool) -> ImapResult<Vec<Folder>> {
        if refresh || self.folders.is_none() {
            let responses = self
                .imap
                .command(vec![Part::Text("LIST \"\" \"*\"".into())])?;
            self.folders = Some(parse_list(&responses));
        }
        Ok(self.folders.clone().unwrap_or_default())
    }

    /// Caminho da pasta com o papel `role`, se houver.
    pub fn role_path(&mut self, role: Role) -> ImapResult<Option<String>> {
        Ok(self
            .folders(false)?
            .into_iter()
            .find(|f| f.role == Some(role))
            .map(|f| f.path))
    }

    /// Abre a pasta (sempre de novo: o total pode ter mudado).
    fn select(&mut self, path: &str) -> ImapResult<u32> {
        check_path(path)?;
        let mut parts = vec![Part::Text("SELECT".into())];
        push_string(&mut parts, path);
        self.selected = None;
        let responses = self.imap.command(parts)?;
        let exists = responses
            .iter()
            .rev()
            .find_map(|r| {
                let mut words = r.line.split_whitespace();
                let (Some("*"), Some(n), Some(w)) = (words.next(), words.next(), words.next())
                else {
                    return None;
                };
                w.eq_ignore_ascii_case("EXISTS")
                    .then(|| n.parse::<u32>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        self.selected = Some(Selected {
            path: path.to_owned(),
            exists,
        });
        Ok(exists)
    }

    /// Garante a pasta aberta sem reabrir à toa.
    fn ensure(&mut self, path: &str) -> ImapResult<()> {
        if self.selected.as_ref().is_some_and(|s| s.path == path) {
            return Ok(());
        }
        self.select(path).map(|_| ())
    }

    /// Página `page` (0 = mais novas) da pasta, ou da busca por `query`.
    pub fn list(&mut self, path: &str, page: u32, query: &str) -> ImapResult<Page> {
        self.list_from(path, page.saturating_mul(PAGE_SIZE), query, Filter::All)
    }

    /// Até `PAGE_SIZE` mensagens depois das `skip` mais novas (rolagem
    /// infinita: `skip` é quantas já estão na tela).
    pub fn list_from(
        &mut self,
        path: &str,
        skip: u32,
        query: &str,
        filter: Filter,
    ) -> ImapResult<Page> {
        let exists = self.select(path)?;
        let query = query.trim();
        if query.is_empty() && filter == Filter::All {
            if exists == 0 {
                return Ok(Page::default());
            }
            let Some((low, high)) = seq_range(exists, skip) else {
                return Ok(Page {
                    messages: Vec::new(),
                    total: exists,
                });
            };
            let mut messages = self.fetch_summaries(&format!("{low}:{high}"), false)?;
            messages.sort_by_key(|m| std::cmp::Reverse(m.uid));
            return Ok(Page {
                messages,
                total: exists,
            });
        }
        let mut uids = self.search(query, filter)?;
        uids.sort_unstable_by(|a, b| b.cmp(a));
        let total = uids.len() as u32;
        let start = (skip as usize).min(uids.len());
        let chunk: Vec<u32> = uids
            .into_iter()
            .skip(start)
            .take(PAGE_SIZE as usize)
            .collect();
        if chunk.is_empty() {
            return Ok(Page {
                messages: Vec::new(),
                total,
            });
        }
        let set = chunk
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut messages = self.fetch_summaries(&set, true)?;
        messages.sort_by_key(|m| std::cmp::Reverse(m.uid));
        Ok(Page { messages, total })
    }

    fn fetch_summaries(&mut self, set: &str, by_uid: bool) -> ImapResult<Vec<Summary>> {
        let command = if by_uid { "UID FETCH" } else { "FETCH" };
        let responses = self.imap.command(vec![Part::Text(format!(
            "{command} {set} (UID FLAGS INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER.FIELDS (FROM TO SUBJECT DATE CONTENT-TYPE CONTENT-TRANSFER-ENCODING)] BODY.PEEK[TEXT]<0.2048>)"
        ))])?;
        let mut out: Vec<Summary> = responses
            .iter()
            .filter_map(|r| {
                let toks = tokens(r);
                summary(&fetch_items(&toks)?)
            })
            .collect();
        out.sort_by_key(|s| s.uid);
        out.dedup_by_key(|s| s.uid);
        Ok(out)
    }

    /// UIDs que batem com `query` e o filtro na pasta aberta.
    fn search(&mut self, query: &str, filter: Filter) -> ImapResult<Vec<u32>> {
        let query = crate::clip(query, QUERY_MAX);
        let gmail = self.gmail && self.has_cap("X-GM-EXT-1");
        // Gmail: a busca do próprio Gmail entende "tem anexo".
        let raw = match (gmail, filter, query.is_empty()) {
            (true, Filter::Attachments, true) => "has:attachment".to_owned(),
            (true, Filter::Attachments, false) => format!("has:attachment {query}"),
            _ => query.to_string(),
        };
        let utf8 = !raw.is_ascii();
        let mut parts = vec![Part::Text(
            if utf8 {
                "UID SEARCH CHARSET UTF-8"
            } else {
                "UID SEARCH"
            }
            .into(),
        )];
        match filter {
            Filter::All => {}
            Filter::Unread => parts.push(Part::Text("UNSEEN".into())),
            Filter::Flagged => parts.push(Part::Text("FLAGGED".into())),
            // Sem o Gmail: e-mail com anexo quase sempre é multipart/mixed.
            Filter::Attachments if !gmail => {
                parts.push(Part::Text("HEADER Content-Type \"multipart/mixed\"".into()))
            }
            Filter::Attachments => {}
        }
        if !raw.is_empty() {
            parts.push(Part::Text(if gmail { "X-GM-RAW" } else { "TEXT" }.into()));
            push_string(&mut parts, &raw);
        } else if filter == Filter::All {
            parts.push(Part::Text("ALL".into()));
        }
        self.search_criteria(parts)
    }

    /// Roda um `UID SEARCH` pronto e lê os UIDs.
    fn search_criteria(&mut self, parts: Vec<Part>) -> ImapResult<Vec<u32>> {
        let responses = self.imap.command(parts)?;
        let mut uids: Vec<u32> = responses
            .iter()
            .filter_map(|r| {
                let toks = tokens(r);
                untagged(&toks, "SEARCH")
                    .map(|rest| rest.iter().filter_map(Tok::number).collect::<Vec<_>>())
            })
            .flatten()
            .collect();
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    /// Abre a mensagem para ler. Grande (`size_hint` acima de
    /// `PARTIAL_FROM`, ou desconhecido e grande no servidor) e com partes:
    /// baixa só o texto e as imagens do corpo (spec 28).
    pub fn open_message(
        &mut self,
        path: &str,
        uid: u32,
        mark_seen: bool,
        size_hint: Option<u32>,
    ) -> ImapResult<Opened> {
        if size_hint.is_none_or(|s| s > PARTIAL_FROM) {
            if let Some(opened) = self.open_partial(path, uid, mark_seen)? {
                return Ok(opened);
            }
        }
        let (raw, seen, flagged) = self.fetch_raw(path, uid, mark_seen)?;
        let mut view = message_view(uid, &raw);
        view.seen = seen || mark_seen;
        view.flagged = flagged;
        Ok(Opened {
            view,
            raw: Some(raw),
        })
    }

    /// `None`: pequena ou de uma parte só (abre inteira).
    fn open_partial(
        &mut self,
        path: &str,
        uid: u32,
        mark_seen: bool,
    ) -> ImapResult<Option<Opened>> {
        use crate::mail_parts::{assemble, decoded_size, leaves, parse, wanted, Node};
        self.ensure(path)?;
        let responses = self.imap.command(vec![Part::Text(format!(
            "UID FETCH {uid} (UID RFC822.SIZE BODYSTRUCTURE)"
        ))])?;
        let mut size = 0;
        let mut root = None;
        for r in &responses {
            let toks = tokens(r);
            let Some(items) = fetch_items(&toks) else {
                continue;
            };
            if items.get("UID").and_then(|t| t.number()) != Some(uid) {
                continue;
            }
            size = items
                .get("RFC822.SIZE")
                .and_then(|t| t.number())
                .unwrap_or(0);
            root = items.get("BODYSTRUCTURE").and_then(|t| parse(t));
        }
        let root = match root {
            Some(root @ Node::Multi { .. }) if size > PARTIAL_FROM => root,
            _ => return Ok(None),
        };
        let all = leaves(&root);
        let mut images = 0usize;
        let fetch: Vec<&str> = all
            .iter()
            .filter(|l| wanted(l, INLINE_IMAGE_MAX as u32))
            .filter(|l| {
                if !l.mime.starts_with("image/") {
                    return true;
                }
                images += l.size as usize;
                images <= INLINE_TOTAL_MAX
            })
            .map(|l| l.part.as_str())
            .collect();
        let header = if mark_seen {
            "BODY[HEADER]"
        } else {
            "BODY.PEEK[HEADER]"
        };
        let mut items = format!("UID FLAGS {header}");
        for l in &all {
            items.push_str(&format!(" BODY.PEEK[{}.MIME]", l.part));
        }
        for p in &fetch {
            items.push_str(&format!(" BODY.PEEK[{p}]"));
        }
        let responses = self
            .imap
            .command(vec![Part::Text(format!("UID FETCH {uid} ({items})"))])?;
        let mut got: HashMap<String, Vec<u8>> = HashMap::new();
        let mut flags: Option<Tok> = None;
        for r in &responses {
            let toks = tokens(r);
            let Some(map) = fetch_items(&toks) else {
                continue;
            };
            if map.get("UID").and_then(|t| t.number()) != Some(uid) {
                continue;
            }
            for (key, value) in map {
                if key == "FLAGS" {
                    flags = Some(value.clone());
                } else if key.starts_with("BODY[") {
                    got.insert(key, value.bytes().to_vec());
                }
            }
        }
        let Some(head) = got.get("BODY[HEADER").cloned() else {
            return Err(ImapError::Rejected(tr("mensagem não encontrada").into()));
        };
        // Pedaço pedido que não veio (ou veio vazio): abre inteira, em vez
        // de mostrar um e-mail em branco.
        let missing = all.iter().any(|l| {
            l.size > 0
                && fetch.contains(&l.part.as_str())
                && got
                    .get(&format!("BODY[{}]", l.part))
                    .is_none_or(|b| b.is_empty())
        });
        if missing {
            return Ok(None);
        }
        let mime = |p: &str| got.get(&format!("BODY[{p}.MIME]")).cloned();
        let body = |p: &str| {
            fetch
                .contains(&p)
                .then(|| got.get(&format!("BODY[{p}]")).cloned())
                .flatten()
        };
        let raw = assemble(&head, &root, &mime, &body);
        let mut view = message_view(uid, &raw);
        // Anexos que vieram vazios: o tamanho pela estrutura.
        let mut empty = all
            .iter()
            .filter(|l| !fetch.contains(&l.part.as_str()))
            .map(|l| decoded_size(l.size));
        for a in view.attachments.iter_mut().filter(|a| a.size == 0) {
            if let Some(size) = empty.next() {
                a.size = size;
            }
        }
        let flags = flags.as_ref();
        view.seen = has_flag(flags.as_ref(), "\\Seen") || mark_seen;
        view.flagged = has_flag(flags.as_ref(), "\\Flagged");
        Ok(Some(Opened { view, raw: None }))
    }

    /// E-mail inteiro (sem marcar como lido) e as marcações dele.
    /// Com `mark_seen`, o próprio servidor marca como lida (sem outro
    /// comando: uma ida e volta a menos).
    pub fn fetch_raw(
        &mut self,
        path: &str,
        uid: u32,
        mark_seen: bool,
    ) -> ImapResult<(Vec<u8>, bool, bool)> {
        self.ensure(path)?;
        let body = if mark_seen { "BODY[]" } else { "BODY.PEEK[]" };
        let responses = self.imap.command(vec![Part::Text(format!(
            "UID FETCH {uid} (UID FLAGS {body})"
        ))])?;
        for r in &responses {
            let toks = tokens(r);
            let Some(items) = fetch_items(&toks) else {
                continue;
            };
            if items.get("UID").and_then(|t| t.number()) != Some(uid) {
                continue;
            }
            let Some(body) = items.get("BODY[]") else {
                continue;
            };
            let flags = items.get("FLAGS");
            return Ok((
                body.bytes().to_vec(),
                has_flag(flags, "\\Seen"),
                has_flag(flags, "\\Flagged"),
            ));
        }
        Err(ImapError::Rejected(tr("mensagem não encontrada").into()))
    }

    /// Liga ou desliga marcações (`\Seen`, `\Flagged`, `\Answered`).
    pub fn set_flags(
        &mut self,
        path: &str,
        uids: &[u32],
        flags: &[&str],
        on: bool,
    ) -> ImapResult<()> {
        if uids.is_empty() || flags.is_empty() {
            return Ok(());
        }
        for flag in flags {
            if !matches!(*flag, "\\Seen" | "\\Flagged" | "\\Answered" | "\\Deleted") {
                return Err(ImapError::Protocol(trf(
                    "marcação inválida: {marca}",
                    &[("marca", &clip(flag))],
                )));
            }
        }
        self.ensure(path)?;
        let sign = if on { '+' } else { '-' };
        self.imap.command(vec![Part::Text(format!(
            "UID STORE {} {sign}FLAGS.SILENT ({})",
            uid_set(uids),
            flags.join(" ")
        ))])?;
        Ok(())
    }

    /// Move mensagens para outra pasta.
    /// Move e devolve os UIDs novos no destino (na ordem de `uids`), quando
    /// o servidor informa (UIDPLUS); senão, vazio.
    pub fn move_to(&mut self, path: &str, uids: &[u32], target: &str) -> ImapResult<Vec<u32>> {
        if uids.is_empty() || path == target {
            return Ok(Vec::new());
        }
        check_path(target)?;
        self.ensure(path)?;
        let set = uid_set(uids);
        let verb = if self.has_cap("MOVE") { "MOVE" } else { "COPY" };
        let mut parts = vec![Part::Text(format!("UID {verb} {set}"))];
        push_string(&mut parts, target);
        let responses = self.imap.command(parts)?;
        let mut lines: Vec<&str> = responses.iter().map(|r| r.line.as_str()).collect();
        let done = self.imap.last_ok.clone();
        lines.push(&done);
        let pairs = copied_uids(&lines);
        if verb == "COPY" {
            self.expunge(&set)?;
        }
        Ok(uids
            .iter()
            .filter_map(|u| pairs.iter().find(|(from, _)| from == u).map(|(_, to)| *to))
            .collect())
    }

    /// Marca todas as mensagens da pasta como lidas (um comando só).
    pub fn mark_all_seen(&mut self, path: &str) -> ImapResult<()> {
        if self.select(path)? == 0 {
            return Ok(());
        }
        self.imap
            .command(vec![Part::Text("STORE 1:* +FLAGS.SILENT (\\Seen)".into())])?;
        Ok(())
    }

    /// Apaga de vez (marca `\Deleted` e expurga).
    fn expunge(&mut self, set: &str) -> ImapResult<()> {
        self.imap.command(vec![Part::Text(format!(
            "UID STORE {set} +FLAGS.SILENT (\\Deleted)"
        ))])?;
        if self.has_cap("UIDPLUS") {
            self.imap
                .command(vec![Part::Text(format!("UID EXPUNGE {set}"))])?;
            return Ok(());
        }
        // Sem UIDPLUS, EXPUNGE apaga tudo o que estiver marcado na pasta:
        // só expurga se não houver outras marcadas (por outro programa).
        // Senão, ficam marcadas (e fora da lista) até outro expurgo.
        let mine: Vec<u32> = set.split(',').filter_map(|u| u.parse().ok()).collect();
        let marked = self.search_criteria(vec![Part::Text("UID SEARCH DELETED".into())])?;
        if marked.iter().all(|u| mine.contains(u)) {
            self.imap.command(vec![Part::Text("EXPUNGE".into())])?;
        }
        Ok(())
    }

    /// Manda para a Lixeira; já na Lixeira (ou sem Lixeira), apaga de vez.
    /// Leva à Lixeira (devolve os UIDs novos lá, para desfazer) ou, sem
    /// ela ou já nela, apaga de vez (vazio).
    pub fn delete(&mut self, path: &str, uids: &[u32]) -> ImapResult<Vec<u32>> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        match self.role_path(Role::Trash)? {
            Some(trash) if trash != path => self.move_to(path, uids, &trash),
            _ => {
                self.ensure(path)?;
                self.expunge(&uid_set(uids))?;
                Ok(Vec::new())
            }
        }
    }

    /// UIDs da pasta com este `Message-ID` (rascunhos do Papeo).
    fn with_message_id(&mut self, path: &str, message_id: &str) -> ImapResult<Vec<u32>> {
        if !searchable_id(message_id) {
            return Err(ImapError::Protocol(
                tr("identificador do rascunho inválido").into(),
            ));
        }
        self.ensure(path)?;
        let mut parts = vec![Part::Text("UID SEARCH HEADER Message-ID".into())];
        push_string(&mut parts, message_id);
        self.search_criteria(parts)
    }

    /// Grava o rascunho e só então apaga as versões anteriores dele (mesmo
    /// `Message-ID`).
    pub fn save_draft(&mut self, path: &str, message: &[u8], message_id: &str) -> ImapResult<()> {
        check_path(path)?;
        let old = self.with_message_id(path, message_id)?;
        let mut parts = vec![Part::Text("APPEND".into())];
        push_string(&mut parts, path);
        parts.push(Part::Text("(\\Seen \\Draft)".into()));
        parts.push(Part::Literal(message.to_vec()));
        self.imap.command(parts)?;
        if !old.is_empty() {
            self.ensure(path)?;
            self.expunge(&uid_set(&old))?;
        }
        Ok(())
    }

    /// Apaga de vez todas as versões de um rascunho do Papeo.
    pub fn discard_drafts(&mut self, path: &str, message_id: &str) -> ImapResult<()> {
        check_path(path)?;
        let old = self.with_message_id(path, message_id)?;
        if old.is_empty() {
            return Ok(());
        }
        self.expunge(&uid_set(&old))
    }

    /// Apaga de vez mensagens (rascunho de outro programa substituído).
    pub fn discard(&mut self, path: &str, uids: &[u32]) -> ImapResult<()> {
        if uids.is_empty() {
            return Ok(());
        }
        check_path(path)?;
        self.ensure(path)?;
        self.expunge(&uid_set(uids))
    }

    /// Apaga de vez o rascunho de outro programa substituído pelo editor,
    /// só se o UID ainda for dele: mesmo `Message-ID` (ou, sem um, marcado
    /// como rascunho). Senão, não mexe.
    pub fn discard_checked(&mut self, path: &str, uid: u32, message_id: &str) -> ImapResult<()> {
        check_path(path)?;
        self.ensure(path)?;
        let mut parts = vec![Part::Text(format!("UID SEARCH UID {uid}"))];
        if searchable_id(message_id) {
            parts.push(Part::Text("HEADER Message-ID".into()));
            push_string(&mut parts, message_id);
        } else {
            parts.push(Part::Text("DRAFT".into()));
        }
        if !self.search_criteria(parts)?.contains(&uid) {
            return Ok(());
        }
        self.expunge(&uid.to_string())
    }

    /// Guarda uma mensagem pronta numa pasta (ex.: Enviados), já lida.
    pub fn append(&mut self, path: &str, message: &[u8]) -> ImapResult<()> {
        check_path(path)?;
        let mut parts = vec![Part::Text("APPEND".into())];
        push_string(&mut parts, path);
        parts.push(Part::Text("(\\Seen)".into()));
        parts.push(Part::Literal(message.to_vec()));
        self.imap.command(parts)?;
        Ok(())
    }
}

/// Identificador que pode ir numa busca de cabeçalho (sem aspas, espaços
/// nem controles).
fn searchable_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 200
        && id
            .bytes()
            .all(|b| (0x21..0x7f).contains(&b) && b != b'"' && b != b'\\')
}

fn uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// Nome de pasta vindo da tela: sem quebras de linha nem controles.
fn check_path(path: &str) -> ImapResult<()> {
    if path.is_empty() || path.len() > 1000 || path.chars().any(char::is_control) {
        return Err(ImapError::Protocol(tr("pasta inválida").into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn deep_nesting_does_not_overflow() {
        let line = format!("* 1 FETCH (BODYSTRUCTURE {})", "(".repeat(200_000));
        let toks = tokens_of_line(&line);
        assert!(!toks.is_empty());
    }

    #[test]
    fn reads_copied_uids() {
        let lines = [
            "* OK [COPYUID 38505 304,319:320 3956:3958] Done",
            "A3 OK done",
        ];
        assert_eq!(
            copied_uids(&lines),
            vec![(304, 3956), (319, 3957), (320, 3958)]
        );
        assert!(copied_uids(&["A1 OK [COPYUID 1 1:3 5] errado"]).is_empty());
        assert!(copied_uids(&["A1 OK sem código"]).is_empty());
    }

    #[test]
    fn finds_unsubscribe() {
        let raw = b"From: a@x.com\r\nList-Unsubscribe: <mailto:sair@x.com?subject=sair>,\r\n <https://x.com/sair?u=1>\r\nSubject: oi\r\n\r\ncorpo\r\nList-Unsubscribe: <https://falso>\r\n";
        let value = header_field(raw, "list-unsubscribe").unwrap();
        assert_eq!(
            unsubscribe_target(&value).as_deref(),
            Some("https://x.com/sair?u=1")
        );
        assert_eq!(
            unsubscribe_target("<mailto:sair@x.com>").as_deref(),
            Some("mailto:sair@x.com")
        );
        assert_eq!(unsubscribe_target("<javascript:alert(1)>"), None);
        assert_eq!(unsubscribe_target("<https://x.com/a b>"), None);
        assert!(header_field(raw, "X-Nada").is_none());
        let view = message_view(1, raw);
        assert_eq!(view.unsubscribe.as_deref(), Some("https://x.com/sair?u=1"));
    }

    #[test]
    fn lists_from_offset() {
        assert_eq!(seq_range(120, 0), Some((71, 120)));
        // Uma excluída na tela: a próxima leva começa onde a anterior parou.
        assert_eq!(seq_range(119, 49), Some((21, 70)));
        assert_eq!(seq_range(120, 100), Some((1, 20)));
        assert_eq!(seq_range(120, 120), None);
        assert_eq!(seq_range(0, 0), None);
        assert_eq!(seq_range(3, 0), Some((1, 3)));
    }

    fn response(line: &str, literals: &[&[u8]]) -> Response {
        Response {
            line: line.to_owned(),
            literals: literals.iter().map(|l| l.to_vec()).collect(),
        }
    }

    #[test]
    fn tokens_read_lists_strings_and_literals() {
        let r = response(
            "* 3 FETCH (UID 9 FLAGS (\\Seen \\Flagged) BODY[HEADER.FIELDS (FROM SUBJECT)] {5} BODY[TEXT]<0> NIL INTERNALDATE \"1-Jan-2024 10:00:00 +0000\")",
            &[b"hello"],
        );
        let toks = tokens(&r);
        let items = fetch_items(&toks).unwrap();
        assert_eq!(items["UID"].number(), Some(9));
        assert_eq!(items["BODY[HEADER"].bytes(), b"hello");
        assert_eq!(*items["BODY[TEXT]"], Tok::Nil);
        assert!(has_flag(items.get("FLAGS"), "\\seen"));
        assert_eq!(
            parse_internal_date(&items["INTERNALDATE"].text().unwrap()),
            Some(1_704_103_200)
        );
        let q = response("* LIST () \"/\" \"a \\\"b\\\\\"", &[]);
        assert_eq!(tokens(&q)[4], Tok::Str(b"a \"b\\".to_vec()));
    }

    #[test]
    fn internal_dates_with_zone() {
        assert_eq!(
            parse_internal_date("17-Jul-1996 02:44:25 -0700"),
            Some(837_596_665)
        );
        assert_eq!(
            parse_internal_date(" 1-Mar-2000 00:00:00 +0000"),
            Some(951_868_800)
        );
        assert_eq!(parse_internal_date("lixo"), None);
    }

    #[test]
    fn modified_utf7_names() {
        assert_eq!(decode_mutf7("Itens Exclu&AO0-dos"), "Itens Excluídos");
        assert_eq!(decode_mutf7("Lixo Eletr&APQ-nico"), "Lixo Eletrônico");
        assert_eq!(decode_mutf7("A &- B"), "A & B");
        assert_eq!(decode_mutf7("&ZeVnLIqe-"), "日本語");
        assert_eq!(decode_mutf7("sem fim &abc"), "sem fim &abc");
    }

    #[test]
    fn folders_with_roles_and_names() {
        let responses = vec![
            response("* LIST (\\HasNoChildren) \"/\" INBOX", &[]),
            response("* LIST (\\Noselect \\HasChildren) \"/\" \"[Gmail]\"", &[]),
            response(
                "* LIST (\\HasNoChildren \\Sent) \"/\" \"[Gmail]/E-mails enviados\"",
                &[],
            ),
            response(
                "* LIST (\\HasNoChildren) \"/\" \"Itens Exclu&AO0-dos\"",
                &[],
            ),
            response("* LIST (\\HasNoChildren) \"/\" {8}", &[b"Projetos"]),
            response("* LIST (\\HasNoChildren) NIL Enviados", &[]),
            response("* LIST (\\HasNoChildren) \".\" \"Projetos.2024\"", &[]),
        ];
        let folders = parse_list(&responses);
        let names: Vec<(&str, Option<Role>, u32)> = folders
            .iter()
            .map(|f| (f.name.as_str(), f.role, f.depth))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Caixa de Entrada", Some(Role::Inbox), 0),
                ("E-mails enviados", Some(Role::Sent), 1),
                ("Itens Excluídos", Some(Role::Trash), 0),
                ("Projetos", None, 0),
                // Já há Enviados marcado pelo servidor: este fica comum.
                ("Enviados", None, 0),
                ("2024", None, 1),
            ]
        );
        assert_eq!(folders[2].path, "Itens Exclu&AO0-dos");
    }

    #[test]
    fn server_flags_win_over_name_guesses() {
        let responses = vec![
            response("* LIST () \"/\" Trash", &[]),
            response("* LIST (\\Trash) \"/\" \"[Gmail]/Lixeira\"", &[]),
        ];
        let folders = parse_list(&responses);
        let trash: Vec<&str> = folders
            .iter()
            .filter(|f| f.role == Some(Role::Trash))
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(trash, vec!["[Gmail]/Lixeira"]);
    }

    #[test]
    fn no_expunge_of_others_without_uidplus() {
        let (stream, server) = fake_server(|r, w| {
            let (tag, _) = read(r);
            w.write_all(format!("* CAPABILITY IMAP4rev1\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"INBOX\"");
            w.write_all(format!("* 3 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID COPY 5 \"Arquivo\"");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID STORE 5 +FLAGS.SILENT (\\Deleted)");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            // Outra marcada por outro programa: não expurga.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID SEARCH DELETED");
            w.write_all(format!("* SEARCH 2 5\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // Só a nossa: expurga.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID COPY 6 \"Arquivo\"");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, _) = read(r);
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID SEARCH DELETED");
            w.write_all(format!("* SEARCH 6\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "EXPUNGE");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
        });
        let imap = Imap::connect(stream).unwrap();
        let mut reader = Reader::new(imap, false).unwrap();
        reader.move_to("INBOX", &[5], "Arquivo").unwrap();
        reader.move_to("INBOX", &[6], "Arquivo").unwrap();
        drop(reader);
        server.join().unwrap();
    }

    #[test]
    fn summary_from_header_and_text() {
        let header = b"From: =?UTF-8?Q?Jo=C3=A3o?= <joao@x.com>\r\nTo: ana@y.com\r\nSubject: Ol\xc3\xa1\r\nDate: Sat, 20 Nov 2021 14:22:01 -0800\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n";
        let r = response(
            "* 1 FETCH (UID 42 FLAGS (\\Answered) RFC822.SIZE 900 BODY[HEADER.FIELDS (FROM TO SUBJECT DATE CONTENT-TYPE)] {0} BODY[TEXT]<0> {0})",
            &[header, b"Primeira linha\r\n\r\n  segunda   linha\r\n"],
        );
        let toks = tokens(&r);
        let s = summary(&fetch_items(&toks).unwrap()).unwrap();
        assert_eq!(s.uid, 42);
        assert_eq!(s.from.name, "João");
        assert_eq!(s.from.email, "joao@x.com");
        assert_eq!(s.to[0].email, "ana@y.com");
        assert_eq!(s.subject, "Olá");
        assert_eq!(s.date, 1_637_446_921);
        assert!(s.answered && !s.seen);
        assert_eq!(s.size, 900);
        assert_eq!(s.preview, "Primeira linha segunda linha");
    }

    #[test]
    fn html_is_cleaned() {
        let mut inline = HashMap::new();
        inline.insert("logo@x".to_owned(), "data:image/png;base64,AAAA".to_owned());
        let (html, remote) = sanitize_html(
            concat!(
                "<html><head><title>T</title><style>p{color:red}</style>",
                "<script>alert(1)</script></head><body onload=x()>",
                "<p style=\"color:blue\" onclick=\"x()\">Oi</p>",
                "<a href=\"javascript:alert(1)\">a</a><a href=\"https://x.com\">b</a>",
                "<a href=\"data:text/html,hi\">c</a><a href=\"/rel\">d</a>",
                "<img src=\"cid:LOGO@x\"><img src=\"cid:outro\">",
                "<img src=\"data:image/svg+xml,<svg/>\"><img src=\"data:image/gif;base64,R0\">",
                "<form action=\"https://x\"><input name=a></form><iframe src=x></iframe>",
                "</body></html>"
            ),
            inline,
        );
        assert!(
            !html.contains("script") && !html.contains("alert"),
            "{html}"
        );
        assert!(
            !html.contains("onclick") && !html.contains("onload"),
            "{html}"
        );
        assert!(!html.contains("<title") && !html.contains(">T<"), "{html}");
        assert!(html.contains("<style>p{color:red}</style>"), "{html}");
        assert!(html.contains("style=\"color:blue\""), "{html}");
        assert!(html.contains("href=\"https://x.com\""), "{html}");
        assert!(html.contains("rel=\"noopener noreferrer\""), "{html}");
        assert!(
            !html.contains("data:text") && !html.contains("/rel"),
            "{html}"
        );
        assert!(
            html.contains("src=\"data:image/png;base64,AAAA\""),
            "{html}"
        );
        assert!(!html.contains("cid:") && !html.contains("svg"), "{html}");
        assert!(html.contains("data:image/gif;base64,R0"), "{html}");
        assert!(
            !html.contains("<form") && !html.contains("<input") && !html.contains("<iframe"),
            "{html}"
        );
        assert!(!remote);

        let (_, remote) = sanitize_html("<img src=\"https://t.x/p.gif\">", HashMap::new());
        assert!(remote);
        let (_, remote) = sanitize_html(
            "<div style=\"background:url(http://x/a.png)\">",
            HashMap::new(),
        );
        assert!(remote);
        let (_, remote) = sanitize_html("<style>@import 'http://x/a.css';</style>", HashMap::new());
        assert!(remote);
        let (_, remote) = sanitize_html("<a href=\"https://x\">link</a>", HashMap::new());
        assert!(!remote);
    }

    #[test]
    fn message_view_with_inline_image_and_attachment() {
        let raw = concat!(
            "From: Ana <ana@x.com>\r\n",
            "To: Bia <bia@y.com>, carlos@z.com\r\n",
            "Cc: dani@w.com\r\n",
            "Subject: =?UTF-8?B?UmVsYXTDs3Jpbw==?=\r\n",
            "Message-ID: <m1@x.com>\r\n",
            "References: <m0@x.com>\r\n",
            "Date: Sat, 20 Nov 2021 14:22:01 -0800\r\n",
            "Content-Type: multipart/mixed; boundary=\"B\"\r\n\r\n",
            "--B\r\n",
            "Content-Type: multipart/related; boundary=\"R\"\r\n\r\n",
            "--R\r\n",
            "Content-Type: text/html; charset=utf-8\r\n\r\n",
            "<p>Veja <img src=\"cid:img1\"></p>\r\n",
            "--R\r\n",
            "Content-Type: image/png\r\n",
            "Content-ID: <img1>\r\n",
            "Content-Disposition: inline\r\n",
            "Content-Transfer-Encoding: base64\r\n\r\n",
            "iVBORw0KGgo=\r\n",
            "--R--\r\n",
            "--B\r\n",
            "Content-Type: application/pdf; name=\"conta.pdf\"\r\n",
            "Content-Disposition: attachment; filename=\"conta.pdf\"\r\n",
            "Content-Transfer-Encoding: base64\r\n\r\n",
            "JVBERi0=\r\n",
            "--B--\r\n"
        );
        let view = message_view(7, raw.as_bytes());
        assert_eq!(view.subject, "Relatório");
        assert_eq!(view.from[0].name, "Ana");
        assert_eq!(view.to.len(), 2);
        assert_eq!(view.cc[0].email, "dani@w.com");
        assert_eq!(view.message_id, "m1@x.com");
        assert_eq!(view.references, vec!["m0@x.com"]);
        let html = view.html.unwrap();
        assert!(
            html.contains("src=\"data:image/png;base64,iVBORw0KGgo=\""),
            "{html}"
        );
        assert_eq!(view.attachments.len(), 1, "{:?}", view.attachments);
        assert_eq!(view.attachments[0].name, "conta.pdf");
        assert_eq!(view.attachments[0].mime, "application/pdf");
        let (name, bytes) = attachment(raw.as_bytes(), view.attachments[0].index).unwrap();
        assert_eq!(name, "conta.pdf");
        assert_eq!(bytes, b"%PDF-");
    }

    #[test]
    fn plain_text_message_has_no_html() {
        let view = message_view(1, b"From: a@x.com\r\nSubject: oi\r\n\r\nso texto <b>\r\n");
        assert!(view.html.is_none());
        assert!(view.text.contains("so texto <b>"));
    }

    fn fake_server(
        script: impl FnOnce(&mut BufReader<TcpStream>, &mut TcpStream) + Send + 'static,
    ) -> (TcpStream, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            out.write_all(b"* OK pronto\r\n").unwrap();
            script(&mut reader, &mut out);
        });
        (TcpStream::connect(addr).unwrap(), handle)
    }

    fn read(reader: &mut BufReader<TcpStream>) -> (String, String) {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let (tag, rest) = line.trim_end().split_once(' ').unwrap();
        (tag.to_owned(), rest.to_owned())
    }

    #[test]
    fn reader_opens_big_message_in_parts_and_more() {
        let (stream, server) = fake_server(|r, w| {
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "CAPABILITY");
            w.write_all(format!("* CAPABILITY IMAP4rev1 UIDPLUS MOVE\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"INBOX\"");
            w.write_all(format!("* 3 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();

            // Estrutura primeiro; grande: só o texto.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID FETCH 7 (UID RFC822.SIZE BODYSTRUCTURE)");
            w.write_all(format!("* 3 FETCH (UID 7 RFC822.SIZE 5000000 BODYSTRUCTURE ((\"TEXT\" \"PLAIN\" (\"CHARSET\" \"utf-8\") NIL NIL \"7BIT\" 5 1 NIL NIL NIL)(\"APPLICATION\" \"PDF\" (\"NAME\" \"a.pdf\") NIL NIL \"BASE64\" 4000000 NIL (\"ATTACHMENT\" (\"FILENAME\" \"a.pdf\")) NIL) \"MIXED\" (\"BOUNDARY\" \"x\") NIL NIL))\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(
                cmd,
                "UID FETCH 7 (UID FLAGS BODY[HEADER] BODY.PEEK[1.MIME] BODY.PEEK[2.MIME] BODY.PEEK[1])"
            );
            let head = b"From: a@x.com\r\nSubject: grande\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n";
            let m1 = b"Content-Type: text/plain; charset=utf-8\r\n\r\n";
            let m2 = b"Content-Type: application/pdf\r\nContent-Disposition: attachment; filename=a.pdf\r\nContent-Transfer-Encoding: base64\r\n\r\n";
            w.write_all(
                format!(
                    "* 3 FETCH (UID 7 FLAGS (\\Seen) BODY[HEADER] {{{}}}\r\n",
                    head.len()
                )
                .as_bytes(),
            )
            .unwrap();
            w.write_all(head).unwrap();
            w.write_all(format!(" BODY[1.MIME] {{{}}}\r\n", m1.len()).as_bytes())
                .unwrap();
            w.write_all(m1).unwrap();
            w.write_all(format!(" BODY[2.MIME] {{{}}}\r\n", m2.len()).as_bytes())
                .unwrap();
            w.write_all(m2).unwrap();
            w.write_all(b" BODY[1] {5}\r\nhello)\r\n").unwrap();
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();

            // Mover devolve o UID novo (COPYUID).
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID MOVE 7 \"Arquivo\"");
            w.write_all(
                format!("* OK [COPYUID 9 7 99] ok\r\n* 3 EXPUNGE\r\n{tag} OK\r\n").as_bytes(),
            )
            .unwrap();

            // Marcar todas como lidas.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"INBOX\"");
            w.write_all(format!("* 2 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "STORE 1:* +FLAGS.SILENT (\\Seen)");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();

            // Filtro: não lidas.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"INBOX\"");
            w.write_all(format!("* 2 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID SEARCH UNSEEN");
            w.write_all(format!("* SEARCH\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
        });
        let imap = Imap::connect(stream).unwrap();
        let mut reader = Reader::new(imap, false).unwrap();
        let opened = reader.open_message("INBOX", 7, true, None).unwrap();
        assert!(opened.raw.is_none());
        assert_eq!(opened.view.subject, "grande");
        assert!(opened.view.text.contains("hello"));
        assert!(opened.view.seen);
        assert_eq!(opened.view.attachments.len(), 1);
        assert_eq!(opened.view.attachments[0].name, "a.pdf");
        assert_eq!(opened.view.attachments[0].size, 3_000_000);

        assert_eq!(reader.move_to("INBOX", &[7], "Arquivo").unwrap(), vec![99]);
        reader.mark_all_seen("INBOX").unwrap();
        let page = reader.list_from("INBOX", 0, "", Filter::Unread).unwrap();
        assert_eq!(page.total, 0);
        server.join().unwrap();
    }

    #[test]
    fn reader_lists_pages_moves_and_deletes() {
        let (stream, server) = fake_server(|r, w| {
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "CAPABILITY");
            w.write_all(format!("* CAPABILITY IMAP4rev1 UIDPLUS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();

            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"INBOX\"");
            w.write_all(
                format!("* 120 EXISTS\r\n* OK [UIDVALIDITY 5]\r\n{tag} OK [READ-WRITE]\r\n")
                    .as_bytes(),
            )
            .unwrap();
            let (tag, cmd) = read(r);
            assert!(cmd.starts_with("FETCH 21:70 (UID FLAGS"), "{cmd}");
            let header = b"From: a@x.com\r\nSubject: um\r\n\r\n";
            w.write_all(
                format!(
                    "* 21 FETCH (UID 300 FLAGS () BODY[HEADER.FIELDS (FROM)] {{{}}}\r\n",
                    header.len()
                )
                .as_bytes(),
            )
            .unwrap();
            w.write_all(header).unwrap();
            w.write_all(b" BODY[TEXT]<0> {2}\r\nok)\r\n").unwrap();
            w.write_all(
                b"* 22 FETCH (UID 301 FLAGS (\\Seen) BODY[HEADER.FIELDS (FROM)] {2}\r\n\r\n)\r\n",
            )
            .unwrap();
            // Aviso solto (sem cabeçalho) e marcada para apagar: fora da lista.
            w.write_all(b"* 22 FETCH (UID 301 FLAGS (\\Seen))\r\n")
                .unwrap();
            w.write_all(b"* 23 FETCH (UID 302 FLAGS (\\Deleted) BODY[HEADER.FIELDS (FROM)] {2}\r\n\r\n)\r\n")
                .unwrap();
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();

            // Apagar: busca a Lixeira, COPY + expurgo (sem MOVE).
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "LIST \"\" \"*\"");
            w.write_all(
                format!("* LIST (\\Trash) \"/\" Lixeira\r\n* LIST () \"/\" INBOX\r\n{tag} OK\r\n")
                    .as_bytes(),
            )
            .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID COPY 300,301 \"Lixeira\"");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID STORE 300,301 +FLAGS.SILENT (\\Deleted)");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID EXPUNGE 300,301");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();

            // Na própria Lixeira: apaga de vez.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"Lixeira\"");
            w.write_all(format!("* 1 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID STORE 9 +FLAGS.SILENT (\\Deleted)");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID EXPUNGE 9");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();

            // Busca com acento: CHARSET e literal.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"INBOX\"");
            w.write_all(format!("* 3 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (search_tag, cmd) = read(r);
            assert_eq!(cmd, "UID SEARCH CHARSET UTF-8 TEXT {7}");
            w.write_all(b"+ ok\r\n").unwrap();
            let mut bytes = [0u8; 7];
            std::io::Read::read_exact(r, &mut bytes).unwrap();
            assert_eq!(&bytes, "relató".as_bytes());
            let mut end = String::new();
            r.read_line(&mut end).unwrap();
            w.write_all(format!("* SEARCH 4 8\r\n{search_tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert!(cmd.starts_with("UID FETCH 8,4 (UID"), "{cmd}");
            w.write_all(
                format!("* 2 FETCH (UID 8 BODY[HEADER] {{2}}\r\n\r\n)\r\n* 1 FETCH (UID 4 BODY[HEADER] {{2}}\r\n\r\n)\r\n{tag} OK\r\n").as_bytes(),
            )
            .unwrap();

            // Mensagem inteira.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID FETCH 8 (UID FLAGS BODY.PEEK[])");
            w.write_all(b"* 2 FETCH (UID 8 FLAGS (\\Seen) BODY[] {4}\r\nabcd)\r\n")
                .unwrap();
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
        });
        let imap = Imap::connect(stream).unwrap();
        let mut reader = Reader::new(imap, false).unwrap();
        let page = reader.list("INBOX", 1, "").unwrap();
        assert_eq!(page.total, 120);
        assert_eq!(
            page.messages.iter().map(|m| m.uid).collect::<Vec<_>>(),
            vec![301, 300]
        );
        assert_eq!(page.messages[1].subject, "um");
        assert_eq!(page.messages[1].preview, "ok");
        assert!(page.messages[0].seen);

        reader.delete("INBOX", &[301, 300]).unwrap();
        reader.delete("Lixeira", &[9]).unwrap();

        let page = reader.list("INBOX", 0, "relató").unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(
            page.messages.iter().map(|m| m.uid).collect::<Vec<_>>(),
            vec![8, 4]
        );

        let (raw, seen, flagged) = reader.fetch_raw("INBOX", 8, false).unwrap();
        assert_eq!(raw, b"abcd");
        assert!(seen && !flagged);
        drop(reader);
        server.join().unwrap();
    }

    #[test]
    fn flags_are_checked() {
        let (stream, server) = fake_server(|r, w| {
            let (tag, _) = read(r);
            w.write_all(format!("* CAPABILITY IMAP4rev1 MOVE\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, _) = read(r);
            w.write_all(format!("* 2 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID STORE 1,2 -FLAGS.SILENT (\\Seen \\Flagged)");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID MOVE 2 \"Arquivo\"");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (_, cmd) = read(r);
            assert_eq!(cmd, "APPEND \"Enviados\" (\\Seen) {5}");
            w.write_all(b"+ ok\r\n").unwrap();
            let mut bytes = [0u8; 5];
            std::io::Read::read_exact(r, &mut bytes).unwrap();
            assert_eq!(&bytes, b"Hello");
            let mut end = String::new();
            r.read_line(&mut end).unwrap();
            w.write_all(b"p5 OK\r\n").unwrap();
        });
        let imap = Imap::connect(stream).unwrap();
        let mut reader = Reader::new(imap, false).unwrap();
        reader
            .set_flags("INBOX", &[2, 1], &["\\Seen", "\\Flagged"], false)
            .unwrap();
        assert!(reader
            .set_flags("INBOX", &[1], &["\\Seen) (x"], true)
            .is_err());
        reader.move_to("INBOX", &[2], "Arquivo").unwrap();
        reader.append("Enviados", b"Hello").unwrap();
        assert!(reader.move_to("INBOX", &[1], "a\r\nb").is_err());
        drop(reader);
        server.join().unwrap();
    }

    #[test]
    fn draft_is_saved_before_old_versions_go() {
        let (stream, server) = fake_server(|r, w| {
            let (tag, _) = read(r);
            w.write_all(format!("* CAPABILITY IMAP4rev1 UIDPLUS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"Rascunhos\"");
            w.write_all(format!("* 1 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(
                cmd,
                "UID SEARCH HEADER Message-ID \"<papeo-draft-1@papeo.draft>\""
            );
            w.write_all(format!("* SEARCH 7\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // A versão nova entra antes de a antiga sair.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "APPEND \"Rascunhos\" (\\Seen \\Draft) {2}");
            w.write_all(b"+ ok\r\n").unwrap();
            let mut bytes = [0u8; 2];
            std::io::Read::read_exact(r, &mut bytes).unwrap();
            assert_eq!(&bytes, b"v2");
            let mut end = String::new();
            r.read_line(&mut end).unwrap();
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID STORE 7 +FLAGS.SILENT (\\Deleted)");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID EXPUNGE 7");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            // Descartar sem versões salvas: só procura.
            let (tag, cmd) = read(r);
            assert_eq!(
                cmd,
                "UID SEARCH HEADER Message-ID \"<papeo-draft-2@papeo.draft>\""
            );
            w.write_all(format!("* SEARCH\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
        });
        let imap = Imap::connect(stream).unwrap();
        let mut reader = Reader::new(imap, false).unwrap();
        reader
            .save_draft("Rascunhos", b"v2", "<papeo-draft-1@papeo.draft>")
            .unwrap();
        reader
            .discard_drafts("Rascunhos", "<papeo-draft-2@papeo.draft>")
            .unwrap();
        // Identificador com aspas ou espaço não vai para o servidor.
        assert!(reader.discard_drafts("Rascunhos", "a\" b").is_err());
        drop(reader);
        server.join().unwrap();
    }

    #[test]
    fn draft_view_reads_what_the_editor_needs() {
        let raw = "From: eu@x.com\r\nTo: \"Silva, Ana\" <ana@x.com>, bia@x.com\r\nBcc: Chefe <c@x.com>\r\nSubject: Re: oi\r\nMessage-ID: <papeo-draft-9@papeo.draft>\r\nIn-Reply-To: <orig@x.com>\r\nReferences: <a@x.com> <orig@x.com>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nTexto\r\n--b\r\nContent-Type: application/pdf; name=a.pdf\r\nContent-Disposition: attachment; filename=a.pdf\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n--b--\r\n";
        let v = draft_view(raw.as_bytes()).unwrap();
        assert_eq!(
            v.to,
            [
                "\"Silva Ana\" <ana@x.com>".replace("Silva Ana", "Silva, Ana"),
                "bia@x.com".into()
            ]
        );
        assert_eq!(v.bcc, ["Chefe <c@x.com>"]);
        assert_eq!(v.subject, "Re: oi");
        assert_eq!(v.text.trim(), "Texto");
        assert_eq!(v.message_id, "papeo-draft-9@papeo.draft");
        assert_eq!(v.in_reply_to.as_deref(), Some("orig@x.com"));
        assert_eq!(v.references, ["a@x.com", "orig@x.com"]);
        assert_eq!(v.attachments.len(), 1);
        assert_eq!(v.attachments[0].0, "a.pdf");
        assert_eq!(v.attachments[0].2, b"%PDF-");
        // Endereço incompleto guardado pelo Papeo volta ao campo dele.
        let enc = encode_unparsed(&["ana@".into(), "Zé <".into()]);
        let raw = format!("From: eu@x.com\r\n{UNPARSED_BCC}: {enc}\r\nSubject: x\r\n\r\noi\r\n");
        let v = draft_view(raw.as_bytes()).unwrap();
        assert_eq!(v.bcc, ["ana@", "Zé <"]);
        assert!(v.to.is_empty());
        // Valor longo: quebrado em pedaços (e dobrado em linhas) volta igual.
        let many: Vec<String> = (0..40).map(|i| format!("pessoa{i}@")).collect();
        let enc = encode_unparsed(&many);
        assert!(enc.split(' ').all(|c| c.len() <= 76));
        let folded = enc.replace(' ', "\r\n ");
        let raw = format!("From: eu@x.com\r\n{UNPARSED_TO}: {folded}\r\n\r\noi\r\n");
        assert_eq!(draft_view(raw.as_bytes()).unwrap().to, many);
    }

    #[test]
    fn foreign_draft_is_only_discarded_if_still_there() {
        let (stream, server) = fake_server(|r, w| {
            let (tag, _) = read(r);
            w.write_all(format!("* CAPABILITY IMAP4rev1 UIDPLUS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "SELECT \"Drafts\"");
            w.write_all(format!("* 3 EXISTS\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // O UID 4 virou outra mensagem: não apaga.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID SEARCH UID 4 HEADER Message-ID \"abc@x.com\"");
            w.write_all(format!("* SEARCH\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // Sem Message-ID: só se ainda estiver marcado como rascunho.
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID SEARCH UID 5 DRAFT");
            w.write_all(format!("* SEARCH 5\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID STORE 5 +FLAGS.SILENT (\\Deleted)");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let (tag, cmd) = read(r);
            assert_eq!(cmd, "UID EXPUNGE 5");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
        });
        let imap = Imap::connect(stream).unwrap();
        let mut reader = Reader::new(imap, false).unwrap();
        reader.discard_checked("Drafts", 4, "abc@x.com").unwrap();
        reader.discard_checked("Drafts", 5, "").unwrap();
        drop(reader);
        server.join().unwrap();
    }
}
