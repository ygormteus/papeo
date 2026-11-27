//! Leitor de e-mail do Papeo: uma conexão IMAP por conta (separada da do
//! vigia de avisos), aberta sob demanda e fechada depois de um tempo parada,
//! e o envio por SMTP. Os comandos de leitura ficam em
//! `papeo_core::mailbox`; conexão e login, em `crate::mail`.
//!
//! Locks: o do estado só é pego por um instante para ler a conta, nunca
//! junto do de uma conexão em uso.

use crate::mail::{self, Access, Control, Tls};
use crate::secrets::{self, Saved};
use crate::state::AppState;
use papeo_core::mail::{ImapError, MailAccount, MailAuth};
use papeo_core::mailbox::{self as core, Folder, MessageView, Page, Reader, Role};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Conexão parada há este tempo fecha.
const IDLE_CLOSE: Duration = Duration::from_secs(5 * 60);
/// Conexão parada há este tempo é testada (NOOP) antes de usar.
const IDLE_CHECK: Duration = Duration::from_secs(30);
/// Soma dos anexos de um e-mail enviado.
const ATTACHMENTS_MAX: usize = 25 * 1024 * 1024;
const RECIPIENTS_MAX: usize = 100;
const BODY_MAX: usize = 2 * 1024 * 1024;

struct Conn {
    reader: Reader<Tls>,
    /// Conta para a qual a conexão foi aberta: mudou, abre outra.
    account: MailAccount,
    used: Instant,
}

#[derive(Default)]
struct Slot {
    conn: Option<Conn>,
    /// Última mensagem baixada inteira (pasta, UID, bytes): salvar um anexo
    /// dela não baixa de novo.
    last: Option<(String, u32, Arc<Vec<u8>>)>,
    /// Token de acesso da Microsoft para ler e para enviar.
    read: Access,
    send: Access,
}

#[derive(Default)]
pub struct MailReaders {
    slots: Mutex<HashMap<String, Arc<Mutex<Slot>>>>,
}

pub fn init(app: &AppHandle) {
    app.manage(MailReaders::default());
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        close_idle(&app);
    });
}

/// Fecha as conexões paradas (as em uso ficam).
fn close_idle(app: &AppHandle) {
    let slots: Vec<(String, Arc<Mutex<Slot>>)> = {
        let readers = app.state::<MailReaders>();
        let map = readers.slots.lock().unwrap_or_else(|e| e.into_inner());
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    let accounts: Vec<String> = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        inner
            .config
            .services
            .iter()
            .filter(|s| s.mail.is_some())
            .map(|s| s.id.clone())
            .collect()
    };
    for (id, slot) in slots {
        let Ok(mut slot) = slot.try_lock() else {
            continue;
        };
        let gone = !accounts.contains(&id);
        if slot
            .conn
            .as_ref()
            .is_some_and(|c| gone || c.used.elapsed() >= IDLE_CLOSE)
        {
            if let Some(mut conn) = slot.conn.take() {
                conn.reader.logout();
            }
        }
        if gone {
            *slot = Slot::default();
        }
    }
}

/// Conta e senha guardada de `id`.
fn account_of(app: &AppHandle, id: &str) -> Result<(MailAccount, Saved), String> {
    let account = app
        .state::<AppState>()
        .lock()
        .service(id)?
        .mail
        .clone()
        .ok_or("Esta conta não tem o e-mail por IMAP configurado.")?;
    // Fora do lock: arquivo ou cofre do sistema.
    let saved = secrets::read(app, id)
        .filter(|s| s.matches(&account))
        .ok_or("Senha não salva: configure o IMAP da conta de novo.")?;
    Ok((account, saved))
}

/// Roda `f` com a conexão de leitura da conta (abrindo ou reabrindo).
fn with_reader<T>(
    app: &AppHandle,
    id: &str,
    f: impl FnOnce(&mut Reader<Tls>, &MailAccount) -> Result<T, ImapError>,
) -> Result<T, String> {
    with_reader_cached(app, id, |r, account, _| f(r, account))
}

/// Como `with_reader`, com acesso à última mensagem baixada.
fn with_reader_cached<T>(
    app: &AppHandle,
    id: &str,
    f: impl FnOnce(
        &mut Reader<Tls>,
        &MailAccount,
        &mut Option<(String, u32, Arc<Vec<u8>>)>,
    ) -> Result<T, ImapError>,
) -> Result<T, String> {
    let (account, mut saved) = account_of(app, id)?;
    let slot = {
        let readers = app.state::<MailReaders>();
        let mut map = readers.slots.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(id.to_owned()).or_default().clone()
    };
    let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
    // Conta trocada ou conexão parada há tempo e morta: abre outra.
    let stale = match slot.conn.as_mut() {
        Some(conn) if conn.account != account => true,
        Some(conn) if conn.used.elapsed() >= IDLE_CHECK => conn.reader.noop().is_err(),
        Some(_) => false,
        None => true,
    };
    if stale {
        if let Some(mut conn) = slot.conn.take() {
            if conn.account != account {
                conn.reader.logout();
                // Outra conta (novo login): os tokens guardados não valem.
                slot.read = None;
                slot.send = None;
                slot.last = None;
            }
        }
        let control = Control::detached();
        let microsoft = account.auth == MailAuth::Microsoft;
        let imap = mail::open(
            app,
            id,
            account.starttls,
            microsoft,
            &mut saved,
            &mut slot.read,
            &control,
        )
        .map_err(|e| describe(&e))?;
        let reader = Reader::new(imap, account.is_gmail()).map_err(|e| describe(&e))?;
        slot.conn = Some(Conn {
            reader,
            account: account.clone(),
            used: Instant::now(),
        });
    }
    let Slot { conn, last, .. } = &mut *slot;
    let conn = conn.as_mut().expect("conexão aberta acima");
    let result = f(&mut conn.reader, &account, last);
    conn.used = Instant::now();
    if let Err(ImapError::Io(_)) = &result {
        slot.conn = None;
    }
    result.map_err(|e| describe(&e))
}

fn describe(err: &ImapError) -> String {
    match err {
        ImapError::Auth(_) => "O servidor recusou o login. Confira a senha da conta.".into(),
        other => mail::describe(other),
    }
}

/// Roda fora da thread dos comandos (a rede pode demorar).
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

pub async fn folders(app: AppHandle, id: String, refresh: bool) -> Result<Vec<Folder>, String> {
    blocking(move || with_reader(&app, &id, |r, _| r.folders(refresh))).await
}

pub async fn list(
    app: AppHandle,
    id: String,
    folder: String,
    page: u32,
    query: String,
) -> Result<Page, String> {
    blocking(move || with_reader(&app, &id, |r, _| r.list(&folder, page, &query))).await
}

/// Abre a mensagem e a marca como lida (`peek`: só baixa, para adiantar a
/// próxima da lista).
pub async fn open(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
    peek: bool,
) -> Result<MessageView, String> {
    blocking(move || {
        with_reader_cached(&app, &id, |r, _, last| {
            let (raw, seen, flagged) = r.fetch_raw(&folder, uid, !peek)?;
            let mut view = core::message_view(uid, &raw);
            view.seen = seen || !peek;
            view.flagged = flagged;
            *last = Some((folder, uid, Arc::new(raw)));
            Ok(view)
        })
    })
    .await
}

pub async fn set_flag(
    app: AppHandle,
    id: String,
    folder: String,
    uids: Vec<u32>,
    flag: String,
    on: bool,
) -> Result<(), String> {
    let flag = match flag.as_str() {
        "seen" => "\\Seen",
        "flagged" => "\\Flagged",
        _ => return Err("Marcação inválida.".into()),
    };
    blocking(move || with_reader(&app, &id, |r, _| r.set_flags(&folder, &uids, &[flag], on))).await
}

/// Move para uma pasta (`target`) ou para a de um papel (`archive`, `junk`,
/// `inbox`).
pub async fn move_to(
    app: AppHandle,
    id: String,
    folder: String,
    uids: Vec<u32>,
    target: String,
) -> Result<(), String> {
    blocking(move || {
        with_reader(&app, &id, |r, _| {
            let role = match target.as_str() {
                "role:archive" => Some(Role::Archive),
                "role:junk" => Some(Role::Junk),
                "role:inbox" => Some(Role::Inbox),
                _ => None,
            };
            let target = match role {
                Some(Role::Archive) => match r.role_path(Role::Archive)? {
                    Some(path) => path,
                    // Gmail: arquivar é tirar da Caixa de Entrada.
                    None => r.role_path(Role::All)?.ok_or_else(|| {
                        ImapError::Rejected("Esta conta não tem pasta de arquivo.".into())
                    })?,
                },
                Some(role) => r
                    .role_path(role)?
                    .ok_or_else(|| ImapError::Rejected("Pasta não encontrada.".into()))?,
                None => target,
            };
            r.move_to(&folder, &uids, &target)
        })
    })
    .await
}

pub async fn delete(
    app: AppHandle,
    id: String,
    folder: String,
    uids: Vec<u32>,
) -> Result<(), String> {
    blocking(move || with_reader(&app, &id, |r, _| r.delete(&folder, &uids))).await
}

/// Grava o anexo na pasta de downloads e devolve o caminho.
pub async fn save_attachment(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
    index: u32,
) -> Result<String, String> {
    blocking(move || {
        let (name, bytes) = with_reader_cached(&app, &id, |r, _, last| {
            let raw = match last {
                Some((f, u, raw)) if *f == folder && *u == uid => raw.clone(),
                _ => Arc::new(r.fetch_raw(&folder, uid, false)?.0),
            };
            core::attachment(&raw, index)
                .ok_or_else(|| ImapError::Rejected("Anexo não encontrado.".into()))
        })?;
        let dir = crate::downloads::downloads_dir(&app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = papeo_core::unique_path(&dir, &papeo_core::safe_file_name(&name), &[]);
        std::fs::write(&path, bytes).map_err(|e| format!("Não deu para salvar o anexo ({e})."))?;
        mark_downloaded(&path);
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

/// Marca o arquivo como baixado da internet: o sistema avisa antes de abrir
/// um executável (Gatekeeper no macOS, SmartScreen no Windows).
fn mark_downloaded(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let value = format!("0083;{secs:x};Papeo;");
        let (Ok(file), Ok(name)) = (
            std::ffi::CString::new(path.as_os_str().as_bytes()),
            std::ffi::CString::new("com.apple.quarantine"),
        ) else {
            return;
        };
        // SAFETY: ponteiros válidos durante a chamada; tamanho do valor exato.
        unsafe {
            libc::setxattr(
                file.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                0,
            );
        }
    }
    #[cfg(windows)]
    {
        let mut stream = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        let _ = std::fs::write(stream, "[ZoneTransfer]\r\nZoneId=3\r\n");
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = path;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAttachment {
    pub name: String,
    pub mime: String,
    /// Conteúdo em base64.
    pub data: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// Resposta: `Message-ID` da original e a cadeia dela.
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
    /// Original respondida (para marcar como respondida).
    #[serde(default)]
    pub reply_folder: Option<String>,
    #[serde(default)]
    pub reply_uid: Option<u32>,
    #[serde(default)]
    pub attachments: Vec<DraftAttachment>,
}

/// Monta a mensagem (sem enviar).
fn build(account: &MailAccount, draft: &Draft) -> Result<lettre::Message, String> {
    use base64::Engine;
    use lettre::message::header::ContentType;
    use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
    let from: Mailbox = account
        .user
        .parse()
        .map_err(|_| "O usuário da conta não é um endereço de e-mail.".to_owned())?;
    let parse = |list: &[String]| -> Result<Vec<Mailbox>, String> {
        list.iter()
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.parse::<Mailbox>()
                    .map_err(|_| format!("Endereço inválido: {}", papeo_core::clip(a, 80)))
            })
            .collect()
    };
    let (to, cc, bcc) = (parse(&draft.to)?, parse(&draft.cc)?, parse(&draft.bcc)?);
    if to.is_empty() && cc.is_empty() && bcc.is_empty() {
        return Err("Informe para quem enviar.".into());
    }
    if to.len() + cc.len() + bcc.len() > RECIPIENTS_MAX {
        return Err("Destinatários demais.".into());
    }
    if draft.body.len() > BODY_MAX {
        return Err("Texto longo demais.".into());
    }
    let subject: String = draft.subject.chars().filter(|c| !c.is_control()).collect();
    let mut builder = lettre::Message::builder()
        .from(from)
        .subject(papeo_core::clip(subject.trim(), 500));
    for m in to {
        builder = builder.to(m);
    }
    for m in cc {
        builder = builder.cc(m);
    }
    for m in bcc {
        builder = builder.bcc(m);
    }
    let msg_id = |id: &str| -> Option<String> {
        let id = id.trim().trim_start_matches('<').trim_end_matches('>');
        (!id.is_empty()
            && id.len() < 1000
            && !id
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || c == '<' || c == '>'))
        .then(|| format!("<{id}>"))
    };
    if let Some(parent) = draft.in_reply_to.as_deref().and_then(msg_id) {
        builder = builder.in_reply_to(parent.clone());
        let mut chain: Vec<String> = draft.references.iter().filter_map(|r| msg_id(r)).collect();
        if !chain.contains(&parent) {
            chain.push(parent);
        }
        let keep = chain.len().saturating_sub(20);
        builder = builder.references(chain[keep..].join(" "));
    }
    let text = SinglePart::plain(draft.body.replace("\r\n", "\n"));
    if draft.attachments.is_empty() {
        return builder.singlepart(text).map_err(|e| e.to_string());
    }
    let mut total = 0usize;
    let mut multipart = MultiPart::mixed().singlepart(text);
    for file in &draft.attachments {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(file.data.as_bytes())
            .map_err(|_| "Anexo inválido.".to_owned())?;
        total += bytes.len();
        if total > ATTACHMENTS_MAX {
            return Err("Os anexos passam de 25 MB.".into());
        }
        let mime = ContentType::parse(&file.mime)
            .unwrap_or_else(|_| ContentType::parse("application/octet-stream").expect("tipo fixo"));
        let name = papeo_core::safe_file_name(&file.name);
        multipart = multipart.singlepart(Attachment::new(name).body(bytes, mime));
    }
    builder.multipart(multipart).map_err(|e| e.to_string())
}

/// Envia por SMTP; guarda em Enviados (se o provedor não guarda sozinho) e
/// marca a original como respondida.
pub async fn send(app: AppHandle, id: String, draft: Draft) -> Result<(), String> {
    blocking(move || {
        let (account, mut saved) = account_of(&app, &id)?;
        let email = build(&account, &draft)?;
        let server = account.smtp();
        let microsoft = account.auth == MailAuth::Microsoft;
        let secret = if microsoft {
            let slot = {
                let readers = app.state::<MailReaders>();
                let mut map = readers.slots.lock().unwrap_or_else(|e| e.into_inner());
                map.entry(id.clone()).or_default().clone()
            };
            let mut access = slot.lock().unwrap_or_else(|e| e.into_inner()).send.clone();
            let token = mail::microsoft_token(
                &app,
                &id,
                &mut saved,
                &mut access,
                &Control::detached(),
                crate::microsoft::Scope::Send,
            )
            .map_err(|e| match e {
                ImapError::Auth(_) => {
                    "Para enviar, entre de novo com a conta da Microsoft (o login antigo só liberava a leitura).".to_owned()
                }
                other => mail::describe(&other),
            })?;
            slot.lock().unwrap_or_else(|e| e.into_inner()).send = access;
            token
        } else {
            account.login_secret(&saved.secret)
        };
        smtp_send(&server, &saved.user, &secret, microsoft, &email)?;
        let raw = email.formatted();
        // Depois de enviado, o resto é melhor esforço: falhar aqui não
        // desfaz o envio.
        let _ = with_reader(&app, &id, |r, account| {
            if !account.saves_sent() {
                if let Some(sent) = r.role_path(Role::Sent)? {
                    r.append(&sent, &raw)?;
                }
            }
            if let (Some(folder), Some(uid)) = (&draft.reply_folder, draft.reply_uid) {
                r.set_flags(folder, &[uid], &["\\Answered"], true)?;
            }
            Ok(())
        });
        Ok(())
    })
    .await
}

fn smtp_send(
    server: &papeo_core::mail::SmtpServer,
    user: &str,
    secret: &str,
    microsoft: bool,
    email: &lettre::Message,
) -> Result<(), String> {
    use lettre::transport::smtp::authentication::{Credentials, Mechanism};
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::Transport;
    let params = TlsParameters::new(server.host.clone()).map_err(|e| format!("TLS: {e}"))?;
    let tls = if server.starttls {
        Tls::Required(params)
    } else {
        Tls::Wrapper(params)
    };
    let mechanisms = if microsoft {
        vec![Mechanism::Xoauth2]
    } else {
        vec![Mechanism::Plain, Mechanism::Login]
    };
    let transport = lettre::SmtpTransport::builder_dangerous(server.host.as_str())
        .port(server.port)
        .tls(tls)
        .credentials(Credentials::new(user.to_owned(), secret.to_owned()))
        .authentication(mechanisms)
        .timeout(Some(Duration::from_secs(60)))
        .build();
    transport.send(email).map(|_| ()).map_err(|e| {
        if e.is_permanent() {
            let text = e.to_string();
            if text.contains("535") || text.to_ascii_lowercase().contains("auth") {
                format!(
                    "O servidor de envio ({}) recusou o login. Alguns provedores pedem liberar o SMTP ou uma senha de app.",
                    server.host
                )
            } else {
                format!("O servidor de envio recusou a mensagem ({}).", papeo_core::clip(&text, 200))
            }
        } else {
            format!(
                "Não deu para falar com o servidor de envio ({}): {}",
                server.host,
                papeo_core::clip(&e.to_string(), 200)
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> MailAccount {
        MailAccount {
            host: "imap.hostinger.com".into(),
            port: 993,
            user: "ana@x.com".into(),
            sleep_site: false,
            starttls: false,
            whole_inbox: false,
            auth: MailAuth::Password,
        }
    }

    fn draft() -> Draft {
        Draft {
            to: vec!["Bia <bia@y.com>".into(), " ".into()],
            cc: vec![],
            bcc: vec!["oculto@z.com".into()],
            subject: "Re: Olá\r\nBcc: x@y".into(),
            body: "Oi\nTudo bem?".into(),
            in_reply_to: Some("m1@x.com".into()),
            references: vec!["<m0@x.com>".into(), "ruim <".into()],
            reply_folder: None,
            reply_uid: None,
            attachments: vec![],
        }
    }

    #[test]
    fn builds_reply_without_header_injection() {
        let email = build(&account(), &draft()).unwrap();
        let raw = String::from_utf8(email.formatted()).unwrap();
        assert!(raw.contains("From: ana@x.com"), "{raw}");
        assert!(raw.contains("bia@y.com"), "{raw}");
        assert!(!raw.contains("oculto@z.com"), "{raw}");
        assert!(!raw.contains("\r\nBcc:"), "{raw}");
        assert!(raw.contains("In-Reply-To: <m1@x.com>"), "{raw}");
        assert!(raw.contains("References: <m0@x.com> <m1@x.com>"), "{raw}");
        assert_eq!(email.envelope().to().len(), 2);
    }

    #[test]
    fn rejects_bad_drafts() {
        let mut d = draft();
        d.to = vec!["não é endereço".into()];
        assert!(build(&account(), &d).is_err());
        let mut d = draft();
        d.to.clear();
        d.bcc.clear();
        assert!(build(&account(), &d).is_err());
        let mut a = account();
        a.user = "ana".into();
        assert!(build(&a, &draft()).is_err());
    }

    #[test]
    fn attaches_files() {
        let mut d = draft();
        d.attachments.push(DraftAttachment {
            name: "../conta.pdf".into(),
            mime: "application/pdf".into(),
            data: "JVBERi0=".into(),
        });
        let raw = String::from_utf8(build(&account(), &d).unwrap().formatted()).unwrap();
        assert!(raw.contains("multipart/mixed"), "{raw}");
        assert!(raw.contains("conta.pdf") && !raw.contains("../"), "{raw}");
    }
}
