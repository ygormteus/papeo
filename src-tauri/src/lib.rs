// Autoteste: bandeja, manutenção e sincronização ficam de fora (sem uso).
#![cfg_attr(feature = "selftest", allow(dead_code, unused_imports))]

mod attachment;
mod commands;
mod downloads;
mod drive;
mod idle;
mod lang;
mod lock;
mod mail;
mod mailbox;
mod memory;
#[cfg(target_os = "macos")]
mod menu;
mod microsoft;
mod notifications;
mod notify;
mod organizer;
mod outbox;
mod popover;
mod preview;
mod reply;
mod secrets;
#[cfg(feature = "selftest")]
mod selftest;
mod sessions;
mod silence;
mod state;
mod sync;
mod tray;
mod updater;
#[cfg(target_os = "macos")]
mod webcrypto;
mod webviews;

use state::{publish, status_of, AppState, Inner, MAIN_WINDOW, UI_WEBVIEW};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Argumento usado pelo início automático com o sistema.
const HIDDEN_ARG: &str = "--hidden";

/// Só recarrega serviço que está em segundo plano há este tempo…
const RELOAD_IDLE: Duration = Duration::from_secs(10 * 60);
/// …e no máximo uma vez neste intervalo.
const RELOAD_COOLDOWN: Duration = Duration::from_secs(60 * 60);
/// Computador sem uso há este tempo: a pessoa saiu.
const AWAY_AFTER: Duration = Duration::from_secs(20 * 60);
/// Com a pessoa fora, o limite cai para a metade (nunca abaixo disto), para
/// a conta estar leve quando ela voltar.
const AWAY_RELOAD_FLOOR: u64 = 1024 * 1024 * 1024;
/// Chamada ou áudio/vídeo há menos que isto: não recarrega. O sinal de vida
/// vem a cada 20 s (mais devagar com o serviço escondido).
const MEDIA_GRACE: Duration = Duration::from_secs(3 * 60);
/// Relógio de parede andou mais que isto entre duas voltas de 15 s: o
/// computador dormiu. Sites que ficam tanto tempo congelados (o WhatsApp
/// Web, por exemplo) costumam não conseguir reconectar sozinhos.
const SLEPT_AFTER: Duration = Duration::from_secs(5 * 60);

/// O computador dormiu entre `before` e `now`? Usa o relógio de parede:
/// no macOS o `Instant` para durante o repouso. Relógio que volta (ajuste
/// manual ou NTP) não conta.
fn slept(before: std::time::SystemTime, now: std::time::SystemTime) -> bool {
    now.duration_since(before)
        .is_ok_and(|gap| gap >= SLEPT_AFTER)
}

/// Pessoa de volta ao computador: mexeu nele há menos que isto, com a tela
/// acesa. As acordadas de manutenção do repouso (Mac de tampa fechada, por
/// exemplo) não contam: recarregar ali, quase sempre sem rede e com o Mac
/// dormindo de novo no meio, deixava o WhatsApp Web preso carregando.
const PRESENT_IDLE: Duration = Duration::from_secs(2 * 60);
/// Com a pessoa de volta, espera a rede até este número de voltas de 15 s;
/// depois reabre assim mesmo (rede que só passa por proxy, por exemplo).
const RESUME_MAX_WAIT_TICKS: u32 = 8;
/// Tempo máximo para checar se os sites das contas respondem.
const NET_CHECK: Duration = Duration::from_secs(4);

/// Reabre as contas na volta do repouso: a conta à vista na hora; as outras
/// espaçadas, como ao abrir o app, para não disputarem CPU e rede. Num
/// thread à parte, para não atrasar bloqueio, sincronização e o resto da
/// manutenção. Conta aberta pela pessoa enquanto esperava fica como está
/// (se travou, o Recarregar dela também reabre do zero).
fn resume_restart(app: &AppHandle, ids: Vec<String>) {
    // Outra volta do repouso no meio desta (tampa fechada e aberta de novo):
    // só a mais nova continua, sem reabrir a mesma conta duas vezes.
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let active = app.state::<AppState>().lock().active.clone();
        let (first, rest): (Vec<_>, Vec<_>) =
            ids.into_iter().partition(|id| Some(id) == active.as_ref());
        for (n, id) in first.into_iter().chain(rest).enumerate() {
            if n > 0 {
                std::thread::sleep(webviews::RESTORE_GAP);
                if GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation {
                    return;
                }
                let used = {
                    let state = app.state::<AppState>();
                    let inner = state.lock();
                    inner.active.as_deref() == Some(id.as_str())
                        || inner.last_used.get(&id).is_some_and(|t| *t > started)
                };
                if used {
                    continue;
                }
            }
            if webviews::restart(&app, &id).unwrap_or(false) {
                publish(&app);
            }
        }
    });
}

fn user_present() -> bool {
    !idle::display_asleep() && idle::system_idle().is_none_or(|t| t < PRESENT_IDLE)
}

/// Algum dos sites responde (conexão TCP na porta do site)? Cada um num
/// thread, com tempo máximo: sem rede, a resolução de nomes pode demorar.
fn reachable(app: &AppHandle, ids: &[String]) -> bool {
    let mut hosts: Vec<(String, u16)> = {
        let state = app.state::<AppState>();
        let inner = state.lock();
        ids.iter()
            .filter_map(|id| inner.service(id).ok())
            .filter_map(|s| url::Url::parse(&s.url).ok())
            .filter_map(|u| Some((u.host_str()?.to_owned(), u.port_or_known_default()?)))
            .collect()
    };
    hosts.sort();
    hosts.dedup();
    hosts.truncate(4);
    if hosts.is_empty() {
        return true;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    for (host, port) in hosts {
        let tx = tx.clone();
        std::thread::spawn(move || {
            use std::net::{TcpStream, ToSocketAddrs};
            let ok = (host.as_str(), port)
                .to_socket_addrs()
                .ok()
                .and_then(|mut addrs| addrs.next())
                .is_some_and(|addr| TcpStream::connect_timeout(&addr, NET_CHECK).is_ok());
            let _ = tx.send(ok);
        });
    }
    drop(tx);
    let deadline = std::time::Instant::now() + NET_CHECK;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(true) => return true,
            Ok(false) => {}
            Err(_) => return false,
        }
    }
    false
}

/// Contas carregadas que podem ser recarregadas na volta do repouso: todas,
/// menos as com download em curso.
fn resume_due(app: &AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let inner = state.lock();
    inner
        .config
        .services
        .iter()
        .map(|s| s.id.clone())
        .filter(|id| app.get_webview(id).is_some())
        .filter(|id| {
            !inner
                .downloads
                .iter()
                .any(|d| &d.service_id == id && d.state == state::DownloadState::Downloading)
        })
        .collect()
}

/// Serviços acima do limite de memória que podem ser recarregados sem
/// atrapalhar: fora de vista, parados há um tempo e sem download em curso.
/// `away`: computador sem uso; o limite cai para a metade (mínimo 1 GB).
fn reload_due(app: &AppHandle, window_visible: bool, away: bool) -> Vec<String> {
    let state = app.state::<AppState>();
    let limit_mb = state.lock().config.settings.reload_above_mb;
    if limit_mb == 0 {
        return Vec::new();
    }
    // Fora do lock: a medição espera a thread principal.
    let report = memory::report(app);
    let mut limit = u64::from(limit_mb) * 1024 * 1024;
    if away {
        limit = limit.min((limit / 2).max(AWAY_RELOAD_FLOOR));
    }
    let inner = state.lock();
    report
        .services
        .into_iter()
        .filter(|(_, bytes)| *bytes > limit)
        .map(|(id, _)| id)
        .filter(|id| !window_visible || inner.active.as_deref() != Some(id.as_str()))
        .filter(|id| {
            inner
                .last_used
                .get(id)
                .is_none_or(|t| t.elapsed() >= RELOAD_IDLE)
        })
        .filter(|id| {
            inner
                .last_reload
                .get(id)
                .is_none_or(|t| t.elapsed() >= RELOAD_COOLDOWN)
        })
        .filter(|id| {
            inner
                .health
                .get(id)
                .and_then(|h| h.media_at)
                .is_none_or(|t| t.elapsed() >= MEDIA_GRACE)
        })
        .filter(|id| {
            !inner
                .downloads
                .iter()
                .any(|d| &d.service_id == id && d.state == state::DownloadState::Downloading)
        })
        .collect()
}

/// A cada 15 s: hiberna serviços parados, reavalia "não responde", aplica o
/// bloqueio automático e agenda a verificação de atualizações. A cada
/// minuto, recarrega serviços que passaram do limite de memória. Na volta
/// do repouso do computador, reabre do zero as contas abertas quando a
/// pessoa está de volta e os sites respondem (ou após ~2 min esperando a
/// rede), nunca nas acordadas de manutenção com a tela apagada.
fn spawn_maintenance(app: AppHandle) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut last_status = HashMap::new();
        let mut last_mode = None;
        let mut tick: u64 = 0;
        let mut wall = std::time::SystemTime::now();
        let mut resume_pending = false;
        // Voltas com a pessoa presente esperando a rede para reabrir.
        let mut resume_waited: u32 = 0;
        loop {
            std::thread::sleep(Duration::from_secs(15));
            tick += 1;
            let now = std::time::SystemTime::now();
            let mut woke = false;
            // Uma volta depois de acordar, no mínimo, para a rede voltar.
            if resume_pending && user_present() {
                let ids = resume_due(&app);
                resume_waited += 1;
                if resume_waited > RESUME_MAX_WAIT_TICKS || reachable(&app, &ids) {
                    resume_pending = false;
                    resume_waited = 0;
                    woke = true;
                    resume_restart(&app, ids);
                }
            }
            if slept(wall, now) {
                resume_pending = true;
                resume_waited = 0;
                // Conexões IMAP de antes do repouso costumam estar mortas.
                mail::kick(&app);
            }
            wall = now;
            // Saída do não perturbe/horário silencioso: resumo (spec 17).
            silence::check(&app);
            let state = app.state::<AppState>();
            let (due, status, mode) = {
                let mut inner = state.lock();
                // A troca manual do automático acaba quando o expediente
                // começa ou termina.
                if inner
                    .mode_override
                    .is_some_and(|o| !o.holds(state::now_secs()))
                {
                    inner.mode_override = None;
                }
                let mode = inner.current_mode();
                let sleep_outside = inner.config.settings.sleep_outside_mode;
                // Conta com o IMAP avisando e pedindo para o site dormir.
                let mail_sleeps = |s: &papeo_core::Service| {
                    s.mail.as_ref().is_some_and(|m| m.sleep_site)
                        && inner.mail_live(&s.id)
                        && inner
                            .last_used
                            .get(&s.id)
                            .is_none_or(|t| t.elapsed() >= mail::SLEEP_SITE_AFTER)
                };
                let due: Vec<String> = inner
                    .config
                    .services
                    .iter()
                    .filter(|s| inner.active.as_deref() != Some(s.id.as_str()))
                    .filter(|s| {
                        mail_sleeps(s)
                            || (sleep_outside && !mode.allows(s.context))
                            || (s.hibernate_after_minutes > 0
                                && inner.last_used.get(&s.id).is_some_and(|t| {
                                    t.elapsed()
                                        >= Duration::from_secs(
                                            u64::from(s.hibernate_after_minutes) * 60,
                                        )
                                }))
                    })
                    .filter(|s| app.get_webview(&s.id).is_some())
                    // Hibernar interromperia o download.
                    .filter(|s| {
                        !inner.downloads.iter().any(|d| {
                            d.service_id == s.id && d.state == state::DownloadState::Downloading
                        })
                    })
                    .map(|s| s.id.clone())
                    .collect();
                let status: HashMap<String, _> = inner
                    .config
                    .services
                    .iter()
                    .map(|s| {
                        let loaded = app.get_webview(&s.id).is_some();
                        (s.id.clone(), status_of(&inner, &s.id, loaded))
                    })
                    .collect();
                (due, status, mode)
            };
            for id in &due {
                webviews::hibernate(&app, id);
            }
            if !due.is_empty() || status != last_status || Some(mode) != last_mode {
                last_status = status;
                last_mode = Some(mode);
                publish(&app);
            }
            // Fora do lock: `is_visible` espera a thread principal.
            // Minimizada conta como escondida (no Windows ela segue "visível").
            let visible = app.get_window(MAIN_WINDOW).is_none_or(|w| {
                w.is_visible().unwrap_or(true) && !w.is_minimized().unwrap_or(false)
            });
            let lock_due = {
                let inner = state.lock();
                let lock = inner.config.settings.lock;
                lock.enabled
                    && !inner.locked
                    && match lock.after_minutes {
                        0 => !visible,
                        n => inner
                            .unfocused_since
                            .is_some_and(|t| t.elapsed() >= Duration::from_secs(u64::from(n) * 60)),
                    }
            };
            if lock_due {
                lock::lock_app(&app);
            }
            if tick % 4 == 0 {
                let away = idle::system_idle().is_some_and(|t| t >= AWAY_AFTER);
                // Reabrir do zero: a recarga simples mantém o processo do
                // site, e a memória quase não baixa.
                for id in reload_due(&app, visible, away) {
                    if webviews::restart(&app, &id).unwrap_or(false) {
                        state
                            .lock()
                            .last_reload
                            .insert(id, std::time::Instant::now());
                    }
                }
            }
            organizer::tick(&app);
            sync::tick(&app, woke);
            updater::tick(&app, started);
        }
    });
}

/// Tema da janela nativa conforme a aparência escolhida (`None` = sistema).
/// As webviews dos sites herdam esse tema como `prefers-color-scheme`.
pub fn window_theme(appearance: papeo_core::Appearance) -> Option<tauri::Theme> {
    appearance.dark().map(|dark| {
        if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }
    })
}

fn build_main_window(
    app: &AppHandle,
    visible: bool,
    protected: bool,
    appearance: papeo_core::Appearance,
) -> tauri::Result<()> {
    let builder = tauri::window::WindowBuilder::new(app, MAIN_WINDOW)
        .title("Papeo")
        .inner_size(1280.0, 820.0)
        .min_inner_size(960.0, 600.0)
        // Fora de capturas e do compartilhamento de tela, se pedido.
        .content_protected(protected)
        .theme(window_theme(appearance))
        .visible(visible);
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    let window = builder.build()?;
    let size = window
        .inner_size()?
        .to_logical::<f64>(window.scale_factor()?);
    window.add_child(
        WebviewBuilder::new(UI_WEBVIEW, WebviewUrl::App(Default::default()))
            .auto_resize()
            // Arrastar para reordenar usa o drag and drop do HTML.
            .disable_drag_drop_handler(),
        LogicalPosition::new(0.0, 0.0),
        size,
    )?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Sem o script que o plugin injeta em toda webview para abrir links
        // de nova aba pelo app: nas contas (sem permissão para o plugin) ele
        // cancelava o clique e o link não abria. Os links das contas vão pelo
        // `on_new_window` (navegador ou prévia).
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .setup(|app| {
            let handle = app.handle();
            // Autoteste: tudo numa pasta temporária, nada dos dados reais.
            #[cfg(feature = "selftest")]
            let (config_dir, data_dir) = (selftest::base_dir(), selftest::base_dir());
            #[cfg(not(feature = "selftest"))]
            let (config_dir, data_dir) = (app.path().app_config_dir()?, app.path().app_data_dir()?);
            let mut store_path = config_dir.join("services.json");
            let (config, writable) = papeo_core::load_guarded(&store_path);
            // services.json existe, não abre e não saiu do caminho (preso por
            // outro programa): esta sessão grava ao lado, sem substituir as
            // contas reais; ao reabrir, o app lê o original de novo.
            if !writable {
                store_path = config_dir.join(state::FALLBACK_STORE);
            }
            let started_hidden =
                config.settings.start_hidden && std::env::args().any(|a| a == HIDDEN_ARG);
            let protected = config.settings.protect_screen;
            let appearance = config.settings.appearance;
            lang::apply(config.settings.language);
            let organizer_path = config_dir.join("organizer.json");
            let organizer = papeo_core::load_organizer(&organizer_path, organizer::now_ms());
            let state = AppState {
                store_path,
                organizer_path,
                sessions_dir: data_dir.join("sessions"),
                inner: Mutex::new(Inner::new(config)),
                config_disk: Default::default(),
                organizer_disk: Default::default(),
            };
            {
                let mut inner = state.lock();
                match organizer {
                    Ok(organizer) => inner.organizer = organizer,
                    Err(err) => {
                        eprintln!("papeo: {err}");
                        inner.organizer_error = Some(err);
                    }
                }
                inner.autostart = handle.autolaunch().is_enabled().unwrap_or(false);
                inner.window_focused = !started_hidden;
                // Já em silêncio ao abrir: a contagem do resumo começa agora.
                let (now, minute) = (state::now_secs(), state::local_minute_of_day());
                if inner.config.settings.dnd_active(now, minute) {
                    inner.silence_active = true;
                    inner.silence.start(now);
                }
                if started_hidden {
                    inner.unfocused_since = Some(std::time::Instant::now());
                }
                state.persist(&inner)?;
            }
            app.manage(state);
            build_main_window(handle, !started_hidden, protected, appearance)?;
            // Autoteste: só a janela e a conta de teste; sem chaveiro, bandeja,
            // sincronização ou manutenção.
            #[cfg(feature = "selftest")]
            selftest::start(handle);
            #[cfg(not(feature = "selftest"))]
            {
                // Chave do WebCrypto fora das Chaves do macOS (antes de qualquer
                // webview de serviço): sem pedido de senha a cada atualização.
                #[cfg(target_os = "macos")]
                webcrypto::init(&app.path().app_data_dir()?, &app.config().identifier);
                // Contas que estavam carregadas ao fechar voltam carregadas.
                webviews::restore_open(handle);
                lock::check_availability(handle);
                // Anexos visualizados numa sessão anterior (pasta temporária).
                attachment::clear_temp(handle);
                // Termina de apagar sessões de serviços removidos (nada está aberto ainda).
                let cleanup = handle.clone();
                tauri::async_runtime::spawn(async move {
                    sessions::retry_pending(&cleanup).await;
                });
                sync::init(handle, app.path().app_local_data_dir()?);
                secrets::init(handle, &app.path().app_local_data_dir()?);
                mail::init(handle);
                mailbox::init(handle);
                outbox::init(handle, &app.path().app_local_data_dir()?);
                notifications::init(handle);
                tray::install(handle)?;
                #[cfg(target_os = "macos")]
                menu::install(handle)?;
                spawn_maintenance(handle.clone());
                // Marca deixada por uma atualização: sem ela, a próxima abertura
                // não cai na cópia temporária do Gatekeeper.
                std::thread::spawn(updater::clear_quarantine);
            }
            Ok(())
        })
        .on_window_event(tray::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::add_service,
            commands::remove_service,
            commands::customize_service,
            commands::move_service,
            commands::set_hibernation,
            commands::set_mail_account,
            commands::mail_microsoft_login,
            commands::mail_microsoft_cancel,
            commands::mail_folders,
            commands::mail_list,
            commands::mail_open,
            commands::mail_set_flag,
            commands::mail_move,
            commands::mail_delete,
            commands::mail_save_attachment,
            commands::mail_save_attachments,
            commands::mail_preview_attachment,
            commands::mail_signature,
            commands::mail_send,
            commands::mail_save_draft,
            commands::mail_discard_draft,
            commands::mail_open_draft,
            commands::mail_send_later,
            commands::mail_send_cancel,
            commands::mail_unsent,
            commands::mail_unsent_taken,
            commands::dismiss_silence_summary,
            commands::set_mail_signature,
            commands::mail_show_site,
            commands::mail_reveal_file,
            commands::set_notifications,
            commands::mute_service,
            commands::set_media_policy,
            commands::set_zoom,
            commands::activate_service,
            commands::hibernate_service,
            commands::open_chat,
            commands::send_reply,
            commands::show_app,
            commands::hide_popover,
            commands::reload_service,
            commands::set_content_area,
            commands::set_overlay,
            commands::set_dnd,
            commands::set_quiet_hours,
            commands::set_mode,
            commands::set_mode_schedule,
            commands::set_preferences,
            commands::set_quick_replies,
            commands::set_autostart,
            commands::set_downloads_dir,
            commands::reveal_download,
            commands::open_downloads_dir,
            commands::clear_downloads,
            commands::get_memory,
            commands::export_config,
            commands::preview_backup,
            commands::import_config,
            commands::clear_service_data,
            commands::get_orphan_sessions,
            commands::clean_orphan_sessions,
            commands::set_lock,
            commands::lock_now,
            commands::unlock,
            commands::set_lock_pin,
            commands::forget_lock_pin,
            commands::open_signin_settings,
            commands::set_appearance,
            commands::check_update,
            commands::install_update,
            commands::set_updates,
            organizer::get_organizer,
            organizer::save_task,
            organizer::set_task_done,
            organizer::snooze_task,
            organizer::delete_task,
            organizer::reply_later,
            organizer::save_note,
            organizer::show_task,
            sync::sync_status,
            sync::sync_connect,
            sync::sync_passphrase,
            sync::sync_now,
            sync::sync_reset,
            sync::sync_disconnect,
            notify::service_notify,
            notify::service_status,
            preview::preview_state,
            preview::preview_open_browser,
            preview::preview_close,
            attachment::attachment_state,
            attachment::attachment_save,
            attachment::attachment_open_app,
            attachment::attachment_reveal,
            attachment::attachment_close,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Papeo");

    app.run(|_app, _event| {
        // macOS: clicar no ícone do Dock com a janela escondida a reabre.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = _event {
            tray::show_main(_app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::slept;
    use std::time::{Duration, SystemTime};

    #[test]
    fn detects_sleep_by_wall_clock_gap() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(!slept(t, t + Duration::from_secs(15)));
        assert!(!slept(t, t + Duration::from_secs(4 * 60)));
        assert!(slept(t, t + Duration::from_secs(5 * 60)));
        assert!(slept(t, t + Duration::from_secs(6 * 60 * 60)));
        // Relógio voltou: não é repouso.
        assert!(!slept(t, t - Duration::from_secs(60 * 60)));
    }
}
