/// Comandos do app. Com o manifesto, cada um só fica disponível para quem a
/// capability liberar: a interface recebe todos os outros; os sites dos
/// serviços, só `service_notify` e `service_status` (ver `capabilities/`).
const COMMANDS: &[&str] = &[
    "get_state",
    "add_service",
    "remove_service",
    "customize_service",
    "move_service",
    "set_hibernation",
    "set_notifications",
    "mute_service",
    "set_media_policy",
    "set_zoom",
    "activate_service",
    "hibernate_service",
    "reload_service",
    "set_content_area",
    "set_overlay",
    "set_dnd",
    "set_quiet_hours",
    "set_preferences",
    "set_autostart",
    "set_downloads_dir",
    "reveal_download",
    "open_downloads_dir",
    "clear_downloads",
    "get_memory",
    "export_config",
    "preview_backup",
    "import_config",
    "clear_service_data",
    "get_orphan_sessions",
    "clean_orphan_sessions",
    "set_lock",
    "lock_now",
    "unlock",
    "set_appearance",
    "check_update",
    "install_update",
    "set_updates",
    "service_notify",
    "service_status",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("falha ao preparar o build do Tauri");
}
