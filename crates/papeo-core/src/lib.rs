//! Núcleo do Papeo: modelo dos serviços, configurações, persistência e regras
//! puras (não perturbe, nomes de arquivo, não lidas). Não depende do Tauri,
//! para ser testado em qualquer máquina.

pub mod mail;
pub mod mailbox;
mod organizer;
mod pin;
mod privacy;
mod service;
mod settings;
mod store;
pub mod sync;
mod util;

pub use organizer::{
    clean_chat, encode_organizer, is_date, is_item_id, load_organizer, new_item_id, save_organizer,
    Contact, ContactRef, Organizer, Repeat, Task, TaskInput, CHAT_MAX, NOTE_MAX, TASK_TEXT_MAX,
};
pub use pin::{check_pin, pin_wait_secs, LockPin, PIN_MAX, PIN_MIN};
pub use privacy::{
    is_session_id, new_session, notification_text, recent_text, same_site, session_from_bytes,
    NotificationPreview,
};
pub use service::{
    clamp_zoom, normalize_color, normalize_emoji, normalize_name, unique_name, MediaPolicy,
    Service, ServiceContext, ServiceKind, ZOOM_MAX, ZOOM_MIN,
};
pub use settings::{
    should_notify, AppLock, Appearance, AttachmentMode, LinkTarget, Mode, ModeChoice, ModeOverride,
    QuickReply, QuietHours, Settings, UpdatePrefs, VoiceCapture, WorkHours, DND_FOREVER,
    LOCK_MAX_MINUTES,
};
pub use store::{
    encode_config, export_backup, load, load_guarded, read_backup, save, write_atomic, Backup,
    Config, STORE_VERSION,
};
pub use util::{attachment_view, clip, parse_unread, safe_file_name, unique_path, AttachmentView};
