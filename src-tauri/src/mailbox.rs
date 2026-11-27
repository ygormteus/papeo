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
use papeo_core::address_book::Seen;
use papeo_core::i18n::{tr, trf};
use papeo_core::mail::{ImapError, MailAccount, MailAuth};
use papeo_core::mail_cache::MailCache;
use papeo_core::mailbox::{self as core, Filter, Folder, MessageView, Page, Reader, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Conexão parada há este tempo fecha.
/// (Longo: reabrir custa TLS e login, a demora de abrir o leitor.)
const IDLE_CLOSE: Duration = Duration::from_secs(20 * 60);
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
        .ok_or(tr("Esta conta não tem o e-mail por IMAP configurado."))?;
    // Fora do lock: arquivo ou cofre do sistema.
    let saved = secrets::read(app, id)
        .filter(|s| s.matches(&account))
        .ok_or(tr("Senha não salva: configure o IMAP da conta de novo."))?;
    Ok((account, saved))
}

/// Mesma conexão: tudo igual menos a assinatura (que não muda o servidor).
fn same_server(a: &MailAccount, b: &MailAccount) -> bool {
    let strip = |m: &MailAccount| MailAccount {
        signature: String::new(),
        signature_html: Default::default(),
        ..m.clone()
    };
    strip(a) == strip(b)
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
        Some(conn) if !same_server(&conn.account, &account) => true,
        Some(conn) if conn.used.elapsed() >= IDLE_CHECK => conn.reader.noop().is_err(),
        Some(_) => false,
        None => true,
    };
    if stale {
        if let Some(mut conn) = slot.conn.take() {
            if !same_server(&conn.account, &account) {
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

/// A mensagem inteira: a guardada da última abertura ou baixada agora (e
/// guardada para o próximo anexo).
fn full_raw(
    r: &mut Reader<Tls>,
    last: &mut Option<(String, u32, Arc<Vec<u8>>)>,
    folder: &str,
    uid: u32,
) -> Result<Arc<Vec<u8>>, ImapError> {
    if let Some((f, u, raw)) = last.as_ref() {
        if f == folder && *u == uid {
            return Ok(raw.clone());
        }
    }
    let raw = Arc::new(r.fetch_raw(folder, uid, false)?.0);
    *last = Some((folder.to_owned(), uid, raw.clone()));
    Ok(raw)
}

fn describe(err: &ImapError) -> String {
    match err {
        ImapError::Auth(_) => tr("O servidor recusou o login. Confira a senha da conta.").into(),
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
    blocking(move || {
        let (folders, account) = with_reader(&app, &id, |r, account| {
            Ok((r.folders(refresh)?, account.clone()))
        })?;
        crate::mail_cache::put_folders(&app, &id, &account, &folders);
        Ok(folders)
    })
    .await
}

/// Pastas e mais novas guardadas da última vez (spec 27), sem rede.
pub async fn cached(app: AppHandle, id: String) -> Result<MailCache, String> {
    blocking(move || {
        let (account, _) = account_of(&app, &id)?;
        Ok(crate::mail_cache::get(&app, &id, &account))
    })
    .await
}

/// Até 50 mensagens depois das `skip` mais novas da pasta ou da busca.
pub async fn list(
    app: AppHandle,
    id: String,
    folder: String,
    skip: u32,
    query: String,
    filter: Filter,
) -> Result<Page, String> {
    blocking(move || {
        let (page, account) = with_reader(&app, &id, |r, account| {
            Ok((r.list_from(&folder, skip, &query, filter)?, account.clone()))
        })?;
        if skip == 0 && query.trim().is_empty() && filter == Filter::All {
            crate::mail_cache::put_list(&app, &id, &account, &folder, &page);
        }
        let own = account.user;
        // Endereços conhecidos (spec 25): de quem escreveu e, nos enviados
        // pela própria conta, para quem.
        let mine = |a: &core::Addr| a.email.eq_ignore_ascii_case(own.trim());
        crate::address_book::note(
            &app,
            &id,
            &own,
            page.messages.iter().flat_map(|m| {
                if mine(&m.from) {
                    m.to.iter().map(|a| (a, Seen::Sent)).collect::<Vec<_>>()
                } else {
                    vec![(&m.from, Seen::Received)]
                }
            }),
        );
        Ok(page)
    })
    .await
}

/// Abre a mensagem e a marca como lida (`peek`: só baixa, para adiantar a
/// próxima da lista).
pub async fn open(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
    peek: bool,
    size: Option<u32>,
) -> Result<MessageView, String> {
    blocking(move || {
        with_reader_cached(&app, &id, |r, account, last| {
            let opened = r.open_message(&folder, uid, !peek, size)?;
            // Veio aos pedaços (spec 28): o anexo baixa a mensagem inteira.
            if let Some(raw) = opened.raw {
                *last = Some((folder, uid, Arc::new(raw)));
            }
            Ok((opened.view, account.user.clone()))
        })
        .map(|(view, own)| {
            if !peek {
                let how = if view
                    .from
                    .iter()
                    .any(|a| a.email.eq_ignore_ascii_case(own.trim()))
                {
                    Seen::Sent
                } else {
                    Seen::Received
                };
                let others = view.to.iter().chain(&view.cc).map(|a| (a, how));
                let from = view.from.iter().map(|a| (a, Seen::Received));
                crate::address_book::note(&app, &id, &own, from.chain(others));
            }
            view
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
        _ => return Err(tr("Marcação inválida.").into()),
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
) -> Result<Vec<u32>, String> {
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
                        ImapError::Rejected(tr("Esta conta não tem pasta de arquivo.").into())
                    })?,
                },
                Some(role) => r
                    .role_path(role)?
                    .ok_or_else(|| ImapError::Rejected(tr("Pasta não encontrada.").into()))?,
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
) -> Result<Vec<u32>, String> {
    blocking(move || with_reader(&app, &id, |r, _| r.delete(&folder, &uids))).await
}

pub async fn mark_all_read(app: AppHandle, id: String, folder: String) -> Result<(), String> {
    blocking(move || with_reader(&app, &id, |r, _| r.mark_all_seen(&folder))).await
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
            let raw = full_raw(r, last, &folder, uid)?;
            core::attachment(&raw, index)
                .ok_or_else(|| ImapError::Rejected(tr("Anexo não encontrado.").into()))
        })?;
        let dir = crate::downloads::downloads_dir(&app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = papeo_core::unique_path(&dir, &papeo_core::safe_file_name(&name), &[]);
        std::fs::write(&path, bytes)
            .map_err(|e| trf("Não deu para salvar o anexo ({erro}).", &[("erro", &e)]))?;
        mark_downloaded(&path);
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}

/// Grava vários anexos de uma mensagem numa pasta nova em Downloads, com o
/// nome do assunto (spec 24), e devolve o caminho da pasta.
pub async fn save_attachments(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
    indices: Vec<u32>,
    subject: String,
) -> Result<String, String> {
    if indices.is_empty() {
        return Err(tr("Anexo não encontrado.").into());
    }
    blocking(move || {
        let files = with_reader_cached(&app, &id, |r, _, last| {
            let raw = full_raw(r, last, &folder, uid)?;
            indices
                .iter()
                .map(|&i| {
                    core::attachment(&raw, i)
                        .ok_or_else(|| ImapError::Rejected(tr("Anexo não encontrado.").into()))
                })
                .collect::<Result<Vec<_>, _>>()
        })?;
        let base = crate::downloads::downloads_dir(&app);
        // Pasta: o assunto sem barras nem pontos (o ponto viraria extensão).
        let name: String = subject
            .replace(['/', '\\', '.'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(80)
            .collect();
        let name = papeo_core::safe_file_name(&name);
        let name = if name.trim().is_empty() || name == "download" {
            tr("Anexos").to_string()
        } else {
            name
        };
        let dir = papeo_core::unique_path(&base, &name, &[]);
        std::fs::create_dir_all(&dir)
            .map_err(|e| trf("Não deu para salvar o anexo ({erro}).", &[("erro", &e)]))?;
        let mut written: Vec<std::path::PathBuf> = Vec::new();
        for (file, bytes) in files {
            let path = papeo_core::unique_path(&dir, &papeo_core::safe_file_name(&file), &written);
            std::fs::write(&path, bytes)
                .map_err(|e| trf("Não deu para salvar o anexo ({erro}).", &[("erro", &e)]))?;
            mark_downloaded(&path);
            written.push(path);
        }
        Ok(dir.to_string_lossy().into_owned())
    })
    .await
}

/// Abre o anexo na janela de visualização (spec 21). `false`: o tipo não
/// tem visualização (a tela salva em Downloads).
pub async fn preview_attachment(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
    index: u32,
) -> Result<bool, String> {
    blocking(move || {
        let (name, bytes) = with_reader_cached(&app, &id, |r, _, last| {
            let raw = full_raw(r, last, &folder, uid)?;
            core::attachment(&raw, index)
                .ok_or_else(|| ImapError::Rejected(tr("Anexo não encontrado.").into()))
        })?;
        crate::attachment::show_bytes(&app, &id, &name, &bytes)
    })
    .await
}

/// Marca o arquivo como baixado da internet: o sistema avisa antes de abrir
/// um executável (Gatekeeper no macOS, SmartScreen no Windows).
pub(crate) fn mark_downloaded(path: &std::path::Path) {
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAttachment {
    pub name: String,
    pub mime: String,
    /// Conteúdo em base64.
    pub data: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// Texto com formatação (spec 31): HTML do editor, com o marcador da
    /// assinatura; limpo de novo no envio. Sem valor, só o texto.
    #[serde(default)]
    pub body_html: Option<String>,
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
    /// Rascunho no servidor (spec 18): vira o `Message-ID` das versões
    /// salvas. Só o formato gerado pelo Papeo.
    #[serde(default)]
    pub draft_token: Option<String>,
    /// Rascunho de outro programa aberto no editor: sai ao salvar ou enviar.
    #[serde(default)]
    pub replaces: Option<DraftRef>,
}

/// Mensagem de uma pasta (rascunho de outro programa).
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRef {
    pub folder: String,
    pub uid: u32,
    /// `Message-ID` do rascunho: confere que o UID ainda é ele antes de
    /// apagar (vazio: o rascunho não tinha, só o `\\Draft` é conferido).
    #[serde(default)]
    pub message_id: String,
}

/// `Message-ID` das versões salvas de um rascunho do Papeo.
fn draft_message_id(token: &str) -> Option<String> {
    crate::outbox::valid_id(token).then(|| format!("<papeo-draft-{token}@papeo.draft>"))
}

/// Token de um rascunho salvo pelo Papeo, pelo `Message-ID` (sem `<>`).
fn token_of(message_id: &str) -> Option<String> {
    let token = message_id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .strip_prefix("papeo-draft-")?
        .strip_suffix("@papeo.draft")?;
    crate::outbox::valid_id(token).then(|| token.to_owned())
}

/// Rascunho aberto no editor.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedDraft {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// HTML limpo do texto (spec 31), se o rascunho tem formatação.
    pub body_html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub attachments: Vec<DraftAttachment>,
    pub draft_token: Option<String>,
    pub replaces: Option<DraftRef>,
}

/// Confere o e-mail como o envio conferiria (endereços, tamanhos, anexos),
/// sem enviar: usado antes da espera para desfazer (spec 16, R5).
pub fn validate(app: &AppHandle, id: &str, draft: &Draft) -> Result<(), String> {
    let account = app
        .state::<AppState>()
        .lock()
        .service(id)?
        .mail
        .clone()
        .ok_or(tr("Esta conta não tem o e-mail por IMAP configurado."))?;
    build(&account, draft).map(|_| ())
}

/// Monta a mensagem (sem enviar).
fn build(account: &MailAccount, draft: &Draft) -> Result<lettre::Message, String> {
    build_as(account, draft, None)
}

/// Monta a mensagem; com `draft_id`, como rascunho: aceita ficar sem
/// destinatário, guarda o Cco e usa esse `Message-ID`.
fn build_as(
    account: &MailAccount,
    draft: &Draft,
    draft_id: Option<&str>,
) -> Result<lettre::Message, String> {
    use base64::Engine;
    use lettre::message::header::ContentType;
    use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
    let from: Mailbox = account
        .user
        .parse()
        .map_err(|_| tr("O usuário da conta não é um endereço de e-mail.").to_owned())?;
    // Rascunho: endereço ainda incompleto ("ana@") não impede salvar; vai
    // num cabeçalho próprio do Papeo e volta ao editor ao reabrir.
    let mut unparsed: Vec<(&'static str, Vec<String>)> = Vec::new();
    let mut parse = |list: &[String], header: &'static str| -> Result<Vec<Mailbox>, String> {
        let mut ok = Vec::new();
        let mut bad = Vec::new();
        for a in list.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
            match a.parse::<Mailbox>() {
                Ok(m) => ok.push(m),
                Err(_) if draft_id.is_some() => bad.push(papeo_core::clip(a, 300)),
                Err(_) => {
                    return Err(trf(
                        "Endereço inválido: {endereco}",
                        &[("endereco", &papeo_core::clip(a, 80))],
                    ));
                }
            }
        }
        if !bad.is_empty() {
            unparsed.push((header, bad));
        }
        Ok(ok)
    };
    let to = parse(&draft.to, core::UNPARSED_TO)?;
    let cc = parse(&draft.cc, core::UNPARSED_CC)?;
    let bcc = parse(&draft.bcc, core::UNPARSED_BCC)?;
    if draft_id.is_none() && to.is_empty() && cc.is_empty() && bcc.is_empty() {
        return Err(tr("Informe para quem enviar.").into());
    }
    if to.len() + cc.len() + bcc.len() > RECIPIENTS_MAX {
        return Err(tr("Destinatários demais.").into());
    }
    if draft.body.len() > BODY_MAX || draft.body_html.as_ref().is_some_and(|h| h.len() > BODY_MAX) {
        return Err(tr("Texto longo demais.").into());
    }
    let subject: String = draft.subject.chars().filter(|c| !c.is_control()).collect();
    let mut builder = lettre::Message::builder()
        .from(from.clone())
        .subject(papeo_core::clip(subject.trim(), 500));
    if let Some(id) = draft_id {
        // Rascunho não é enviado: o envelope (exigido pelo lettre) aponta
        // para o próprio remetente e o Cco fica no cabeçalho.
        let envelope =
            lettre::address::Envelope::new(Some(from.email.clone()), vec![from.email.clone()])
                .map_err(|e| e.to_string())?;
        builder = builder
            .message_id(Some(id.to_owned()))
            .keep_bcc()
            .envelope(envelope);
    }
    for m in to {
        builder = builder.to(m);
    }
    for m in cc {
        builder = builder.cc(m);
    }
    for m in bcc {
        builder = builder.bcc(m);
    }
    for (name, list) in unparsed {
        use lettre::message::header::{HeaderName, HeaderValue};
        builder = builder.raw_header(HeaderValue::new(
            HeaderName::new_from_ascii_str(name),
            core::encode_unparsed(&list),
        ));
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
    let body = draft.body.replace("\r\n", "\n");
    let text = SinglePart::plain(body.clone());
    // Assinatura formatada no corpo sem mudanças (spec 22): texto + HTML,
    // com as imagens dentro do e-mail.
    // Com formatação (spec 31): o HTML do editor, limpo de novo aqui.
    let html = match draft.body_html.as_deref().filter(|h| !h.trim().is_empty()) {
        Some(edited) => Some(papeo_core::signature::html_body(
            &body,
            edited,
            &account.signature,
            &account.signature_html,
        )),
        None => {
            papeo_core::signature::html_message(&body, &account.signature, &account.signature_html)
        }
    };
    let rich = html.map(|html| {
        let (html, images) =
            papeo_core::signature::inline_images(&html, &papeo_core::new_item_id());
        let mut related = MultiPart::related().singlepart(SinglePart::html(html));
        for image in images {
            let mime = ContentType::parse(&image.mime).unwrap_or_else(|_| {
                ContentType::parse("application/octet-stream").expect("tipo fixo")
            });
            related = related.singlepart(Attachment::new_inline(image.cid).body(image.bytes, mime));
        }
        MultiPart::alternative()
            .singlepart(text.clone())
            .multipart(related)
    });
    if draft.attachments.is_empty() {
        return match rich {
            Some(alternative) => builder.multipart(alternative),
            None => builder.singlepart(text),
        }
        .map_err(|e| e.to_string());
    }
    let mut total = 0usize;
    let mut multipart = match rich {
        Some(alternative) => MultiPart::mixed().multipart(alternative),
        None => MultiPart::mixed().singlepart(text),
    };
    for file in &draft.attachments {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(file.data.as_bytes())
            .map_err(|_| tr("Anexo inválido.").to_owned())?;
        total += bytes.len();
        if total > ATTACHMENTS_MAX {
            return Err(tr("Os anexos passam de 25 MB.").into());
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
                    tr("Para enviar, entre de novo com a conta da Microsoft (o login antigo só liberava a leitura).").to_owned()
                }
                other => mail::describe(&other),
            })?;
            slot.lock().unwrap_or_else(|e| e.into_inner()).send = access;
            token
        } else {
            account.login_secret(&saved.secret)
        };
        smtp_send(&server, &saved.user, &secret, microsoft, &email)?;
        note_recipients(&app, &id, &account.user, &draft);
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
        // O rascunho deste e-mail sai do servidor (melhor esforço).
        let _ = with_reader(&app, &id, |r, _| {
            drop_draft(r, draft.draft_token.as_deref(), draft.replaces.as_ref())
        });
        Ok(())
    })
    .await
}

/// Destinatários de um e-mail enviado entram nos endereços conhecidos.
fn note_recipients(app: &AppHandle, id: &str, own: &str, draft: &Draft) {
    use lettre::message::Mailbox;
    let addrs: Vec<core::Addr> = draft
        .to
        .iter()
        .chain(&draft.cc)
        .chain(&draft.bcc)
        .filter_map(|a| a.trim().parse::<Mailbox>().ok())
        .map(|m| core::Addr {
            name: m.name.unwrap_or_default(),
            email: m.email.to_string(),
        })
        .collect();
    crate::address_book::note(app, id, own, addrs.iter().map(|a| (a, Seen::Sent)));
}

/// Lê os destinatários dos últimos enviados (uma vez por conta), para a
/// pessoa já achar quem ela costuma escrever (spec 25).
pub fn backfill_contacts(app: AppHandle, id: String) {
    tauri::async_runtime::spawn_blocking(move || {
        let result = with_reader(&app, &id, |r, account| {
            let mut seen = Vec::new();
            if let Some(sent) = r.role_path(Role::Sent)? {
                for page in 0..4 {
                    let list = r.list(&sent, page, "")?;
                    let done = list.messages.is_empty();
                    seen.extend(list.messages.into_iter().flat_map(|m| m.to));
                    if done {
                        break;
                    }
                }
            }
            Ok((seen, account.user.clone()))
        });
        let ok = match result {
            Ok((seen, own)) => {
                crate::address_book::note(&app, &id, &own, seen.iter().map(|a| (a, Seen::Sent)));
                true
            }
            Err(_) => false,
        };
        crate::address_book::backfilled(&app, &id, ok);
    });
}

/// Apaga as versões salvas do rascunho e o de outro programa substituído.
fn drop_draft(
    r: &mut Reader<Tls>,
    token: Option<&str>,
    replaces: Option<&DraftRef>,
) -> Result<(), ImapError> {
    if let Some(id) = token.and_then(draft_message_id) {
        if let Some(folder) = r.role_path(Role::Drafts)? {
            r.discard_drafts(&folder, &id)?;
        }
    }
    if let Some(old) = replaces {
        if let Some(folder) = r.role_path(Role::Drafts)? {
            replace_foreign(r, &folder, old)?;
        }
    }
    Ok(())
}

/// Sem a pasta: o editor para de salvar sozinho (spec 18, R8).
pub const NO_DRAFTS: &str = "Esta conta não tem pasta Rascunhos no servidor.";

/// Apaga o rascunho de outro programa que o editor substituiu: só na pasta
/// Rascunhos e só se o UID ainda for daquele rascunho (`Message-ID`).
fn replace_foreign(r: &mut Reader<Tls>, drafts: &str, old: &DraftRef) -> Result<(), ImapError> {
    if old.folder != drafts {
        return Ok(());
    }
    r.discard_checked(drafts, old.uid, &old.message_id)
}

/// Conta de e-mail configurada (sem a senha).
fn account_only(app: &AppHandle, id: &str) -> Result<MailAccount, String> {
    app.state::<AppState>()
        .lock()
        .service(id)?
        .mail
        .clone()
        .ok_or_else(|| tr("Esta conta não tem o e-mail por IMAP configurado.").to_owned())
}

/// Salva o rascunho na pasta Rascunhos (spec 18). Devolve o token dele.
pub async fn save_draft(app: AppHandle, id: String, draft: Draft) -> Result<String, String> {
    blocking(move || {
        let account = account_only(&app, &id)?;
        let token = draft
            .draft_token
            .clone()
            .filter(|t| crate::outbox::valid_id(t))
            .unwrap_or_else(crate::outbox::new_id);
        let message_id = draft_message_id(&token).ok_or(tr("Rascunho inválido."))?;
        let raw = build_as(&account, &draft, Some(&message_id))?.formatted();
        // Uma ida só à conexão da conta: dois salvamentos não se cruzam
        // (a ordem é a de chegada).
        let saved = with_reader(&app, &id, |r, _| {
            let Some(folder) = r.role_path(Role::Drafts)? else {
                return Ok(false);
            };
            r.save_draft(&folder, &raw, &message_id)?;
            if let Some(old) = &draft.replaces {
                replace_foreign(r, &folder, old)?;
            }
            Ok(true)
        })?;
        if !saved {
            return Err(NO_DRAFTS.into());
        }
        Ok(token)
    })
    .await
}

/// Descarta o rascunho: apaga do servidor as versões salvas.
pub async fn discard_draft(
    app: AppHandle,
    id: String,
    token: Option<String>,
    replaces: Option<DraftRef>,
) -> Result<(), String> {
    blocking(move || {
        with_reader(&app, &id, |r, _| {
            drop_draft(r, token.as_deref(), replaces.as_ref())
        })
    })
    .await
}

/// Abre uma mensagem da pasta Rascunhos no editor.
pub async fn open_draft(
    app: AppHandle,
    id: String,
    folder: String,
    uid: u32,
) -> Result<OpenedDraft, String> {
    use base64::Engine;
    blocking(move || {
        let raw = with_reader(&app, &id, |r, _| {
            r.fetch_raw(&folder, uid, false).map(|(raw, _, _)| raw)
        })?;
        let view = core::draft_view(&raw).ok_or(tr("Os anexos deste rascunho passam de 25 MB."))?;
        let token = token_of(&view.message_id);
        Ok(OpenedDraft {
            to: view.to,
            cc: view.cc,
            bcc: view.bcc,
            subject: view.subject,
            body: view.text.replace("\r\n", "\n"),
            body_html: view.html,
            in_reply_to: view.in_reply_to,
            references: view.references,
            attachments: view
                .attachments
                .into_iter()
                .map(|(name, mime, bytes)| DraftAttachment {
                    name,
                    mime,
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                })
                .collect(),
            replaces: token.is_none().then(|| DraftRef {
                folder: folder.clone(),
                uid,
                message_id: view.message_id.clone(),
            }),
            draft_token: token,
        })
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
                trf(
                    "O servidor de envio ({servidor}) recusou o login. Alguns provedores pedem liberar o SMTP ou uma senha de app.",
                    &[("servidor", &server.host)],
                )
            } else {
                trf(
                    "O servidor de envio recusou a mensagem ({texto}).",
                    &[("texto", &papeo_core::clip(&text, 200))],
                )
            }
        } else {
            trf(
                "Não deu para falar com o servidor de envio ({servidor}): {erro}",
                &[
                    ("servidor", &server.host),
                    ("erro", &papeo_core::clip(&e.to_string(), 200)),
                ],
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
            signature: String::new(),
            signature_html: Default::default(),
        }
    }

    fn draft() -> Draft {
        Draft {
            to: vec!["Bia <bia@y.com>".into(), " ".into()],
            cc: vec![],
            bcc: vec!["oculto@z.com".into()],
            subject: "Re: Olá\r\nBcc: x@y".into(),
            body: "Oi\nTudo bem?".into(),
            body_html: None,
            in_reply_to: Some("m1@x.com".into()),
            references: vec!["<m0@x.com>".into(), "ruim <".into()],
            reply_folder: None,
            reply_uid: None,
            attachments: vec![],
            draft_token: None,
            replaces: None,
        }
    }

    #[test]
    fn formatted_body_goes_as_clean_html_with_the_signature() {
        const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let mut acc = account();
        acc.signature = "Ana Souza".into();
        acc.signature_html = format!(r#"<b>Ana</b><img src="data:image/png;base64,{PNG}">"#).into();
        let mut d = draft();
        d.body = "Olá grande\n\n-- \nAna Souza".into();
        d.body_html = Some(format!(
            r#"<div><b>Olá</b> <font size="5">grande</font><img src=x onerror=alert(1)></div>{}"#,
            papeo_core::signature::SIGNATURE_TOKEN
        ));
        let raw = String::from_utf8(build(&acc, &d).unwrap().formatted()).unwrap();
        assert!(
            raw.contains("multipart/alternative") && raw.contains("multipart/related"),
            "{raw}"
        );
        // Parte HTML (quoted-printable): negrito e tamanho; sem o script.
        let flat = raw.replace("=\r\n", "").replace("=3D", "=");
        assert!(
            flat.contains("<b>Ol") && flat.contains(r#"<font size="5">grande</font>"#),
            "{flat}"
        );
        assert!(
            !flat.contains("onerror") && !flat.contains("src=x"),
            "{flat}"
        );
        assert!(flat.contains("Content-ID: <assinatura1."), "{flat}");
        // Parte de texto continua.
        assert!(raw.contains("Content-Type: text/plain"), "{raw}");
        // HTML grande demais.
        d.body_html = Some("x".repeat(BODY_MAX + 1));
        assert!(build(&acc, &d).is_err());
    }

    #[test]
    fn rich_signature_goes_as_html_with_inline_images() {
        const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let mut acc = account();
        acc.signature = "Ana Souza".into();
        acc.signature_html = format!(r#"<b>Ana</b><img src="data:image/png;base64,{PNG}">"#).into();
        let mut d = draft();
        d.body = "Olá\n\n-- \nAna Souza\n\n> citado".into();
        let raw = String::from_utf8(build(&acc, &d).unwrap().formatted()).unwrap();
        assert!(raw.contains("multipart/alternative"), "{raw}");
        assert!(raw.contains("multipart/related"), "{raw}");
        assert!(raw.contains("Content-Type: text/html"), "{raw}");
        assert!(raw.contains("Content-Disposition: inline"), "{raw}");
        assert!(raw.contains("Content-ID: <assinatura1."), "{raw}");
        assert!(!raw.contains("data:image"), "{raw}");
        // Assinatura apagada no editor: só texto.
        d.body = "Olá".into();
        let raw = String::from_utf8(build(&acc, &d).unwrap().formatted()).unwrap();
        assert!(!raw.contains("multipart"), "{raw}");
        assert!(!raw.contains("text/html"), "{raw}");
        // Com anexo: misto, com a alternativa dentro.
        d.body = "Olá\n-- \nAna Souza".into();
        d.attachments = vec![DraftAttachment {
            name: "a.txt".into(),
            mime: "text/plain".into(),
            data: "b2k=".into(),
        }];
        let raw = String::from_utf8(build(&acc, &d).unwrap().formatted()).unwrap();
        assert!(
            raw.contains("multipart/mixed") && raw.contains("multipart/alternative"),
            "{raw}"
        );
    }

    #[test]
    fn draft_keeps_bcc_and_may_have_no_recipient() {
        let token = crate::outbox::new_id();
        let id = draft_message_id(&token).unwrap();
        let mut d = draft();
        d.to.clear();
        let raw =
            String::from_utf8(build_as(&account(), &d, Some(&id)).unwrap().formatted()).unwrap();
        assert!(raw.contains("Bcc: oculto@z.com"), "{raw}");
        assert!(raw.contains(&format!("Message-ID: {id}")), "{raw}");
        d.bcc.clear();
        assert!(build_as(&account(), &d, Some(&id)).is_ok());
        // Endereço incompleto: salva no rascunho, mas não envia.
        d.to = vec!["ana@".into(), "bia@y.com".into()];
        let raw =
            String::from_utf8(build_as(&account(), &d, Some(&id)).unwrap().formatted()).unwrap();
        assert!(raw.contains(papeo_core::mailbox::UNPARSED_TO), "{raw}");
        assert!(raw.contains("To: bia@y.com"), "{raw}");
        assert_eq!(
            papeo_core::mailbox::draft_view(raw.as_bytes()).unwrap().to,
            ["bia@y.com", "ana@"]
        );
        assert!(build(&account(), &d).is_err());
        // O Papeo reconhece o próprio rascunho pelo `Message-ID`.
        assert_eq!(token_of(id.trim_matches(['<', '>'])), Some(token.clone()));
        assert_eq!(token_of(&id), Some(token));
        assert_eq!(token_of("papeo-draft-../x@papeo.draft"), None);
        assert_eq!(token_of("abc@gmail.com"), None);
        assert!(draft_message_id("../x").is_none());
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
