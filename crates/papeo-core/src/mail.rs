//! Avisos de e-mail por IMAP, sem abrir o site: configuração da conta,
//! um cliente IMAP mínimo (só leitura) e o vigia da Caixa de Entrada.
//!
//! O cliente fala com qualquer fluxo `Read + Write`: o app entrega a conexão
//! TLS; os testes, uma conexão local com um servidor falso. Nada aqui marca
//! e-mail como lido: a caixa é aberta com `EXAMINE` (só leitura) e os
//! cabeçalhos vêm com `BODY.PEEK`.

use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::time::Duration;

/// Porta do IMAP com TLS direto.
pub const IMAPS_PORT: u16 = 993;
/// Porta do IMAP comum (cifrada com STARTTLS).
pub const IMAP_PORT: u16 = 143;
const USER_MAX: usize = 254;
const HOST_MAX: usize = 253;
/// Senha ou senha de app: o bastante para qualquer provedor.
pub const SECRET_MAX: usize = 512;
/// Teto de uma linha de resposta (busca com muitas não lidas cabe folgada).
const LINE_MAX: usize = 4 * 1024 * 1024;
/// Teto de um bloco literal (cabeçalhos De e Assunto).
const LITERAL_MAX: usize = 256 * 1024;
/// Teto da soma dos literais de uma resposta.
const LITERALS_MAX: usize = 1024 * 1024;
/// E-mails novos avisados de uma vez (os mais novos); o resto só conta.
pub const NOTIFY_MAX: usize = 3;

/// Conta IMAP ligada a uma caixa de e-mail do Papeo. A senha nunca fica
/// aqui (vai para o cofre do sistema ou um arquivo só do usuário).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailAccount {
    pub host: String,
    #[serde(default = "imaps_port")]
    pub port: u16,
    pub user: String,
    /// Deixa o site hibernado enquanto o IMAP avisa (economiza memória).
    #[serde(default = "yes")]
    pub sleep_site: bool,
    /// Conexão comum que passa a ser cifrada com STARTTLS (porta 143), para
    /// provedores sem TLS direto. Nunca cai para sem criptografia.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starttls: bool,
    /// Gmail: conta a Caixa de Entrada inteira, não só a aba Principal
    /// (para quem não usa as abas).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub whole_inbox: bool,
    /// Como entrar: senha (padrão) ou login da Microsoft (Outlook).
    #[serde(default, skip_serializing_if = "MailAuth::is_password")]
    pub auth: MailAuth,
    /// Assinatura em texto que o leitor põe nas mensagens (spec 16).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub signature: String,
    /// Assinatura formatada (HTML limpo, spec 22); vazia = só texto.
    /// Compartilhada (`Arc`): o estado é copiado a cada publicação e a
    /// formatada pode ter centenas de KB.
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub signature_html: std::sync::Arc<str>,
}

/// Teto da assinatura, em caracteres.
pub const SIGNATURE_MAX: usize = 2000;

/// Assinatura limpa: sem controles (exceto quebra de linha e tab), sem
/// espaços sobrando nas pontas, cortada no teto.
pub fn normalize_signature(raw: &str) -> String {
    let text: String = raw
        .replace("\r\n", "\n")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(SIGNATURE_MAX)
        .collect();
    text.trim().to_owned()
}

/// Forma de entrar no servidor IMAP.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailAuth {
    /// Usuário e senha (ou senha de app).
    #[default]
    Password,
    /// Login da Microsoft (OAuth, XOAUTH2): o "segredo" guardado é o token de
    /// renovação.
    Microsoft,
}

impl MailAuth {
    fn is_password(&self) -> bool {
        *self == MailAuth::Password
    }
}

/// Servidor IMAP do Outlook (pessoal e Microsoft 365).
pub const OUTLOOK_HOST: &str = "outlook.office365.com";

/// SMTP com TLS direto e com STARTTLS (envio).
pub const SMTPS_PORT: u16 = 465;
pub const SUBMISSION_PORT: u16 = 587;

/// Servidor de envio de uma conta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpServer {
    pub host: String,
    pub port: u16,
    pub starttls: bool,
}

fn imaps_port() -> u16 {
    IMAPS_PORT
}

fn yes() -> bool {
    true
}

impl MailAccount {
    /// Confere e normaliza servidor, porta e usuário.
    pub fn sanitized(mut self) -> Result<Self, String> {
        self.host = normalize_host(&self.host)?;
        if self.port == 0 {
            return Err(tr("Porta inválida.").into());
        }
        // Na 993 o servidor espera TLS desde o começo: STARTTLS só travaria.
        if self.starttls && self.port == IMAPS_PORT {
            return Err(tr("STARTTLS usa a porta 143; a 993 é para TLS direto.").into());
        }
        let user = self.user.trim();
        if user.is_empty() {
            return Err(tr("Informe o usuário (normalmente, o endereço de e-mail).").into());
        }
        if user.chars().count() > USER_MAX || user.chars().any(char::is_control) {
            return Err(tr("Usuário inválido.").into());
        }
        self.user = user.to_owned();
        // Login da Microsoft: o token só vai para o servidor da Microsoft.
        if self.auth == MailAuth::Microsoft
            && (self.host != OUTLOOK_HOST || self.port != IMAPS_PORT || self.starttls)
        {
            return Err(tr("O login da Microsoft só vale para o servidor do Outlook.").into());
        }
        // A opção do contador só existe no Gmail.
        if !self.is_gmail() {
            self.whole_inbox = false;
        }
        self.signature = normalize_signature(&self.signature);
        // Sem a versão em texto não há onde pôr a formatada (spec 22, R4).
        self.signature_html = if self.signature.is_empty() {
            Default::default()
        } else {
            crate::signature::sanitize_signature_html(&self.signature_html)
                .unwrap_or_default()
                .into()
        };
        Ok(self)
    }

    /// Gmail com o contador da aba Principal (o número do site).
    pub fn primary_only(&self) -> bool {
        self.is_gmail() && !self.whole_inbox
    }

    pub fn is_gmail(&self) -> bool {
        self.host == "imap.gmail.com" || self.host == "imap.googlemail.com"
    }

    /// Servidor de envio (SMTP) desta conta, tirado do de leitura: provedor
    /// conhecido pela tabela; outro, `imap.` vira `smtp.` (ou o mesmo
    /// servidor), na 465 com TLS direto. Fica preso ao servidor de leitura
    /// (a senha guardada vale para os dois).
    pub fn smtp(&self) -> SmtpServer {
        let host = self.host.as_str();
        let tls = |host: &str| SmtpServer {
            host: host.to_owned(),
            port: SMTPS_PORT,
            starttls: false,
        };
        let starttls = |host: &str| SmtpServer {
            host: host.to_owned(),
            port: SUBMISSION_PORT,
            starttls: true,
        };
        match host {
            "imap.gmail.com" | "imap.googlemail.com" => tls("smtp.gmail.com"),
            OUTLOOK_HOST | "imap-mail.outlook.com" => {
                let domain = self
                    .user
                    .rsplit_once('@')
                    .map(|(_, d)| d.to_ascii_lowercase())
                    .unwrap_or_default();
                let personal = ["hotmail.", "outlook.", "live.", "msn.", "passport."]
                    .iter()
                    .any(|p| domain.starts_with(p));
                if personal {
                    starttls("smtp-mail.outlook.com")
                } else {
                    starttls("smtp.office365.com")
                }
            }
            "email-ssl.com.br" => tls("email-ssl.com.br"),
            "imap.kinghost.net" => tls("smtp.kinghost.net"),
            "imap.uni5.net" => tls("smtp.uni5.net"),
            "imap.hostinger.com" => tls("smtp.hostinger.com"),
            "imap.uol.com.br" => tls("smtps.uol.com.br"),
            "imap.bol.com.br" => tls("smtps.bol.com.br"),
            "imap.terra.com.br" => starttls("smtp.terra.com.br"),
            "imap.mail.yahoo.com" => tls("smtp.mail.yahoo.com"),
            "imap.mail.me.com" => starttls("smtp.mail.me.com"),
            _ => match host.strip_prefix("imap.") {
                Some(rest) => tls(&format!("smtp.{rest}")),
                None => tls(host),
            },
        }
    }

    /// O provedor já guarda o enviado por SMTP na pasta Enviados.
    pub fn saves_sent(&self) -> bool {
        self.is_gmail() || self.host == OUTLOOK_HOST || self.host == "imap-mail.outlook.com"
    }

    /// Senha como o servidor espera. A senha de app do Google aparece em
    /// grupos ("abcd efgh ijkl mnop"); os espaços não fazem parte dela.
    pub fn login_secret(&self, secret: &str) -> String {
        if self.is_gmail() {
            secret.chars().filter(|c| !c.is_whitespace()).collect()
        } else {
            secret.to_owned()
        }
    }
}

/// Nome de servidor: letras, números, ponto e hífen (ou um IPv4).
fn normalize_host(raw: &str) -> Result<String, String> {
    let host = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let ok = !host.is_empty()
        && host.len() <= HOST_MAX
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
    if ok {
        Ok(host)
    } else {
        Err(tr("Servidor inválido; use algo como imap.exemplo.com.br.").into())
    }
}

/// Confere a senha antes de guardar.
pub fn check_secret(secret: &str) -> Result<(), String> {
    if secret.is_empty() {
        return Err(tr("Informe a senha.").into());
    }
    if secret.len() > SECRET_MAX || secret.chars().any(char::is_control) {
        return Err(tr("Senha inválida.").into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cabeçalhos

/// Remetente e assunto de um e-mail novo, já decodificados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMail {
    pub uid: u32,
    /// Nome de quem enviou (ou o endereço, sem nome).
    pub from: String,
    pub subject: String,
}

/// Lê De e Assunto de um bloco de cabeçalhos (linhas dobradas unidas,
/// palavras codificadas da RFC 2047 decodificadas).
pub fn parse_headers(raw: &[u8]) -> (String, String) {
    let text = decode_bytes(raw);
    let mut from = String::new();
    let mut subject = String::new();
    let mut current: Option<(String, String)> = None;
    let mut finish = |field: Option<(String, String)>| {
        if let Some((name, value)) = field {
            match name.as_str() {
                "from" if from.is_empty() => from = display_from(&decode_words(&value)),
                "subject" if subject.is_empty() => subject = decode_words(&value),
                _ => {}
            }
        }
    };
    for line in text.split('\n').map(|l| l.trim_end_matches('\r')) {
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = current.as_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        finish(current.take());
        if let Some((name, value)) = line.split_once(':') {
            current = Some((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    finish(current);
    (clean_text(&from), clean_text(&subject))
}

/// Sem quebras nem caracteres de controle; espaços repetidos viram um.
fn clean_text(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Nome de exibição de um campo De: `"Ana" <ana@x>` → `Ana`;
/// `<ana@x>` ou `ana@x` → `ana@x`.
fn display_from(value: &str) -> String {
    let value = value.trim();
    let (name, addr) = match value.rfind('<') {
        Some(start) => {
            let addr = value[start + 1..].trim_end().trim_end_matches('>').trim();
            (value[..start].trim(), addr)
        }
        None => ("", value),
    };
    let name = name.trim_matches('"').trim().replace("\\\"", "\"");
    if name.is_empty() {
        addr.to_owned()
    } else {
        name
    }
}

/// Bytes de cabeçalho: UTF-8 quando válido; senão Windows-1252 (o comum em
/// servidores antigos que mandam acentos sem codificar).
fn decode_bytes(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(text) => text.to_owned(),
        Err(_) => decode_charset("windows-1252", raw),
    }
}

/// Decodifica as palavras `=?charset?B|Q?texto?=` (RFC 2047). Espaço entre
/// duas palavras codificadas seguidas some, como manda a norma.
pub fn decode_words(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    let mut last_encoded = false;
    while let Some(start) = rest.find("=?") {
        let (before, tail) = rest.split_at(start);
        let decoded = parse_word(tail);
        match decoded {
            Some((text, used)) => {
                if !(last_encoded && before.trim().is_empty()) {
                    out.push_str(before);
                }
                out.push_str(&text);
                rest = &tail[used..];
                last_encoded = true;
            }
            None => {
                out.push_str(before);
                out.push_str("=?");
                rest = &tail[2..];
                last_encoded = false;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Uma palavra codificada no começo de `text`: (texto, bytes consumidos).
fn parse_word(text: &str) -> Option<(String, usize)> {
    let rest = text.strip_prefix("=?")?;
    let (charset, rest) = rest.split_once('?')?;
    let (encoding, rest) = rest.split_once('?')?;
    let end = rest.find("?=")?;
    let payload = &rest[..end];
    if payload.contains(' ') || charset.is_empty() {
        return None;
    }
    let used = 2 + charset.len() + 1 + encoding.len() + 1 + end + 2;
    // Idioma opcional (RFC 2231): "UTF-8*pt".
    let charset = charset.split('*').next().unwrap_or(charset);
    let bytes = match encoding {
        "B" | "b" => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(payload.trim_end_matches('='))
                .ok()?
        }
        "Q" | "q" => decode_q(payload)?,
        _ => return None,
    };
    Some((decode_charset(charset, &bytes), used))
}

fn decode_q(payload: &str) -> Option<Vec<u8>> {
    let bytes = payload.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'_' => out.push(b' '),
            b'=' => {
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    Some(out)
}

/// Converte bytes de um conjunto de caracteres para texto. Cobre os usados
/// no Brasil (UTF-8, Latin-1, Windows-1252); os demais viram UTF-8 com
/// substituição dos bytes inválidos.
fn decode_charset(charset: &str, bytes: &[u8]) -> String {
    const CP1252: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match charset.to_ascii_lowercase().as_str() {
        "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" | "iso-8859-15" | "windows-1252"
        | "cp1252" | "us-ascii" | "ascii" => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9f => CP1252[usize::from(b - 0x80)],
                _ => char::from(b),
            })
            .collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

// ---------------------------------------------------------------------------
// Cliente IMAP

#[derive(Debug)]
pub enum ImapError {
    /// Rede: sem conexão, tempo esgotado, conexão caída.
    Io(io::Error),
    /// Usuário ou senha recusados (texto do servidor).
    Auth(String),
    /// Comando recusado pelo servidor (NO).
    Rejected(String),
    /// Resposta inesperada ou erro do servidor (BAD e afins).
    Protocol(String),
}

impl ImapError {
    /// Tempo de espera esgotado na leitura (não é erro durante o IDLE).
    pub fn is_timeout(&self) -> bool {
        matches!(self, ImapError::Io(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
    }
}

impl From<io::Error> for ImapError {
    fn from(err: io::Error) -> Self {
        ImapError::Io(err)
    }
}

impl std::fmt::Display for ImapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImapError::Io(e) => f.write_str(&trf("rede: {erro}", &[("erro", e)])),
            ImapError::Auth(m) => f.write_str(&trf("senha recusada: {erro}", &[("erro", m)])),
            ImapError::Rejected(m) | ImapError::Protocol(m) => {
                f.write_str(&trf("servidor: {erro}", &[("erro", m)]))
            }
        }
    }
}

pub type ImapResult<T> = Result<T, ImapError>;

/// Fluxo da conexão com o tempo máximo de espera ajustável.
pub trait Transport: Read + Write {
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()>;
}

impl Transport for std::net::TcpStream {
    fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.set_read_timeout(Some(timeout))?;
        self.set_write_timeout(Some(timeout))
    }
}

/// Uma resposta do servidor: a linha (literais trocados por `{n}`) e os
/// blocos literais, em ordem.
#[derive(Debug, Default)]
pub(crate) struct Response {
    pub(crate) line: String,
    pub(crate) literals: Vec<Vec<u8>>,
}

/// Situação da Caixa de Entrada ao abrir.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mailbox {
    pub uid_validity: Option<u32>,
    pub uid_next: Option<u32>,
}

pub struct Imap<S: Transport> {
    io: BufReader<S>,
    tag: u32,
    /// Tag do IDLE em curso.
    idling: Option<String>,
    /// Saudação PREAUTH: já autenticado, sem LOGIN.
    preauth: bool,
    /// O servidor avisou de mudança na caixa fora do IDLE (junto da
    /// resposta de outro comando).
    changed: bool,
    /// Teto de um bloco literal e da soma deles numa resposta (o leitor
    /// aumenta para receber e-mails inteiros).
    literal_max: usize,
    literals_max: usize,
    /// Linha do último OK com tag (traz códigos como `[COPYUID …]`).
    pub(crate) last_ok: String,
}

/// Argumento de comando: texto entre aspas ou, com caracteres fora do
/// ASCII visível, literal (`{n}` seguido dos bytes).
pub(crate) fn push_string(parts: &mut Vec<Part>, text: &str) {
    if text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        parts.push(Part::Text(format!("\"{escaped}\"")));
    } else {
        parts.push(Part::Literal(text.as_bytes().to_vec()));
    }
}

pub(crate) enum Part {
    Text(String),
    Literal(Vec<u8>),
}

impl<S: Transport> Imap<S> {
    /// Recebe a saudação do servidor.
    pub fn connect(stream: S) -> ImapResult<Self> {
        let mut imap = Self {
            io: BufReader::new(stream),
            tag: 0,
            idling: None,
            preauth: false,
            changed: false,
            literal_max: LITERAL_MAX,
            literals_max: LITERALS_MAX,
            last_ok: String::new(),
        };
        let greeting = imap.read_response()?;
        let mut words = greeting.line.split_whitespace();
        match (words.next(), words.next().map(str::to_ascii_uppercase)) {
            (Some("*"), Some(w)) if w == "OK" => Ok(imap),
            (Some("*"), Some(w)) if w == "PREAUTH" => {
                imap.preauth = true;
                Ok(imap)
            }
            _ => Err(ImapError::Protocol(clip(&greeting.line))),
        }
    }

    /// Sessão já cifrada por STARTTLS: o servidor não manda outra saudação.
    pub fn after_starttls(stream: S) -> Self {
        Self {
            io: BufReader::new(stream),
            tag: 0,
            idling: None,
            preauth: false,
            changed: false,
            literal_max: LITERAL_MAX,
            literals_max: LITERALS_MAX,
            last_ok: String::new(),
        }
    }

    /// Aceita respostas com até `bytes` em literais (e-mail inteiro).
    pub fn allow_large(&mut self, bytes: usize) {
        self.literal_max = bytes.max(LITERAL_MAX);
        self.literals_max = bytes.max(LITERALS_MAX);
    }

    /// Marca aviso de mudança recebido fora do IDLE (a resposta ao nosso
    /// próprio `FETCH` de cabeçalhos não conta).
    fn note_change(&mut self, response: &Response) {
        if is_change(&response.line) && !response.line.contains("BODY[") {
            self.changed = true;
        }
    }

    /// Houve aviso de mudança desde a última consulta.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn stream_mut(&mut self) -> &mut S {
        self.io.get_mut()
    }

    pub fn set_timeout(&mut self, timeout: Duration) -> io::Result<()> {
        self.io.get_mut().set_timeout(timeout)
    }

    fn read_line(&mut self, out: &mut Vec<u8>) -> io::Result<()> {
        let start = out.len();
        let limit = (LINE_MAX + 2) as u64;
        let read = (&mut self.io).take(limit).read_until(b'\n', out)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                tr("conexão encerrada"),
            ));
        }
        if !out.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                tr("linha longa demais"),
            ));
        }
        out.truncate(out.len() - 1);
        if out.ends_with(b"\r") {
            out.truncate(out.len() - 1);
        }
        if out.len() - start > LINE_MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                tr("linha longa demais"),
            ));
        }
        Ok(())
    }

    /// Lê uma resposta inteira, juntando os literais `{n}`.
    fn read_response(&mut self) -> io::Result<Response> {
        let mut response = Response::default();
        let mut line = Vec::new();
        let mut total = 0usize;
        loop {
            self.read_line(&mut line)?;
            if line.len() > LINE_MAX {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "resposta longa demais",
                ));
            }
            let Some(size) = literal_size(&line) else {
                break;
            };
            total += size;
            if size > self.literal_max || total > self.literals_max {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "bloco longo demais",
                ));
            }
            let mut literal = vec![0; size];
            self.io.read_exact(&mut literal)?;
            response.literals.push(literal);
            if response.literals.len() > 64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "resposta longa demais",
                ));
            }
        }
        response.line = String::from_utf8_lossy(&line).into_owned();
        Ok(response)
    }

    fn next_tag(&mut self) -> String {
        self.tag += 1;
        format!("p{}", self.tag)
    }

    /// Envia um comando e junta as respostas sem tag até a resposta dele.
    pub(crate) fn command(&mut self, parts: Vec<Part>) -> ImapResult<Vec<Response>> {
        let tag = self.next_tag();
        let mut pending = tag.clone().into_bytes();
        let mut untagged = Vec::new();
        for part in parts {
            pending.push(b' ');
            match part {
                Part::Text(text) => pending.extend_from_slice(text.as_bytes()),
                Part::Literal(bytes) => {
                    pending.extend_from_slice(format!("{{{}}}\r\n", bytes.len()).as_bytes());
                    self.send(&pending)?;
                    pending.clear();
                    // Espera o "+" do servidor antes dos bytes.
                    loop {
                        let response = self.read_response()?;
                        if response.line.starts_with('+') {
                            break;
                        }
                        match tagged(&tag, &response) {
                            Tagged::Other => {
                                self.note_change(&response);
                                untagged.push(response);
                            }
                            Tagged::Done(result) => {
                                result?;
                                return Err(ImapError::Protocol("literal recusado".into()));
                            }
                        }
                    }
                    pending.extend_from_slice(&bytes);
                }
            }
        }
        pending.extend_from_slice(b"\r\n");
        self.send(&pending)?;
        loop {
            let response = self.read_response()?;
            match tagged(&tag, &response) {
                Tagged::Other => {
                    self.note_change(&response);
                    untagged.push(response);
                }
                Tagged::Done(result) => {
                    if result.is_ok() {
                        self.last_ok = response.line;
                    }
                    return result.map(|()| untagged);
                }
            }
        }
    }

    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        let stream = self.io.get_mut();
        stream.write_all(bytes)?;
        stream.flush()
    }

    /// Entra com usuário e senha. Só um NO de verdade vira "senha
    /// recusada": servidor indisponível (`[UNAVAILABLE]`) ou comando
    /// estranho (BAD) contam como falha passageira.
    pub fn login(&mut self, user: &str, secret: &str) -> ImapResult<()> {
        if self.preauth {
            return Ok(());
        }
        let mut parts = vec![Part::Text("LOGIN".into())];
        push_string(&mut parts, user);
        push_string(&mut parts, secret);
        match self.command(parts) {
            Err(ImapError::Rejected(text))
                if !text.to_ascii_uppercase().contains("[UNAVAILABLE]") =>
            {
                Err(ImapError::Auth(text))
            }
            Err(ImapError::Rejected(text)) => Err(ImapError::Protocol(text)),
            other => other.map(|_| ()),
        }
    }

    /// Entra com um token OAuth (SASL XOAUTH2, com a resposta inicial no
    /// próprio comando). Recusa vira "login recusado" (`Auth`), menos
    /// servidor indisponível (`[UNAVAILABLE]`), que é passageiro.
    pub fn authenticate_xoauth2(&mut self, user: &str, token: &str) -> ImapResult<()> {
        if self.preauth {
            return Ok(());
        }
        let bad = |s: &str| s.is_empty() || s.chars().any(|c| c.is_control() || c.is_whitespace());
        if bad(user) || bad(token) {
            return Err(ImapError::Auth(tr("usuário ou token inválido").into()));
        }
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("user={user}\x01auth=Bearer {token}\x01\x01"));
        let tag = self.next_tag();
        self.send(format!("{tag} AUTHENTICATE XOAUTH2 {encoded}\r\n").as_bytes())?;
        loop {
            let response = self.read_response()?;
            // Recusa: o servidor manda os detalhes como desafio e espera uma
            // linha vazia antes do NO.
            if response.line.starts_with('+') {
                self.send(b"\r\n")?;
                continue;
            }
            match tagged(&tag, &response) {
                Tagged::Other => {}
                Tagged::Done(Ok(())) => return Ok(()),
                Tagged::Done(Err(ImapError::Rejected(text)))
                    if text.to_ascii_uppercase().contains("[UNAVAILABLE]") =>
                {
                    return Err(ImapError::Protocol(text))
                }
                Tagged::Done(Err(ImapError::Rejected(text))) => return Err(ImapError::Auth(text)),
                Tagged::Done(Err(err)) => return Err(err),
            }
        }
    }

    pub fn capabilities(&mut self) -> ImapResult<Vec<String>> {
        let responses = self.command(vec![Part::Text("CAPABILITY".into())])?;
        Ok(responses
            .iter()
            .filter_map(|r| {
                let rest = r.line.strip_prefix("* ")?;
                let (word, caps) = rest.split_once(' ')?;
                word.eq_ignore_ascii_case("CAPABILITY").then_some(caps)
            })
            .flat_map(|caps| caps.split_whitespace().map(str::to_ascii_uppercase))
            .collect())
    }

    /// Abre a Caixa de Entrada só para leitura.
    pub fn examine_inbox(&mut self) -> ImapResult<Mailbox> {
        let responses = self.command(vec![Part::Text("EXAMINE INBOX".into())])?;
        let mut mailbox = Mailbox::default();
        for r in &responses {
            let upper = r.line.to_ascii_uppercase();
            if let Some(n) = code_number(&upper, "[UIDVALIDITY ") {
                mailbox.uid_validity = Some(n);
            }
            if let Some(n) = code_number(&upper, "[UIDNEXT ") {
                mailbox.uid_next = Some(n);
            }
        }
        Ok(mailbox)
    }

    /// `UID SEARCH <critério>`: UIDs encontrados.
    pub fn search(&mut self, criteria: &str) -> ImapResult<Vec<u32>> {
        let responses = self.command(vec![Part::Text(format!("UID SEARCH {criteria}"))])?;
        let mut uids = Vec::new();
        for r in &responses {
            let Some(rest) = r.line.strip_prefix("* ") else {
                continue;
            };
            let mut words = rest.split_whitespace();
            if !words
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("SEARCH"))
            {
                continue;
            }
            uids.extend(words.filter_map(|w| w.parse::<u32>().ok()));
        }
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    /// Remetente e assunto, sem marcar como lido.
    pub fn fetch_headers(&mut self, uids: &[u32]) -> ImapResult<Vec<NewMail>> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let set = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let responses = self.command(vec![Part::Text(format!(
            "UID FETCH {set} (UID BODY.PEEK[HEADER.FIELDS (FROM SUBJECT)])"
        ))])?;
        let mut mails: Vec<NewMail> = responses
            .iter()
            .filter(|r| r.line.to_ascii_uppercase().contains(" FETCH "))
            .filter_map(|r| {
                let uid = code_number(&r.line.to_ascii_uppercase(), "UID ")?;
                let (from, subject) = r
                    .literals
                    .first()
                    .map(|raw| parse_headers(raw))
                    .unwrap_or_default();
                Some(NewMail { uid, from, subject })
            })
            .filter(|m| uids.contains(&m.uid))
            .collect();
        mails.sort_by_key(|m| m.uid);
        mails.dedup_by_key(|m| m.uid);
        Ok(mails)
    }

    pub fn noop(&mut self) -> ImapResult<()> {
        self.command(vec![Part::Text("NOOP".into())]).map(|_| ())
    }

    /// Começa a esperar avisos do servidor (RFC 2177).
    pub fn idle_start(&mut self) -> ImapResult<()> {
        let tag = self.next_tag();
        self.send(format!("{tag} IDLE\r\n").as_bytes())?;
        loop {
            let response = self.read_response()?;
            if response.line.starts_with('+') {
                self.idling = Some(tag);
                return Ok(());
            }
            match tagged(&tag, &response) {
                Tagged::Other => self.note_change(&response),
                Tagged::Done(result) => {
                    result?;
                    return Err(ImapError::Protocol("IDLE recusado".into()));
                }
            }
        }
    }

    /// Espera o próximo aviso durante o IDLE: `true` se a caixa mudou
    /// (e-mail novo, apagado ou marcado). Tempo esgotado sai como erro
    /// (`is_timeout`): hora de renovar.
    pub fn idle_wait(&mut self) -> ImapResult<bool> {
        let response = self.read_response()?;
        let upper = response.line.to_ascii_uppercase();
        if upper.starts_with("* BYE") {
            return Err(ImapError::Io(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                clip(&response.line),
            )));
        }
        if let Some(tag) = &self.idling {
            if response.line.starts_with(tag.as_str()) {
                self.idling = None;
                return Err(ImapError::Protocol("IDLE encerrado pelo servidor".into()));
            }
        }
        Ok(is_change(&response.line))
    }

    /// Termina o IDLE e espera a confirmação.
    pub fn idle_done(&mut self) -> ImapResult<()> {
        let Some(tag) = self.idling.take() else {
            return Ok(());
        };
        self.send(b"DONE\r\n")?;
        loop {
            let response = self.read_response()?;
            if let Tagged::Done(result) = tagged(&tag, &response) {
                return result;
            }
        }
    }

    pub fn logout(&mut self) {
        let _ = self.idle_done();
        let _ = self.command(vec![Part::Text("LOGOUT".into())]);
    }
}

/// Lê uma linha byte a byte (nada além dela sai do fluxo: depois do
/// STARTTLS, o que vem é o TLS).
fn read_raw_line<S: Read>(stream: &mut S) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                tr("conexão encerrada"),
            ));
        }
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
        if line.len() > 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                tr("linha longa demais"),
            ));
        }
    }
    if line.ends_with(b"\r") {
        line.pop();
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// Começo de uma conexão comum: saudação e pedido de STARTTLS. Depois de
/// `Ok`, o chamador faz o aperto de mão TLS no mesmo fluxo e segue com
/// `Imap::after_starttls`. Sem STARTTLS aceito, falha: a senha nunca vai
/// sem criptografia.
pub fn starttls_preamble<S: Read + Write>(stream: &mut S) -> ImapResult<()> {
    let greeting = read_raw_line(stream)?;
    if !greeting.to_ascii_uppercase().starts_with("* OK") {
        return Err(ImapError::Protocol(clip(&greeting)));
    }
    stream.write_all(b"s0 STARTTLS\r\n")?;
    stream.flush()?;
    loop {
        let line = read_raw_line(stream)?;
        let Some(rest) = line.strip_prefix("s0 ") else {
            if line.to_ascii_uppercase().starts_with("* BYE") {
                return Err(ImapError::Protocol(clip(&line)));
            }
            continue;
        };
        return if rest.to_ascii_uppercase().starts_with("OK") {
            Ok(())
        } else {
            Err(ImapError::Protocol(format!(
                "STARTTLS recusado ({})",
                clip(rest)
            )))
        };
    }
}

enum Tagged {
    /// Resposta sem tag (ou de outro comando).
    Other,
    /// Resposta final do comando: OK, ou o erro (NO/BAD, ou BYE do servidor).
    Done(ImapResult<()>),
}

/// Classifica uma resposta em relação ao comando `tag`.
fn tagged(tag: &str, response: &Response) -> Tagged {
    let line = &response.line;
    if line.starts_with("* ") {
        if line.to_ascii_uppercase().starts_with("* BYE") {
            return Tagged::Done(Err(ImapError::Io(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                clip(line),
            ))));
        }
        return Tagged::Other;
    }
    let Some(rest) = line.strip_prefix(tag).and_then(|r| r.strip_prefix(' ')) else {
        return Tagged::Other;
    };
    let (status, text) = rest.split_once(' ').unwrap_or((rest, ""));
    if status.eq_ignore_ascii_case("OK") {
        Tagged::Done(Ok(()))
    } else if status.eq_ignore_ascii_case("NO") {
        Tagged::Done(Err(ImapError::Rejected(clip(text))))
    } else {
        Tagged::Done(Err(ImapError::Protocol(clip(text))))
    }
}

/// Aviso sem tag de mudança na caixa: e-mail novo, apagado ou marcado.
fn is_change(line: &str) -> bool {
    let upper = line.to_ascii_uppercase();
    upper.starts_with("* ")
        && [" EXISTS", " EXPUNGE", " FETCH", " RECENT"]
            .iter()
            .any(|w| upper.contains(w))
}

/// Tamanho do literal anunciado no fim da linha: `{123}` ou `{123+}`.
fn literal_size(line: &[u8]) -> Option<usize> {
    let line = line.strip_suffix(b"}")?;
    let start = line.iter().rposition(|&b| b == b'{')?;
    let digits = &line[start + 1..];
    let digits = digits.strip_suffix(b"+").unwrap_or(digits);
    if digits.is_empty() || digits.len() > 9 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

/// Número logo depois de `key` (ex.: `[UIDNEXT 42]`, `UID 7 `).
pub(crate) fn code_number(upper: &str, key: &str) -> Option<u32> {
    let start = upper.find(key)? + key.len();
    let digits: String = upper[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

pub(crate) fn clip(text: &str) -> String {
    crate::clip(text.trim(), 200)
}

// ---------------------------------------------------------------------------
// Vigia

/// Quem recebe o que o vigia descobre.
pub trait Sink {
    /// Conectado e com a caixa aberta.
    fn connected(&mut self);
    /// Não lidas da Caixa de Entrada (no Gmail, da aba Principal).
    fn unread(&mut self, count: u32);
    /// E-mails novos e ainda não lidos, mais antigo primeiro.
    fn new_mail(&mut self, mails: Vec<NewMail>);
    /// Pedido para parar (conta removida, configuração trocada, app saindo).
    fn stopped(&self) -> bool;
    /// Espera até `timeout` ou um pedido para parar/acordar; `true` = parar.
    /// Usado só por servidor sem IDLE.
    fn wait(&mut self, timeout: Duration) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct WatchTimes {
    /// Espera por resposta de comando.
    pub command: Duration,
    /// Renova o IDLE depois disto (e confere se a conexão ainda vive).
    pub renew: Duration,
    /// Intervalo de consulta quando o servidor não tem IDLE.
    pub poll: Duration,
}

impl Default for WatchTimes {
    fn default() -> Self {
        Self {
            command: Duration::from_secs(60),
            // Bem abaixo dos 29 min da norma: roteadores e redes corporativas
            // costumam derrubar conexões paradas antes disso.
            renew: Duration::from_secs(5 * 60),
            poll: Duration::from_secs(2 * 60),
        }
    }
}

/// Critério das não lidas: no Gmail com a extensão própria, só a aba
/// Principal (o número que o site mostra); nos outros, toda a caixa.
fn unread_criteria(gmail: bool) -> &'static str {
    if gmail {
        "X-GM-RAW \"category:primary is:unread\""
    } else {
        "UNSEEN"
    }
}

/// Até onde o vigia já avisou, guardado entre conexões: e-mail que chega
/// durante uma reconexão ainda é avisado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub uid_validity: u32,
    pub last_uid: u32,
}

/// Último UID já visto ao abrir a caixa: o da conexão anterior, se a caixa
/// é a mesma (mesmo UIDVALIDITY); senão, o maior atual (só avisa o que
/// chegar daqui em diante).
fn resume_point(mailbox: Mailbox, cursor: Option<Cursor>) -> Option<u32> {
    let (cursor, validity) = (cursor?, mailbox.uid_validity?);
    (cursor.uid_validity == validity).then_some(cursor.last_uid)
}

/// Vigia a Caixa de Entrada de uma sessão já autenticada até `sink`
/// pedir para parar (`Ok`) ou a conexão falhar (`Err`). `cursor` passa de
/// uma conexão para a seguinte.
pub fn watch<S: Transport>(
    imap: &mut Imap<S>,
    gmail: bool,
    times: WatchTimes,
    cursor: &mut Option<Cursor>,
    sink: &mut dyn Sink,
) -> ImapResult<()> {
    imap.set_timeout(times.command)?;
    let caps = imap.capabilities()?;
    let has_idle = caps.iter().any(|c| c == "IDLE");
    let gmail = gmail && caps.iter().any(|c| c == "X-GM-EXT-1");
    let criteria = unread_criteria(gmail);
    let mailbox = imap.examine_inbox()?;
    // O EXISTS da abertura não é mudança.
    imap.take_changed();
    let mut last_uid = match resume_point(mailbox, *cursor) {
        Some(uid) => uid,
        None => match mailbox.uid_next {
            Some(next) => next.saturating_sub(1),
            None => imap.search("UID *")?.last().copied().unwrap_or(0),
        },
    };
    let mut first = true;
    // Uma consulta extra seguida por aviso recebido fora do IDLE, no máximo
    // (servidor que repete o aviso a cada comando não prende o laço).
    let mut recheck = false;
    loop {
        // Confere a cada volta, mesmo na renovação sem aviso: há servidor
        // que não avisa marcações feitas em outro lugar.
        let unread = imap.search(criteria)?;
        sink.unread(u32::try_from(unread.len()).unwrap_or(u32::MAX));
        // "n:*" sempre traz o maior UID, mesmo se menor que n.
        let arrived: Vec<u32> = imap
            .search(&format!("UID {}:*", last_uid.saturating_add(1)))?
            .into_iter()
            .filter(|&uid| uid > last_uid)
            .collect();
        if let Some(&max) = arrived.last() {
            last_uid = max;
            let mut fresh: Vec<u32> = arrived
                .into_iter()
                .filter(|uid| unread.binary_search(uid).is_ok())
                .collect();
            if fresh.len() > NOTIFY_MAX {
                fresh.drain(..fresh.len() - NOTIFY_MAX);
            }
            let mails = imap.fetch_headers(&fresh)?;
            if !mails.is_empty() {
                sink.new_mail(mails);
            }
        }
        if let Some(validity) = mailbox.uid_validity {
            *cursor = Some(Cursor {
                uid_validity: validity,
                last_uid,
            });
        }
        // Conectado só depois do primeiro número: a tela não pisca em zero.
        if std::mem::take(&mut first) {
            sink.connected();
        }
        if sink.stopped() {
            imap.logout();
            return Ok(());
        }
        let pending = imap.take_changed() && !recheck;
        recheck = pending;
        if pending {
            continue;
        }
        if has_idle {
            imap.idle_start()?;
            // Aviso chegado entre a consulta e o começo do IDLE.
            if !imap.take_changed() {
                imap.set_timeout(times.renew)?;
                // Até um aviso de mudança ou a hora de renovar.
                loop {
                    match imap.idle_wait() {
                        Ok(true) => break,
                        Ok(false) => {}
                        Err(err) if err.is_timeout() => break,
                        Err(err) => return Err(err),
                    }
                }
                imap.set_timeout(times.command)?;
            }
            imap.idle_done()?;
        } else {
            if sink.wait(times.poll) {
                imap.logout();
                return Ok(());
            }
            imap.noop()?;
        }
        if sink.stopped() {
            imap.logout();
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn signature_is_cleaned_and_optional() {
        // Conta gravada antes da assinatura: lê vazia e não grava o campo.
        let old: MailAccount =
            serde_json::from_str(r#"{"host":"imap.x.com","user":"a@x.com"}"#).unwrap();
        assert_eq!(old.signature, "");
        assert!(!serde_json::to_string(&old).unwrap().contains("signature"));
        assert_eq!(
            normalize_signature("  Ana\r\nPapeo\u{7}\t \n"),
            "Ana\nPapeo"
        );
        let long = "a".repeat(SIGNATURE_MAX + 50);
        assert_eq!(normalize_signature(&long).chars().count(), SIGNATURE_MAX);
        let mut a = old.clone();
        a.signature = " Ana ".into();
        assert_eq!(a.sanitized().unwrap().signature, "Ana");
    }

    #[test]
    fn account_validation() {
        let account = MailAccount {
            host: "  IMAP.Gmail.com. ".into(),
            port: 993,
            user: " ana@gmail.com ".into(),
            sleep_site: true,
            starttls: false,
            whole_inbox: false,
            auth: MailAuth::Password,
            signature: String::new(),
            signature_html: Default::default(),
        }
        .sanitized()
        .unwrap();
        assert_eq!(account.host, "imap.gmail.com");
        assert_eq!(account.user, "ana@gmail.com");
        assert!(account.is_gmail());
        assert_eq!(
            account.login_secret("abcd efgh ijkl mnop"),
            "abcdefghijklmnop"
        );
        let other = MailAccount {
            host: "mail.exemplo.com.br".into(),
            ..account.clone()
        };
        assert_eq!(other.login_secret("a b"), "a b");
        for bad in [
            "",
            "imap exemplo.com",
            "-x.com",
            "a..b",
            "imap.exemplo.com/x",
            "ex:993",
        ] {
            let mut a = account.clone();
            a.host = bad.into();
            assert!(a.sanitized().is_err(), "{bad}");
        }
        let mut a = account.clone();
        a.user = "  ".into();
        assert!(a.sanitized().is_err());
        let mut a = account.clone();
        a.port = 0;
        assert!(a.sanitized().is_err());
        let mut a = account.clone();
        a.starttls = true;
        assert!(a.clone().sanitized().is_err(), "STARTTLS na 993");
        a.port = IMAP_PORT;
        assert!(a.sanitized().is_ok());
        // Registro antigo/sem campos opcionais.
        let a: MailAccount = serde_json::from_str(r#"{"host":"x.com","user":"u"}"#).unwrap();
        assert_eq!(a.port, IMAPS_PORT);
        assert!(a.sleep_site);
        assert!(check_secret("").is_err());
        assert!(check_secret("a\nb").is_err());
        assert!(check_secret("senha com espaço é ok").is_ok());
    }

    #[test]
    fn headers_are_decoded() {
        let raw = b"From: =?UTF-8?Q?Jo=C3=A3o_Silva?= <joao@x.com>\r\nSubject: =?utf-8?B?UmV1bmnDo28gZGU=?=\r\n =?utf-8?B?IGhvamU=?=\r\n\r\n";
        assert_eq!(
            parse_headers(raw),
            ("João Silva".into(), "Reunião de hoje".into())
        );
        let raw = b"subject: Pedido   n\xba 5\r\nfrom: \"Loja, Ltda\" <a@b.c>\r\n";
        assert_eq!(
            parse_headers(raw),
            ("Loja, Ltda".into(), "Pedido nº 5".into())
        );
        let raw = b"From: ana@x.com\r\nSubject: =?ISO-8859-1?Q?Promo=E7=E3o?= agora\r\n";
        assert_eq!(
            parse_headers(raw),
            ("ana@x.com".into(), "Promoção agora".into())
        );
        let raw = b"From: <bia@x.com>\r\nSubject: =?bogus?X?abc?= ok\r\n";
        assert_eq!(
            parse_headers(raw),
            ("bia@x.com".into(), "=?bogus?X?abc?= ok".into())
        );
        assert_eq!(decode_words("=?UTF-8*pt?Q?ol=C3=A1?= mundo"), "olá mundo");
        assert_eq!(parse_headers(b""), (String::new(), String::new()));
    }

    #[test]
    fn resumes_only_in_the_same_mailbox() {
        let cursor = Some(Cursor {
            uid_validity: 7,
            last_uid: 40,
        });
        let same = Mailbox {
            uid_validity: Some(7),
            uid_next: Some(45),
        };
        assert_eq!(resume_point(same, cursor), Some(40));
        let recreated = Mailbox {
            uid_validity: Some(8),
            ..same
        };
        assert_eq!(resume_point(recreated, cursor), None);
        assert_eq!(resume_point(same, None), None);
        let unknown = Mailbox {
            uid_validity: None,
            ..same
        };
        assert_eq!(resume_point(unknown, cursor), None);
    }

    #[test]
    fn login_kinds_of_refusal() {
        let (stream, server) = fake_server(|r, w| {
            let tag = expect(r, "LOGIN");
            w.write_all(format!("{tag} NO [UNAVAILABLE] tente depois\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "LOGIN");
            w.write_all(format!("{tag} BAD comando estranho\r\n").as_bytes())
                .unwrap();
        });
        let mut imap = Imap::connect(stream).unwrap();
        assert!(matches!(imap.login("a", "b"), Err(ImapError::Protocol(_))));
        assert!(matches!(imap.login("a", "b"), Err(ImapError::Protocol(_))));
        server.join().unwrap();
    }

    #[test]
    fn gmail_counter_choice_and_old_records() {
        let mut a: MailAccount =
            serde_json::from_str(r#"{"host":"imap.gmail.com","user":"u"}"#).unwrap();
        assert!(!a.starttls && !a.whole_inbox && a.primary_only());
        a.whole_inbox = true;
        assert!(!a.primary_only());
        let other: MailAccount = serde_json::from_str(r#"{"host":"x.com","user":"u"}"#).unwrap();
        assert!(!other.primary_only());
        // Opções desligadas não aparecem no arquivo.
        let json = serde_json::to_string(&other).unwrap();
        assert!(!json.contains("starttls") && !json.contains("wholeInbox"));
    }

    #[test]
    fn starttls_is_required() {
        let (mut stream, server) = fake_server(|r, w| {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert_eq!(line, "s0 STARTTLS\r\n");
            w.write_all(b"* OK ainda conversando\r\ns0 OK Begin TLS\r\nTLS")
                .unwrap();
        });
        starttls_preamble(&mut stream).unwrap();
        // O que vem depois do OK fica no fluxo para o TLS.
        let mut rest = [0u8; 3];
        std::io::Read::read_exact(&mut stream, &mut rest).unwrap();
        assert_eq!(&rest, b"TLS");
        server.join().unwrap();

        let (mut stream, server) = fake_server(|r, w| {
            let tag = expect(r, "STARTTLS");
            w.write_all(format!("{tag} BAD sem TLS aqui\r\n").as_bytes())
                .unwrap();
        });
        match starttls_preamble(&mut stream) {
            Err(ImapError::Protocol(text)) => assert!(text.contains("STARTTLS")),
            other => panic!("{other:?}"),
        }
        server.join().unwrap();
    }

    #[test]
    fn microsoft_login_only_for_outlook_server() {
        let mut a: MailAccount = serde_json::from_str(
            r#"{"host":"outlook.office365.com","user":"ana@outlook.com","auth":"microsoft"}"#,
        )
        .unwrap();
        assert_eq!(a.auth, MailAuth::Microsoft);
        assert!(a.clone().sanitized().is_ok());
        a.host = "imap.outro.com".into();
        assert!(a.sanitized().is_err(), "token não vai para outro servidor");
        // Senha (padrão) não aparece no arquivo.
        let p: MailAccount = serde_json::from_str(r#"{"host":"x.com","user":"u"}"#).unwrap();
        assert_eq!(p.auth, MailAuth::Password);
        assert!(!serde_json::to_string(&p).unwrap().contains("auth"));
    }

    #[test]
    fn xoauth2_login_and_refusal() {
        let (stream, server) = fake_server(|r, w| {
            let tag = expect(r, "AUTHENTICATE XOAUTH2 ");
            w.write_all(format!("{tag} OK autenticado\r\n").as_bytes())
                .unwrap();
            // Recusa: desafio com detalhes, linha vazia do cliente, NO.
            let tag = expect(r, "AUTHENTICATE XOAUTH2 ");
            w.write_all(b"+ eyJzdGF0dXMiOiI0MDEifQ==\r\n").unwrap();
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert_eq!(line, "\r\n");
            w.write_all(format!("{tag} NO AUTHENTICATE failed.\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "AUTHENTICATE XOAUTH2 ");
            w.write_all(format!("{tag} NO [UNAVAILABLE] tente depois\r\n").as_bytes())
                .unwrap();
        });
        let mut imap = Imap::connect(stream).unwrap();
        imap.authenticate_xoauth2("ana@outlook.com", "tok.en-1")
            .unwrap();
        assert!(matches!(
            imap.authenticate_xoauth2("ana@outlook.com", "velho"),
            Err(ImapError::Auth(_))
        ));
        assert!(matches!(
            imap.authenticate_xoauth2("ana@outlook.com", "tok"),
            Err(ImapError::Protocol(_))
        ));
        assert!(matches!(
            imap.authenticate_xoauth2("ana@outlook.com", "com espaço"),
            Err(ImapError::Auth(_))
        ));
        server.join().unwrap();
    }

    #[test]
    fn smtp_follows_the_reading_server() {
        let account = |host: &str, user: &str| MailAccount {
            host: host.into(),
            port: IMAPS_PORT,
            user: user.into(),
            sleep_site: false,
            starttls: false,
            whole_inbox: false,
            auth: MailAuth::Password,
            signature: String::new(),
            signature_html: Default::default(),
        };
        let s = account("imap.gmail.com", "a@gmail.com").smtp();
        assert_eq!(
            (s.host.as_str(), s.port, s.starttls),
            ("smtp.gmail.com", 465, false)
        );
        let s = account(OUTLOOK_HOST, "a@hotmail.com").smtp();
        assert_eq!(
            (s.host.as_str(), s.port, s.starttls),
            ("smtp-mail.outlook.com", 587, true)
        );
        let s = account(OUTLOOK_HOST, "a@empresa.com.br").smtp();
        assert_eq!(s.host, "smtp.office365.com");
        assert_eq!(
            account("imap.hostinger.com", "a@x.com").smtp().host,
            "smtp.hostinger.com"
        );
        assert_eq!(
            account("imap.uol.com.br", "a@uol.com.br").smtp().host,
            "smtps.uol.com.br"
        );
        assert_eq!(
            account("imap.exemplo.com", "a@x.com").smtp().host,
            "smtp.exemplo.com"
        );
        assert_eq!(
            account("mail.exemplo.com", "a@x.com").smtp().host,
            "mail.exemplo.com"
        );
        assert!(account("imap.gmail.com", "a@gmail.com").saves_sent());
        assert!(!account("imap.hostinger.com", "a@x.com").saves_sent());
    }

    #[test]
    fn literal_sizes() {
        assert_eq!(literal_size(b"* 1 FETCH (BODY[] {12}"), Some(12));
        assert_eq!(literal_size(b"a LOGIN {3+}"), Some(3));
        assert_eq!(literal_size(b"* OK {abc}"), None);
        assert_eq!(literal_size(b"* OK done"), None);
    }

    /// Servidor IMAP falso: responde a cada comando conforme `script`.
    /// Devolve as linhas recebidas.
    fn fake_server(
        script: impl FnOnce(&mut BufReader<TcpStream>, &mut TcpStream) + Send + 'static,
    ) -> (TcpStream, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            out.write_all(b"* OK IMAP pronto\r\n").unwrap();
            script(&mut reader, &mut out);
        });
        (TcpStream::connect(addr).unwrap(), handle)
    }

    fn expect(reader: &mut BufReader<TcpStream>, start: &str) -> String {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let mut parts = line.splitn(2, ' ');
        let tag = parts.next().unwrap().to_owned();
        let rest = parts.next().unwrap_or("");
        assert!(rest.starts_with(start), "esperava {start:?}, veio {line:?}");
        tag
    }

    #[test]
    fn login_quotes_and_literals() {
        let (stream, server) = fake_server(|r, w| {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert_eq!(line, "p1 LOGIN \"ana@x.com\" {7}\r\n");
            w.write_all(b"+ pode mandar\r\n").unwrap();
            let mut secret = [0u8; 7];
            std::io::Read::read_exact(r, &mut secret).unwrap();
            assert_eq!(&secret, "sen\"há".as_bytes());
            line.clear();
            r.read_line(&mut line).unwrap();
            assert_eq!(line, "\r\n");
            w.write_all(b"p1 OK logado\r\n").unwrap();
            let tag = expect(r, "LOGIN \"b\\\\o\" \"x\\\"y\"");
            w.write_all(
                format!("{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n").as_bytes(),
            )
            .unwrap();
        });
        let mut imap = Imap::connect(stream).unwrap();
        imap.login("ana@x.com", "sen\"há").unwrap();
        match imap.login("b\\o", "x\"y") {
            Err(ImapError::Auth(text)) => assert!(text.contains("Invalid credentials")),
            other => panic!("{other:?}"),
        }
        server.join().unwrap();
    }

    struct Recorder {
        unread: Vec<u32>,
        mails: Vec<NewMail>,
        connected: bool,
        stop_after_mails: bool,
    }

    impl Sink for Recorder {
        fn connected(&mut self) {
            self.connected = true;
        }
        fn unread(&mut self, count: u32) {
            self.unread.push(count);
        }
        fn new_mail(&mut self, mails: Vec<NewMail>) {
            self.mails.extend(mails);
        }
        fn stopped(&self) -> bool {
            self.stop_after_mails && !self.mails.is_empty()
        }
        fn wait(&mut self, _timeout: Duration) -> bool {
            false
        }
    }

    #[test]
    fn watch_counts_and_announces_new_mail() {
        let (stream, server) = fake_server(|r, w| {
            let tag = expect(r, "CAPABILITY");
            w.write_all(format!("* CAPABILITY IMAP4rev1 IDLE\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "EXAMINE INBOX");
            w.write_all(
                format!("* 3 EXISTS\r\n* OK [UIDVALIDITY 7] ok\r\n* OK [UIDNEXT 11] ok\r\n{tag} OK [READ-ONLY]\r\n")
                    .as_bytes(),
            )
            .unwrap();
            let tag = expect(r, "UID SEARCH UNSEEN");
            w.write_all(format!("* SEARCH 4 9\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH UID 11:*");
            w.write_all(format!("* SEARCH 10\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // Primeiro IDLE: o tempo esgota sem aviso (renovação).
            let tag = expect(r, "IDLE");
            w.write_all(b"+ idling\r\n").unwrap();
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert_eq!(line, "DONE\r\n");
            w.write_all(format!("{tag} OK IDLE terminated\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH UNSEEN");
            w.write_all(format!("* SEARCH 4 9\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH UID 11:*");
            // Nada novo: o servidor devolve o maior UID existente.
            w.write_all(format!("* SEARCH 10\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // Segundo IDLE: chegam dois e-mails, um já lido no celular.
            let tag = expect(r, "IDLE");
            w.write_all(b"+ idling\r\n* 5 EXISTS\r\n").unwrap();
            line.clear();
            r.read_line(&mut line).unwrap();
            assert_eq!(line, "DONE\r\n");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
            let tag = expect(r, "UID SEARCH UNSEEN");
            w.write_all(format!("* SEARCH 4 9 12\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH UID 11:*");
            w.write_all(format!("* SEARCH 11 12\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(
                r,
                "UID FETCH 12 (UID BODY.PEEK[HEADER.FIELDS (FROM SUBJECT)])",
            );
            let headers = "From: =?UTF-8?Q?Jo=C3=A3o?= <j@x.com>\r\nSubject: Ol\u{e1}\r\n\r\n";
            w.write_all(
                format!(
                    "* 5 FETCH (UID 12 BODY[HEADER.FIELDS (FROM SUBJECT)] {{{}}}\r\n{headers})\r\n{tag} OK\r\n",
                    headers.len()
                )
                .as_bytes(),
            )
            .unwrap();
            let tag = expect(r, "LOGOUT");
            w.write_all(format!("* BYE\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
        });
        let mut imap = Imap::connect(stream).unwrap();
        let mut sink = Recorder {
            unread: Vec::new(),
            mails: Vec::new(),
            connected: false,
            stop_after_mails: true,
        };
        let times = WatchTimes {
            command: Duration::from_secs(5),
            renew: Duration::from_millis(300),
            poll: Duration::from_secs(1),
        };
        let mut cursor = None;
        watch(&mut imap, false, times, &mut cursor, &mut sink).unwrap();
        server.join().unwrap();
        assert!(sink.connected);
        assert_eq!(
            cursor,
            Some(Cursor {
                uid_validity: 7,
                last_uid: 12
            })
        );
        assert_eq!(sink.unread, [2, 2, 3]);
        assert_eq!(
            sink.mails,
            [NewMail {
                uid: 12,
                from: "João".into(),
                subject: "Olá".into()
            }]
        );
    }

    #[test]
    fn change_seen_outside_idle_is_checked_at_once() {
        let (stream, server) = fake_server(|r, w| {
            let tag = expect(r, "CAPABILITY");
            w.write_all(format!("* CAPABILITY IMAP4rev1 IDLE\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "EXAMINE INBOX");
            w.write_all(
                format!(
                    "* 2 EXISTS\r\n* OK [UIDVALIDITY 1] ok\r\n* OK [UIDNEXT 3] ok\r\n{tag} OK\r\n"
                )
                .as_bytes(),
            )
            .unwrap();
            // Um e-mail chega no meio da primeira consulta.
            let tag = expect(r, "UID SEARCH UNSEEN");
            w.write_all(format!("* 3 EXISTS\r\n* SEARCH\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH UID 3:*");
            w.write_all(format!("* SEARCH 2\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // Sem IDLE no meio: consulta de novo na hora.
            let tag = expect(r, "UID SEARCH UNSEEN");
            w.write_all(format!("* SEARCH 3\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH UID 3:*");
            w.write_all(format!("* SEARCH 3\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID FETCH 3 ");
            let headers = "From: Ana <a@x.com>\r\nSubject: Oi\r\n\r\n";
            w.write_all(
                format!(
                    "* 3 FETCH (UID 3 BODY[HEADER.FIELDS (FROM SUBJECT)] {{{}}}\r\n{headers})\r\n{tag} OK\r\n",
                    headers.len()
                )
                .as_bytes(),
            )
            .unwrap();
            let tag = expect(r, "LOGOUT");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
        });
        let mut imap = Imap::connect(stream).unwrap();
        let mut sink = Recorder {
            unread: Vec::new(),
            mails: Vec::new(),
            connected: false,
            stop_after_mails: true,
        };
        let times = WatchTimes {
            command: Duration::from_secs(5),
            renew: Duration::from_secs(30),
            poll: Duration::from_secs(1),
        };
        watch(&mut imap, false, times, &mut None, &mut sink).unwrap();
        server.join().unwrap();
        assert_eq!(sink.unread, [0, 1]);
        assert_eq!(sink.mails.len(), 1);
        assert_eq!(sink.mails[0].from, "Ana");
    }

    #[test]
    fn gmail_counts_primary_only() {
        let (stream, server) = fake_server(|r, w| {
            let tag = expect(r, "CAPABILITY");
            w.write_all(format!("* CAPABILITY IMAP4rev1 X-GM-EXT-1\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "EXAMINE INBOX");
            w.write_all(format!("* OK [UIDNEXT 5] ok\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH X-GM-RAW \"category:primary is:unread\"");
            w.write_all(format!("* SEARCH 2\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            let tag = expect(r, "UID SEARCH UID 5:*");
            w.write_all(format!("* SEARCH 4\r\n{tag} OK\r\n").as_bytes())
                .unwrap();
            // Sem IDLE: consulta com NOOP.
            let tag = expect(r, "NOOP");
            w.write_all(format!("{tag} OK\r\n").as_bytes()).unwrap();
        });
        let mut imap = Imap::connect(stream).unwrap();
        struct Once(Vec<u32>, u8);
        impl Sink for Once {
            fn connected(&mut self) {}
            fn unread(&mut self, count: u32) {
                self.0.push(count);
            }
            fn new_mail(&mut self, _: Vec<NewMail>) {}
            fn stopped(&self) -> bool {
                false
            }
            fn wait(&mut self, _: Duration) -> bool {
                self.1 += 1;
                self.1 > 1
            }
        }
        let mut sink = Once(Vec::new(), 0);
        // O servidor some depois do NOOP: a busca seguinte falha na rede.
        let result = watch(&mut imap, true, WatchTimes::default(), &mut None, &mut sink);
        server.join().unwrap();
        assert!(matches!(result, Err(ImapError::Io(_))), "{result:?}");
        assert_eq!(sink.0, [1]);
    }
}
