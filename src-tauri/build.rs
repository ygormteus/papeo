/// Comandos do app. Com o manifesto, cada um só fica disponível para quem a
/// capability liberar: a interface recebe todos; os sites dos serviços, só
/// `service_notify` (ver `capabilities/`).
const COMMANDS: &[&str] = &[
    "get_state",
    "add_service",
    "remove_service",
    "customize_service",
    "move_service",
    "set_hibernation",
    "set_notifications",
    "activate_service",
    "hibernate_service",
    "reload_service",
    "set_content_area",
    "set_overlay",
    "service_notify",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("falha ao preparar o build do Tauri");
}
