//! Núcleo do Papeo: modelo dos serviços, configurações, persistência e regras
//! puras (não perturbe, nomes de arquivo, não lidas). Não depende do Tauri,
//! para ser testado em qualquer máquina.

mod privacy;
mod service;
mod settings;
mod store;
mod util;

pub use privacy::{
    is_session_id, new_session, notification_text, same_site, session_from_bytes,
    NotificationPreview,
};
pub use service::{
    clamp_zoom, normalize_color, normalize_emoji, normalize_name, unique_name, MediaPolicy,
    Service, ServiceKind, ZOOM_MAX, ZOOM_MIN,
};
pub use settings::{
    should_notify, AppLock, Appearance, QuietHours, Settings, UpdatePrefs, DND_FOREVER,
    LOCK_MAX_MINUTES,
};
pub use store::{export_backup, load, read_backup, save, Config, STORE_VERSION};
pub use util::{clip, parse_unread, safe_file_name, unique_path};
