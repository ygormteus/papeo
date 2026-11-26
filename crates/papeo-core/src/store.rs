//! Persistência em `services.json` e backup/restauração da configuração.

use crate::organizer::Organizer;
use crate::service::{Service, ServiceKind};
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub const STORE_VERSION: u32 = 2;
const BACKUP_KIND: &str = "papeo-backup";

/// Tudo o que o app guarda (sem cookies/sessões, que ficam nas pastas de dados).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub services: Vec<Service>,
    pub settings: Settings,
    /// Sessões de serviços removidos cujos dados ainda não puderam ser
    /// apagados (pasta em uso no Windows, armazenamento do macOS). O app tenta
    /// de novo ao abrir. Não entra no backup.
    #[serde(default)]
    pub pending_removal: Vec<String>,
    /// Serviços carregados (não hibernados), para reabri-los ao iniciar o
    /// app. Local deste computador: não entra no backup.
    #[serde(default)]
    pub open: Vec<String>,
    /// PIN do bloqueio (só o hash). Local deste computador: não entra no
    /// backup nem na sincronização.
    #[serde(default)]
    pub lock_pin: Option<crate::LockPin>,
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    /// Presente só em arquivos de backup exportados.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    services: Vec<Service>,
    #[serde(default)]
    settings: Settings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pending_removal: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    open: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lock_pin: Option<crate::LockPin>,
    /// Tarefas e anotações: só nos backups (no app ficam em
    /// `organizer.json`). Backup antigo não traz.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organizer: Option<Organizer>,
}

/// Backup lido e validado.
#[derive(Debug, Clone)]
pub struct Backup {
    pub config: Config,
    /// `None`: backup de uma versão sem tarefas (as atuais ficam).
    pub organizer: Option<Organizer>,
}

fn defaults() -> Config {
    Config {
        services: [ServiceKind::Whatsapp, ServiceKind::Telegram]
            .into_iter()
            .filter_map(|kind| Service::new(kind, None, None).ok())
            .collect(),
        settings: Settings::default(),
        pending_removal: Vec::new(),
        open: Vec::new(),
        lock_pin: None,
    }
}

/// Valida serviços e configurações; descarta ids/sessões repetidos.
/// `strict`: um serviço inválido invalida tudo (backup); senão, só ele é
/// descartado (arquivo do próprio app, para não perder o resto).
/// Retorna também quantos serviços inválidos foram descartados.
fn sanitize(file: StoreFile, strict: bool) -> Result<(Config, usize), String> {
    if file.version > STORE_VERSION {
        return Err(format!(
            "Arquivo de uma versão mais nova do Papeo (formato {}).",
            file.version
        ));
    }
    let mut ids = HashSet::new();
    let mut sessions = HashSet::new();
    let mut services = Vec::with_capacity(file.services.len());
    let mut dropped = 0;
    for service in file.services {
        let service = match service.sanitized() {
            Ok(service) => service,
            Err(err) if !strict => {
                eprintln!("papeo: serviço ignorado em services.json ({err})");
                dropped += 1;
                continue;
            }
            Err(err) => return Err(err),
        };
        if ids.insert(service.id.clone()) && sessions.insert(service.session.clone()) {
            services.push(service);
        }
    }
    // Só sessões no formato do app e que nenhum serviço atual usa.
    let mut pending_removal: Vec<String> = file
        .pending_removal
        .into_iter()
        .filter(|s| crate::is_session_id(s) && !sessions.contains(s))
        .collect();
    pending_removal.sort();
    pending_removal.dedup();
    // Só serviços que existem, sem repetir.
    let mut open = Vec::new();
    for id in file.open {
        if ids.contains(&id) && !open.contains(&id) {
            open.push(id);
        }
    }
    let config = Config {
        services,
        settings: file.settings.sanitized(),
        pending_removal,
        open,
        // Backup nunca traz PIN (`to_json` não o exporta); um adulterado
        // também não entra.
        lock_pin: if strict {
            None
        } else {
            file.lock_pin.and_then(crate::LockPin::sanitized)
        },
    };
    Ok((config, dropped))
}

/// Lê o arquivo, tentando de novo por um instante se ele estiver preso
/// (antivírus ou indexador com o arquivo aberto, no Windows).
fn read_retrying(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut tries = 0;
    loop {
        match fs::read(path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound && tries < 5 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            result => return result,
        }
    }
}

/// Onde guardar um arquivo ilegível ou inválido: `.corrompido`, ou com a
/// hora no nome se já houver um (o mais antigo não é substituído).
fn kept_path(path: &Path) -> std::path::PathBuf {
    let kept = path.with_extension("json.corrompido");
    if !kept.exists() {
        return kept;
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    path.with_extension(format!("json.corrompido-{secs}"))
}

/// Lê a configuração. Na primeira execução, cria WhatsApp e Telegram.
/// Um arquivo ilegível ou inválido é preservado como `.corrompido`.
pub fn load(path: &Path) -> Config {
    load_guarded(path).0
}

/// Como `load`, dizendo também se dá para gravar em `path`. `false`: o
/// arquivo existe, não abre e não pôde ser tirado do caminho (preso por
/// outro programa); gravar ali substituiria as contas reais pelos padrões.
pub fn load_guarded(path: &Path) -> (Config, bool) {
    let bytes = match read_retrying(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return (defaults(), true),
        // Existe mas não abre: tira do caminho antes de seguir com os
        // padrões, senão a próxima gravação substituiria as contas reais.
        Err(err) => {
            eprintln!("papeo: não foi possível ler services.json ({err}); usando padrões");
            let moved = fs::rename(path, kept_path(path)).is_ok();
            return (defaults(), moved);
        }
    };
    let parsed = serde_json::from_slice::<StoreFile>(&bytes)
        .map_err(|e| e.to_string())
        .and_then(|file| sanitize(file, false));
    match parsed {
        Ok((config, dropped)) => {
            if dropped > 0 {
                // Guarda o original antes que o app regrave sem os descartados.
                let _ = fs::copy(path, kept_path(path));
            }
            (config, true)
        }
        Err(err) => {
            eprintln!("papeo: services.json inválido ({err}); usando padrões");
            let moved = fs::rename(path, kept_path(path)).is_ok();
            (defaults(), moved)
        }
    }
}

fn to_json(
    config: &Config,
    kind: Option<&str>,
    organizer: Option<&Organizer>,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&StoreFile {
        organizer: organizer.cloned(),
        version: STORE_VERSION,
        kind: kind.map(str::to_owned),
        services: config.services.clone(),
        settings: config.settings.clone(),
        // Limpeza pendente e serviços abertos são locais deste computador.
        pending_removal: if kind.is_some() {
            Vec::new()
        } else {
            config.pending_removal.clone()
        },
        open: if kind.is_some() {
            Vec::new()
        } else {
            config.open.clone()
        },
        // O PIN é deste computador: nunca vai para o backup.
        lock_pin: if kind.is_some() {
            None
        } else {
            config.lock_pin.clone()
        },
    })
    .map_err(|e| e.to_string())
}

/// Grava de forma atômica (arquivo temporário + rename).
pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    write_atomic(path, &encode_config(config)?)
}

/// Conteúdo de `config.json` (para gravar com `write_file`).
pub fn encode_config(config: &Config) -> Result<Vec<u8>, String> {
    to_json(config, None, None)
}

/// Grava inteiro ou nada (arquivo temporário, `fsync` e troca).
pub fn write_atomic(path: &Path, body: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp-papeo");
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(body).map_err(|e| e.to_string())?;
        // No disco antes de trocar: queda de energia não deixa arquivo vazio.
        file.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Exporta serviços, cores, ordem e configurações. Não inclui cookies nem
/// conversas: em outro computador, cada serviço pede login de novo.
/// Tarefas e anotações vão junto (sem os registros apagados).
pub fn export_backup(path: &Path, config: &Config, organizer: &Organizer) -> Result<(), String> {
    write_atomic(
        path,
        &to_json(config, Some(BACKUP_KIND), Some(&organizer.visible()))?,
    )
}

/// Lê e valida um backup. Não altera nada no disco.
pub fn read_backup(path: &Path, now_ms: u64) -> Result<Backup, String> {
    let meta = fs::metadata(path).map_err(|e| format!("Não foi possível ler o arquivo: {e}"))?;
    if meta.len() > 64 * 1024 * 1024 {
        return Err("Arquivo grande demais para ser um backup do Papeo.".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("Não foi possível ler o arquivo: {e}"))?;
    let mut file: StoreFile = serde_json::from_slice(&bytes)
        .map_err(|_| "Este arquivo não é um backup válido do Papeo.".to_owned())?;
    if file.kind.as_deref() != Some(BACKUP_KIND) {
        return Err("Este arquivo não é um backup do Papeo.".into());
    }
    let organizer = file.organizer.take().map(|o| o.sanitized(now_ms));
    sanitize(file, true)
        .map(|(config, _)| Backup { config, organizer })
        .map_err(|e| format!("Backup inválido: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::random_hex;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("papeo-test-{}", random_hex()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_and_corruption() {
        let dir = temp_dir();
        let path = dir.join("services.json");
        let first = load(&path);
        assert_eq!(first.services.len(), 2);
        save(&path, &first).unwrap();
        assert_eq!(load(&path).services[0].id, first.services[0].id);
        fs::write(&path, b"{oops").unwrap();
        assert_eq!(load(&path).services.len(), 2);
        assert!(path.with_extension("json.corrompido").exists());
        // Outro arquivo inválido não substitui o guardado antes.
        fs::write(&path, b"{oops2").unwrap();
        assert_eq!(load(&path).services.len(), 2);
        assert_eq!(
            fs::read(path.with_extension("json.corrompido")).unwrap(),
            b"{oops"
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    /// Arquivo que existe mas não abre: as contas reais saem do caminho
    /// antes que a próxima gravação as substitua pelos padrões.
    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.join("services.json");
        save(&path, &defaults()).unwrap();
        let original = fs::read(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&path).is_ok() {
            // Rodando como root: a permissão não impede a leitura.
            let _ = fs::remove_dir_all(dir);
            return;
        }
        assert_eq!(load(&path).services.len(), 2);
        assert!(!path.exists());
        let kept = path.with_extension("json.corrompido");
        fs::set_permissions(&kept, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read(&kept).unwrap(), original);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn lock_pin_stays_local() {
        let dir = temp_dir();
        let mut config = defaults();
        config.lock_pin = Some(crate::LockPin::new("2468").unwrap());
        config.lock_pin.as_mut().unwrap().failures = 3;
        let path = dir.join("services.json");
        save(&path, &config).unwrap();
        let loaded = load(&path).lock_pin.unwrap();
        assert!(loaded.verify("2468"));
        assert_eq!(loaded.failures, 3);
        // Backup não leva o PIN, nem o aceita de um arquivo adulterado.
        let backup = dir.join("backup.json");
        export_backup(&backup, &config, &Organizer::default()).unwrap();
        assert!(!fs::read_to_string(&backup).unwrap().contains("lockPin"));
        assert!(!fs::read_to_string(&backup).unwrap().contains("lock_pin"));
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&backup).unwrap()).unwrap();
        json["lock_pin"] = serde_json::to_value(&config.lock_pin).unwrap();
        fs::write(&backup, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(read_backup(&backup, 0).unwrap().config.lock_pin.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn v1_file_without_settings_loads() {
        let dir = temp_dir();
        let path = dir.join("services.json");
        let svc = Service::new(ServiceKind::Telegram, None, None).unwrap();
        let v1 = serde_json::json!({ "version": 1, "services": [svc] });
        fs::write(&path, serde_json::to_vec(&v1).unwrap()).unwrap();
        let config = load(&path);
        assert_eq!(config.services.len(), 1);
        assert!(config.settings.close_to_tray);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn backup_roundtrip_and_validation() {
        let dir = temp_dir();
        let mut config = defaults();
        config.services[0].name = "Pessoal".into();
        config.settings.quiet_hours.enabled = true;
        let path = dir.join("backup.json");
        export_backup(&path, &config, &Organizer::default()).unwrap();
        let restored = read_backup(&path, 0).unwrap().config;
        assert_eq!(restored.services[0].name, "Pessoal");
        assert!(restored.settings.quiet_hours.enabled);

        // services.json comum não é aceito como backup.
        let plain = dir.join("services.json");
        save(&plain, &config).unwrap();
        assert!(read_backup(&plain, 0).is_err());

        // Sessão adulterada é rejeitada.
        let mut tampered: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        tampered["services"][0]["session"] = "../../x".into();
        fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(read_backup(&path, 0).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn one_bad_service_does_not_wipe_the_file() {
        let dir = temp_dir();
        let path = dir.join("services.json");
        let mut config = defaults();
        let mut bad = config.services[0].clone();
        bad.id = "svc-../../x".into();
        bad.session = "f".repeat(32);
        config.services.push(bad);
        save(&path, &config).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.services.len(), 2);
        // O original fica guardado; o arquivo em uso não é renomeado.
        assert!(path.with_extension("json.corrompido").exists());
        assert!(path.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_removal_is_local_and_validated() {
        let dir = temp_dir();
        let mut config = defaults();
        let in_use = config.services[0].session.clone();
        config.pending_removal = vec![crate::new_session(), in_use, "../../x".into()];
        let path = dir.join("services.json");
        save(&path, &config).unwrap();
        assert_eq!(load(&path).pending_removal.len(), 1);
        let backup = dir.join("backup.json");
        export_backup(&backup, &config, &Organizer::default()).unwrap();
        assert!(read_backup(&backup, 0)
            .unwrap()
            .config
            .pending_removal
            .is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn open_services_are_local_and_validated() {
        let dir = temp_dir();
        let mut config = defaults();
        let first = config.services[0].id.clone();
        config.open = vec![first.clone(), "svc-nao-existe".into(), first.clone()];
        let path = dir.join("services.json");
        save(&path, &config).unwrap();
        assert_eq!(load(&path).open, vec![first]);
        let backup = dir.join("backup.json");
        export_backup(&backup, &config, &Organizer::default()).unwrap();
        assert!(read_backup(&backup, 0).unwrap().config.open.is_empty());
        // Arquivo antigo, sem o campo.
        fs::write(&path, br#"{"version":2,"services":[]}"#).unwrap();
        assert!(load(&path).open.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_ids_are_dropped() {
        let svc = Service::new(ServiceKind::Whatsapp, None, None).unwrap();
        let file = StoreFile {
            version: STORE_VERSION,
            kind: None,
            services: vec![svc.clone(), svc],
            settings: Settings::default(),
            pending_removal: Vec::new(),
            open: Vec::new(),
            lock_pin: None,
            organizer: None,
        };
        assert_eq!(sanitize(file, true).unwrap().0.services.len(), 1);
    }

    #[test]
    fn newer_version_is_rejected() {
        let file = StoreFile {
            version: STORE_VERSION + 1,
            kind: None,
            services: vec![],
            settings: Settings::default(),
            pending_removal: Vec::new(),
            open: Vec::new(),
            lock_pin: None,
            organizer: None,
        };
        assert!(sanitize(file, false).is_err());
    }

    #[test]
    fn backup_carries_tasks_and_notes() {
        use crate::organizer::{ContactRef, TaskInput};
        let dir = temp_dir();
        let config = defaults();
        let mut org = Organizer::default();
        let now = 1_800_000_000_000;
        let gone = org
            .add_task(
                &TaskInput {
                    text: "apagada".into(),
                    ..Default::default()
                },
                now,
            )
            .unwrap();
        org.delete_task(&gone, now).unwrap();
        org.add_task(
            &TaskInput {
                text: "ligar".into(),
                ..Default::default()
            },
            now,
        )
        .unwrap();
        let r = ContactRef {
            service_id: config.services[0].id.clone(),
            service_kind: "whatsapp".into(),
            service_name: "WhatsApp".into(),
            chat: "Ana".into(),
        };
        org.set_note(&r, "cliente desde 2020", now).unwrap();
        let path = dir.join("backup.json");
        export_backup(&path, &config, &org).unwrap();
        let back = read_backup(&path, now).unwrap().organizer.unwrap();
        assert_eq!(back.tasks.len(), 1);
        assert_eq!(back.tasks[0].text, "ligar");
        assert_eq!(back.contacts[0].note, "cliente desde 2020");
        // Backup de versão antiga: sem tarefas.
        let mut old: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        old.as_object_mut().unwrap().remove("organizer");
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(read_backup(&path, now).unwrap().organizer.is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
