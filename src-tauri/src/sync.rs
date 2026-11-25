//! Sincronização de tarefas e anotações pelo Google Drive (pasta oculta do
//! app), cifrada com a frase-senha. Ver `papeo_core::sync` para a junção e a
//! criptografia e `drive.rs` para o acesso ao Google.
//!
//! Guardado neste computador (`sync.json`, só o usuário lê): o token de
//! renovação do Google, a chave derivada da frase-senha (a frase em si não
//! fica em lugar nenhum) e o último estado sincronizado (base da junção).
//!
//! Regra: nunca segurar o lock do `AppState` durante acesso à rede.

use crate::drive;
use crate::state::{AppState, Inner};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use papeo_core::sync::{self as core, DeviceFile, Sealed, SyncKey, SyncMeta};
use papeo_core::Organizer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const SYNC_EVENT: &str = "papeo://sync";
const META_NAME: &str = "meta.json";
/// Depois de uma mudança, espera isto antes de enviar (junta digitação).
const AFTER_CHANGE: Duration = Duration::from_secs(5);
/// Sincroniza sozinho a cada…
const EVERY: Duration = Duration::from_secs(2 * 60);
/// Depois de um erro, espera mais antes de tentar de novo.
const AFTER_ERROR: Duration = Duration::from_secs(5 * 60);

/// O que fica no disco (`sync.json`, permissão só do usuário).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    device_id: String,
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    refresh_token: Option<String>,
    /// Chave (base64) derivada da frase-senha.
    #[serde(default)]
    key: Option<String>,
    /// Último estado sincronizado (base da junção de anotações).
    #[serde(default)]
    base: Organizer,
    /// Segundos Unix da última sincronização completa.
    #[serde(default)]
    last_sync: Option<u64>,
    #[serde(default)]
    own_file_id: Option<String>,
}

/// O que a interface mostra.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// Esta versão tem o acesso ao Google configurado.
    pub available: bool,
    /// Entrou na conta Google.
    pub signed_in: bool,
    /// Tem a chave: sincroniza sozinho.
    pub connected: bool,
    /// Falta a frase-senha: "create" (primeiro computador) ou "enter".
    pub needs_passphrase: Option<String>,
    pub running: bool,
    pub last_sync: Option<u64>,
    pub error: Option<String>,
    pub device_name: String,
    /// Quantos outros computadores sincronizam.
    pub devices: usize,
}

pub struct SyncRuntime {
    path: PathBuf,
    /// Uma coisa por vez: rodada, conectar, frase-senha, recomeçar, sair.
    gate: tauri::async_runtime::Mutex<()>,
    /// Arquivos dos outros computadores já abertos (id → data na nuvem).
    cache: Mutex<HashMap<String, (String, DeviceFile)>>,
    stored: Mutex<Stored>,
    access: Mutex<Option<(String, Instant)>>,
    running: AtomicBool,
    dirty_at: Mutex<Option<Instant>>,
    last_try: Mutex<Option<Instant>>,
    error: Mutex<Option<String>>,
    needs: Mutex<Option<String>>,
    devices: Mutex<usize>,
}

fn now_secs() -> u64 {
    crate::state::now_secs()
}

fn read_stored(path: &PathBuf) -> Stored {
    let mut stored: Stored = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if !core::is_device_id(&stored.device_id) {
        stored.device_id = core::new_device_id();
    }
    if stored.device_name.is_empty() {
        stored.device_name = sysinfo::System::host_name()
            .ok()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Computador".into());
    }
    stored.base = std::mem::take(&mut stored.base).sanitized(crate::organizer::now_ms());
    stored
}

fn write_stored(path: &PathBuf, stored: &Stored) -> Result<(), String> {
    let body = serde_json::to_vec(stored).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp-papeo");
    // Sobra antiga pode ter permissão mais aberta: começa do zero.
    let _ = std::fs::remove_file(&tmp);
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        file.write_all(&body).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Chamar no `setup`. `dir`: pasta local do app (não sincronizada pelo
/// perfil do Windows).
pub fn init(app: &AppHandle, dir: PathBuf) {
    let path = dir.join("sync.json");
    let stored = read_stored(&path);
    let _ = write_stored(&path, &stored);
    app.manage(SyncRuntime {
        path,
        gate: tauri::async_runtime::Mutex::new(()),
        cache: Mutex::new(HashMap::new()),
        stored: Mutex::new(stored),
        access: Mutex::new(None),
        running: AtomicBool::new(false),
        dirty_at: Mutex::new(None),
        last_try: Mutex::new(None),
        error: Mutex::new(None),
        needs: Mutex::new(None),
        devices: Mutex::new(0),
    });
}

fn rt(app: &AppHandle) -> tauri::State<'_, SyncRuntime> {
    app.state::<SyncRuntime>()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn save(app: &AppHandle) -> Result<(), String> {
    let rt = rt(app);
    let stored = lock(&rt.stored).clone();
    write_stored(&rt.path, &stored)
}

fn key_of(stored: &Stored) -> Option<SyncKey> {
    let bytes = B64.decode(stored.key.as_deref()?).ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    Some(SyncKey::from_bytes(arr))
}

pub fn status(app: &AppHandle) -> SyncStatus {
    let rt = rt(app);
    let needs_passphrase = lock(&rt.needs).clone();
    let error = lock(&rt.error).clone();
    let devices = *lock(&rt.devices);
    let running = rt.running.load(Ordering::Relaxed);
    let stored = lock(&rt.stored).clone();
    SyncStatus {
        available: drive::client().is_some(),
        signed_in: stored.refresh_token.is_some(),
        connected: stored.refresh_token.is_some() && stored.key.is_some(),
        needs_passphrase,
        running,
        last_sync: stored.last_sync,
        error,
        device_name: stored.device_name,
        devices,
    }
}

fn emit(app: &AppHandle) {
    let _ = app.emit_to(crate::state::UI_WEBVIEW, SYNC_EVENT, status(app));
}

/// Tarefas ou anotações mudaram: sincroniza em instantes.
pub fn mark_dirty(app: &AppHandle) {
    if let Some(rt) = app.try_state::<SyncRuntime>() {
        lock(&rt.dirty_at).get_or_insert_with(Instant::now);
    }
}

/// Manutenção (a cada 15 s): sincroniza se mudou algo, se faz tempo ou
/// logo depois de o computador acordar.
pub fn tick(app: &AppHandle, woke: bool) {
    let Some(rt) = app.try_state::<SyncRuntime>() else {
        return;
    };
    if !status(app).connected || rt.running.load(Ordering::Relaxed) {
        return;
    }
    let dirty = lock(&rt.dirty_at).is_some_and(|t| t.elapsed() >= AFTER_CHANGE);
    let wait = if lock(&rt.error).is_some() {
        AFTER_ERROR
    } else {
        EVERY
    };
    let due = lock(&rt.last_try).is_none_or(|t| t.elapsed() >= wait);
    if dirty || due || woke {
        spawn_sync(app);
    }
}

pub fn spawn_sync(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = run(&app).await;
    });
}

async fn access_token(app: &AppHandle) -> Result<String, String> {
    let rt = rt(app);
    if let Some((token, until)) = lock(&rt.access).clone() {
        if Instant::now() < until {
            return Ok(token);
        }
    }
    let refresh = lock(&rt.stored)
        .refresh_token
        .clone()
        .ok_or("Entre na conta Google de novo.")?;
    match drive::refresh(&refresh).await {
        Ok(t) => {
            let life = Duration::from_secs(t.expires_in.clamp(60, 3600).saturating_sub(60));
            *lock(&rt.access) = Some((t.access_token.clone(), Instant::now() + life));
            Ok(t.access_token)
        }
        Err(e) if e == "REAUTH" => {
            lock(&rt.stored).refresh_token = None;
            let _ = save(app);
            Err("O acesso ao Google Drive expirou ou foi removido. Conecte de novo.".into())
        }
        Err(e) => Err(e),
    }
}

/// Uma chamada ao Drive com o token; 401 limpa o token e tenta uma vez mais.
macro_rules! with_token {
    ($app:expr, |$t:ident| $call:expr) => {{
        let $t = access_token($app).await?;
        match $call.await {
            Err(e) if e == "REAUTH" => {
                *lock(&rt($app).access) = None;
                let $t = access_token($app).await?;
                $call.await.map_err(|e| {
                    if e == "REAUTH" {
                        "O Google recusou o acesso. Conecte de novo.".to_owned()
                    } else {
                        e
                    }
                })
            }
            other => other,
        }
    }};
}

fn device_name_of(name: &str) -> Option<&str> {
    name.strip_prefix("device-")?.strip_suffix(".json")
}

/// `meta.json` que vale: o mais antigo (dois computadores podem criar ao
/// mesmo tempo; todos escolhem o mesmo).
fn chosen_meta(files: &[drive::RemoteFile]) -> Option<&drive::RemoteFile> {
    files.iter().filter(|f| f.name == META_NAME).min_by(|a, b| {
        a.created_time
            .cmp(&b.created_time)
            .then_with(|| a.id.cmp(&b.id))
    })
}

async fn read_meta(
    app: &AppHandle,
    files: &[drive::RemoteFile],
) -> Result<Option<(String, SyncMeta)>, String> {
    let Some(file) = chosen_meta(files) else {
        return Ok(None);
    };
    let id = file.id.clone();
    let bytes = with_token!(app, |t| drive::download(&t, &id))?;
    serde_json::from_slice(&bytes)
        .map(|m| Some((id, m)))
        .map_err(|_| "Dados de sincronização inválidos no Google Drive.".into())
}

/// Apaga da nuvem, sem parar se falhar (limpeza).
async fn delete_quietly(app: &AppHandle, id: &str) {
    let id = id.to_owned();
    let result: Result<(), String> = async { with_token!(app, |t| drive::delete(&t, &id)) }.await;
    if let Err(e) = result {
        eprintln!("papeo: sincronização não apagou um arquivo: {e}");
    }
}

fn friendly(e: String) -> String {
    if e == drive::NOT_FOUND {
        "Um arquivo mudou no Google Drive durante a sincronização. Tentamos de novo em instantes."
            .into()
    } else {
        e
    }
}

/// Liga contatos vindos de outro computador às contas daqui (o id da conta
/// muda de um computador para outro; o tipo e o nome, não).
fn relink(inner: &Inner, org: &mut Organizer) {
    for c in org.contacts.iter_mut().filter(|c| !c.deleted) {
        if inner.service(&c.service_id).is_ok() {
            continue;
        }
        let kind = |s: &papeo_core::Service| {
            serde_json::to_value(s.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default()
        };
        if let Some(s) = inner
            .config
            .services
            .iter()
            .find(|s| kind(s) == c.service_kind && s.name == c.service_name)
        {
            c.service_id = s.id.clone();
        }
    }
}

/// Uma rodada: baixa os outros computadores, junta, grava aqui e envia o
/// nosso arquivo.
/// Rodada automática: se outra coisa da sincronização está em andamento,
/// fica para o próximo ciclo.
pub async fn run(app: &AppHandle) -> Result<(), String> {
    let rt = rt(app);
    let Ok(_gate) = rt.gate.try_lock() else {
        return Ok(());
    };
    round(app).await
}

/// Uma rodada; quem chama segura o `gate`.
async fn round(app: &AppHandle) -> Result<(), String> {
    let rt = rt(app);
    rt.running.store(true, Ordering::SeqCst);
    *lock(&rt.last_try) = Some(Instant::now());
    emit(app);
    let result = run_inner(app).await.map_err(friendly);
    rt.running.store(false, Ordering::SeqCst);
    *lock(&rt.error) = result.as_ref().err().cloned();
    emit(app);
    result
}

async fn run_inner(app: &AppHandle) -> Result<(), String> {
    let rt = rt(app);
    let (key, device_id, device_name) = {
        let stored = lock(&rt.stored);
        (
            key_of(&stored).ok_or("Falta a frase-senha.")?,
            stored.device_id.clone(),
            stored.device_name.clone(),
        )
    };
    // Mudanças a partir daqui entram na próxima rodada.
    *lock(&rt.dirty_at) = None;
    let files = with_token!(app, |t| drive::list(&t))?;
    // A frase-senha ainda é a mesma? (outro computador pode ter recomeçado)
    match read_meta(app, &files).await? {
        Some((meta_id, meta)) if core::open(&key, "meta", &meta.check).is_ok() => {
            // Sobras de criações ao mesmo tempo.
            for f in files
                .iter()
                .filter(|f| f.name == META_NAME && f.id != meta_id)
            {
                delete_quietly(app, &f.id).await;
            }
        }
        _ => {
            lock(&rt.stored).key = None;
            lock(&rt.cache).clear();
            *lock(&rt.needs) = Some("enter".into());
            let _ = save(app);
            return Err(
                "A sincronização foi recomeçada em outro computador. Digite a nova frase-senha."
                    .into(),
            );
        }
    }
    let now_ms = crate::organizer::now_ms();
    let stale_before = now_ms.saturating_sub(core::STALE_DEVICE_MS);
    let too_old = now_ms.saturating_sub(core::MERGE_DEVICE_MAX_MS);
    // Algum arquivo não pôde ser lido: a lista dos outros está incompleta.
    let mut partial = false;
    let known_own = lock(&rt.stored).own_file_id.clone();
    let mut remotes = Vec::new();
    let mut own_files = Vec::new();
    let mut seen = Vec::new();
    for f in &files {
        let Some(dev) = device_name_of(&f.name).filter(|d| core::is_device_id(d)) else {
            continue;
        };
        if dev == device_id {
            own_files.push(f.id.clone());
            continue;
        }
        seen.push(f.id.clone());
        let cached = lock(&rt.cache)
            .get(&f.id)
            .filter(|(at, _)| !f.modified_time.is_empty() && *at == f.modified_time)
            .map(|(_, file)| file.clone());
        let file = match cached {
            Some(file) => file,
            None => {
                let id = f.id.clone();
                let bytes = match with_token!(app, |t| drive::download(&t, &id)) {
                    Ok(b) => b,
                    // Apagado entre a lista e o download.
                    Err(e) if e == drive::NOT_FOUND => {
                        partial = true;
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                let opened = serde_json::from_slice::<Sealed>(&bytes)
                    .map_err(|e| e.to_string())
                    .and_then(|sealed| core::open_device(&key, dev, &sealed, now_ms));
                match opened {
                    Ok(file) => {
                        lock(&rt.cache)
                            .insert(f.id.clone(), (f.modified_time.clone(), file.clone()));
                        file
                    }
                    // Arquivo de outro computador ilegível: ignora este, segue.
                    Err(e) => {
                        eprintln!("papeo: sincronização ignorou {dev}: {e}");
                        partial = true;
                        continue;
                    }
                }
            }
        };
        // Computador parado há muito tempo: junta uma última vez (enquanto os
        // apagados ainda valem) e tira da nuvem; volta inteiro se ele
        // sincronizar de novo. Velho demais: só tira.
        if file.written_at < stale_before {
            delete_quietly(app, &f.id).await;
            lock(&rt.cache).remove(&f.id);
            if file.written_at < too_old {
                continue;
            }
        }
        remotes.push(file.organizer);
    }
    lock(&rt.cache).retain(|id, _| seen.contains(id));
    *lock(&rt.devices) = remotes.len();
    // O nosso arquivo: um só (sobras de falhas antigas saem).
    let own_file = known_own
        .filter(|id| own_files.contains(id))
        .or_else(|| own_files.first().cloned());
    for extra in own_files.iter().filter(|id| Some(*id) != own_file.as_ref()) {
        delete_quietly(app, extra).await;
    }
    // Este computador ficou parado muito tempo: esquece o que os outros
    // apagaram de vez nesse meio-tempo.
    let long_offline = !partial
        && lock(&rt.stored)
            .last_sync
            .is_some_and(|at| at.saturating_mul(1000) < stale_before);

    // Junta com o estado atual (sob o lock, sem rede).
    let base = lock(&rt.stored).base.clone();
    let merged = {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        if let Some(err) = &inner.organizer_error {
            return Err(err.clone());
        }
        let mut acc = inner.organizer.clone();
        if long_offline {
            core::forget_purged(&mut acc, &base, &remotes);
        }
        for mut remote in remotes {
            relink(&inner, &mut remote);
            acc = core::merge(&base, &acc, &remote, now_ms);
        }
        core::dedupe_contacts(&mut acc, now_ms);
        let changed = acc != inner.organizer;
        if changed {
            inner.organizer = acc.clone();
            state.persist_organizer(&inner)?;
        }
        (acc, changed)
    };
    let (merged, changed) = merged;
    if changed {
        crate::organizer::publish_organizer(app);
    }

    // Envia o nosso arquivo.
    let file = DeviceFile {
        version: 1,
        device: device_id.clone(),
        device_name,
        written_at: now_ms,
        organizer: merged.clone(),
    };
    let body = serde_json::to_vec(&core::seal_device(&key, &file)?).map_err(|e| e.to_string())?;
    let name = format!("device-{device_id}.json");
    let uploaded = match with_token!(app, |t| drive::upload(
        &t,
        own_file.as_deref(),
        &name,
        body.clone()
    )) {
        Ok(id) => id,
        // Só se sumiu da nuvem: cria de novo. Outro erro não cria (o envio
        // pode ter dado certo e a resposta se perdido).
        Err(e) if e == drive::NOT_FOUND && own_file.is_some() => {
            with_token!(app, |t| drive::upload(&t, None, &name, body.clone()))?
        }
        Err(e) => return Err(e),
    };
    {
        let mut stored = lock(&rt.stored);
        stored.own_file_id = Some(uploaded);
        stored.base = merged;
        stored.last_sync = Some(now_secs());
    }
    save(app)
}

// ── Comandos da interface ────────────────────────────────────────────────

fn ensure_unlocked(app: &AppHandle) -> Result<(), String> {
    if app.state::<AppState>().lock().locked {
        Err("Desbloqueie o Papeo primeiro.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn sync_status(app: AppHandle) -> SyncStatus {
    status(&app)
}

/// Entra na conta Google (navegador do sistema) e diz se falta criar ou
/// digitar a frase-senha.
#[tauri::command]
pub async fn sync_connect(app: AppHandle) -> Result<SyncStatus, String> {
    ensure_unlocked(&app)?;
    let opener = app.clone();
    let code = tauri::async_runtime::spawn_blocking(move || {
        drive::login(|url| {
            use tauri_plugin_opener::OpenerExt;
            opener
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|e| e.to_string())
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    let tokens = drive::exchange(code).await?;
    if !tokens.has_drive_scope() {
        return Err(
            "O acesso à pasta do Papeo no Google Drive não foi marcado. Conecte de novo e marque essa permissão."
                .into(),
        );
    }
    let rt = rt(&app);
    let _gate = rt.gate.lock().await;
    lock(&rt.cache).clear();
    let refresh = tokens
        .refresh_token
        .clone()
        .ok_or("O Google não liberou o acesso contínuo. Tente de novo.")?;
    {
        let mut stored = lock(&rt.stored);
        stored.refresh_token = Some(refresh);
        stored.key = None;
        stored.own_file_id = None;
    }
    *lock(&rt.access) = Some((
        tokens.access_token.clone(),
        Instant::now() + Duration::from_secs(tokens.expires_in.clamp(60, 3600).saturating_sub(60)),
    ));
    save(&app)?;
    let files = with_token!(&app, |t| drive::list(&t))?;
    let needs = if files.iter().any(|f| f.name == META_NAME) {
        "enter"
    } else {
        "create"
    };
    *lock(&rt.needs) = Some(needs.into());
    *lock(&rt.error) = None;
    emit(&app);
    Ok(status(&app))
}

/// Cria (primeiro computador) ou confere a frase-senha e sincroniza.
#[tauri::command]
pub async fn sync_passphrase(app: AppHandle, passphrase: String) -> Result<SyncStatus, String> {
    ensure_unlocked(&app)?;
    let rt = rt(&app);
    let _gate = rt.gate.lock().await;
    use_passphrase(&app, passphrase).await.map_err(friendly)
}

async fn unlock(meta: SyncMeta, passphrase: String) -> Result<SyncKey, String> {
    tauri::async_runtime::spawn_blocking(move || core::unlock_meta(&meta, &passphrase))
        .await
        .map_err(|e| e.to_string())?
}

/// Quem chama segura o `gate`.
async fn use_passphrase(app: &AppHandle, passphrase: String) -> Result<SyncStatus, String> {
    let rt = rt(app);
    let files = with_token!(app, |t| drive::list(&t))?;
    let key = match read_meta(app, &files).await? {
        Some((_, meta)) => unlock(meta, passphrase).await?,
        None => {
            let pass = passphrase.clone();
            let (meta, key) = tauri::async_runtime::spawn_blocking(move || core::new_meta(&pass))
                .await
                .map_err(|e| e.to_string())??;
            let body = serde_json::to_vec(&meta).map_err(|e| e.to_string())?;
            let mine = with_token!(app, |t| drive::upload(&t, None, META_NAME, body.clone()))?;
            // Outro computador criou ao mesmo tempo? Vale o mais antigo.
            let files = with_token!(app, |t| drive::list(&t))?;
            match read_meta(app, &files).await? {
                Some((id, _)) if id == mine => key,
                Some((_, theirs)) => {
                    delete_quietly(app, &mine).await;
                    unlock(theirs, passphrase).await.map_err(|_| {
                        "Outro computador criou a sincronização agora mesmo, com outra frase-senha. Digite a frase usada nele."
                            .to_owned()
                    })?
                }
                None => key,
            }
        }
    };
    // Sobras de criações ao mesmo tempo: só o `meta.json` que vale fica.
    let files = with_token!(app, |t| drive::list(&t))?;
    let chosen = chosen_meta(&files).map(|f| f.id.clone());
    for f in files.iter().filter(|f| f.name == META_NAME) {
        if Some(&f.id) != chosen.as_ref() {
            delete_quietly(app, &f.id).await;
        }
    }
    {
        let mut stored = lock(&rt.stored);
        stored.key = Some(B64.encode(key.bytes()));
        // Primeira rodada com este conjunto: sem base (nada de "conflito").
        stored.base = Organizer::default();
    }
    lock(&rt.cache).clear();
    *lock(&rt.needs) = None;
    save(app)?;
    round(app).await?;
    Ok(status(app))
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<SyncStatus, String> {
    ensure_unlocked(&app)?;
    let rt = rt(&app);
    let _gate = rt.gate.lock().await;
    round(&app).await?;
    Ok(status(&app))
}

/// Esqueceu a frase-senha: apaga tudo da nuvem (os dados deste computador
/// ficam) e recomeça com uma frase nova.
#[tauri::command]
pub async fn sync_reset(app: AppHandle, passphrase: String) -> Result<SyncStatus, String> {
    ensure_unlocked(&app)?;
    if passphrase.chars().count() < core::PASSPHRASE_MIN {
        return Err(format!(
            "Use uma frase-senha com pelo menos {} caracteres.",
            core::PASSPHRASE_MIN
        ));
    }
    let rt = rt(&app);
    let _gate = rt.gate.lock().await;
    let files = with_token!(&app, |t| drive::list(&t))?;
    for f in files {
        let id = f.id.clone();
        with_token!(&app, |t| drive::delete(&t, &id))?;
    }
    lock(&rt.stored).own_file_id = None;
    use_passphrase(&app, passphrase).await.map_err(friendly)
}

/// Para de sincronizar neste computador. `forget`: também apaga o arquivo
/// deste computador da nuvem.
#[tauri::command]
pub async fn sync_disconnect(app: AppHandle, forget: bool) -> Result<SyncStatus, String> {
    ensure_unlocked(&app)?;
    let rt = rt(&app);
    let _gate = rt.gate.lock().await;
    lock(&rt.cache).clear();
    if forget {
        let own = lock(&rt.stored).own_file_id.clone();
        if let Some(id) = own {
            let _ = with_token!(&app, |t| drive::delete(&t, &id));
        }
    }
    let refresh = {
        let mut stored = lock(&rt.stored);
        stored.key = None;
        stored.own_file_id = None;
        stored.base = Organizer::default();
        stored.last_sync = None;
        stored.refresh_token.take()
    };
    if let Some(token) = refresh {
        drive::revoke(&token).await;
    }
    *lock(&rt.access) = None;
    *lock(&rt.needs) = None;
    *lock(&rt.error) = None;
    *lock(&rt.devices) = 0;
    save(&app)?;
    emit(&app);
    Ok(status(&app))
}
