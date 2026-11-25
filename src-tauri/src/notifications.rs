//! Notificações nativas com "Responder" e clique que abre a conversa.
//!
//! - macOS: `UNUserNotificationCenter`, com ação de texto (só no app
//!   instalado; rodando fora de um `.app` o sistema não permite).
//! - Windows: toast com campo de texto (WinRT), no mesmo AppUserModelID do
//!   plugin de notificações.
//! - Se o caminho nativo não estiver disponível, cai no plugin de
//!   notificações (sem resposta nem conversa no clique).
//!
//! Para onde cada notificação leva (conta e conversa) fica só na memória
//! do app, por identificador; nada disso vai para o sistema.

use crate::state::AppState;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

/// Quantas notificações recentes ainda respondem a um clique.
const MAX_TARGETS: usize = 64;

#[derive(Debug, Clone)]
pub struct Target {
    pub service_id: String,
    /// Conversa (contato ou grupo), como veio do site.
    pub chat: Option<String>,
    /// Mostra o campo "Responder".
    pub reply: bool,
    /// Lembrete desta tarefa: botões "Adiar 1 h" e "Concluir".
    pub task: Option<String>,
    /// A vez do lembrete que esta notificação avisa.
    pub remind_at: Option<u64>,
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static TARGETS: Mutex<VecDeque<(String, Target)>> = Mutex::new(VecDeque::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Marca desta execução no identificador. Notificações de execuções
/// anteriores continuam na Central; com o mesmo identificador, o macOS só
/// atualiza a antiga, sem banner nem som.
fn run_mark() -> u64 {
    static RUN: OnceLock<u64> = OnceLock::new();
    *RUN.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    })
}

fn remember(target: Target) -> String {
    // Lembrete: o id da tarefa vai no identificador, para os botões e o
    // clique funcionarem mesmo depois de reabrir o app.
    let task = target
        .task
        .as_deref()
        .filter(|t| papeo_core::is_item_id(t))
        .map(|t| format!("task-{t}-r{}-", target.remind_at.unwrap_or(0)))
        .unwrap_or_default();
    let id = format!(
        "papeo-{task}{}-{}",
        run_mark(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let mut targets = TARGETS.lock().unwrap_or_else(|e| e.into_inner());
    targets.push_back((id.clone(), target));
    while targets.len() > MAX_TARGETS {
        targets.pop_front();
    }
    id
}

/// Tarefa (e a vez do lembrete) pelo identificador da notificação.
fn task_of(id: &str) -> Option<(String, Option<u64>)> {
    let mut parts = id.strip_prefix("papeo-task-")?.split('-');
    let task = parts.next()?;
    let at = parts
        .next()
        .and_then(|p| p.strip_prefix('r'))
        .and_then(|n| n.parse::<u64>().ok())
        .filter(|n| *n > 0);
    papeo_core::is_item_id(task).then(|| (task.to_owned(), at))
}

fn target(id: &str) -> Option<Target> {
    TARGETS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(k, _)| k == id)
        .map(|(_, t)| t.clone())
}

/// Liga o caminho nativo. Chamar uma vez, no `setup`.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    #[cfg(target_os = "macos")]
    mac::init();
}

/// Mostra a notificação. `sound`: nome do som para o plugin (caminho
/// alternativo); o nativo usa o som padrão do sistema.
pub fn show(
    app: &AppHandle,
    title: &str,
    body: &str,
    sound: Option<&str>,
    target: Target,
) -> Result<(), String> {
    let id = remember(target.clone());
    #[cfg(target_os = "macos")]
    if mac::show(&id, title, body, sound.is_some(), &target) {
        return Ok(());
    }
    #[cfg(windows)]
    if win::show(app, &id, title, body, sound.is_some(), &target).is_ok() {
        return Ok(());
    }
    let _ = id;
    let mut notification = app.notification().builder().title(title).body(body);
    if let Some(sound) = sound {
        notification = notification.sound(sound);
    }
    notification.show().map_err(|e| e.to_string())
}

/// Clique na notificação: traz o app e abre a conversa (se desbloqueado).
/// Lembrete sem conversa abre a página Tarefas.
fn opened(id: &str) {
    let (Some(app), Some(target)) = (APP.get().cloned(), target(id)) else {
        if let Some(app) = APP.get().cloned() {
            // Lembrete de antes de reabrir o app: abre pela tarefa.
            match task_of(id) {
                Some((task, _)) => {
                    std::thread::spawn(move || crate::organizer::open_task(&app, &task));
                }
                None => crate::tray::show_main(&app),
            }
        }
        return;
    };
    // Fora da thread do sistema: abrir pode criar webview (espera a principal).
    std::thread::spawn(move || {
        crate::popover::hide(&app);
        if target.service_id.is_empty() {
            crate::organizer::show_tasks(&app);
            return;
        }
        crate::tray::show_main(&app);
        if app.state::<AppState>().lock().locked {
            return;
        }
        let _ = match &target.chat {
            Some(chat) => crate::webviews::open_chat(&app, &target.service_id, chat, true),
            None => crate::webviews::activate(&app, Some(target.service_id.clone())),
        };
    });
}

/// Resposta digitada na notificação: envia sem trazer o app; se falhar,
/// avisa com outra notificação (o clique nela abre a conversa).
fn replied(id: &str, text: String) {
    let (Some(app), Some(target)) = (APP.get().cloned(), target(id)) else {
        return;
    };
    let Some(chat) = target.chat.clone().filter(|_| target.reply) else {
        return;
    };
    std::thread::spawn(move || {
        if let Err(err) = crate::reply::send(&app, &target.service_id, &chat, &text) {
            let retry = Target {
                reply: false,
                ..target
            };
            let _ = show(&app, "Resposta não enviada", &err, None, retry);
        }
    });
}

/// Botão do lembrete ("snooze" ou "done"), sem trazer o app.
fn reminder_action(id: &str, action: &str) {
    let task = target(id)
        .and_then(|t| t.task.map(|task| (task, t.remind_at)))
        .or_else(|| task_of(id));
    let (Some(app), Some((task, remind_at))) = (APP.get().cloned(), task) else {
        return;
    };
    let action = action.to_owned();
    std::thread::spawn(move || {
        crate::organizer::notification_action(&app, &task, remind_at, &action)
    });
}

#[cfg(target_os = "macos")]
mod mac {
    use super::Target;
    use block2::{DynBlock, RcBlock};
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{define_class, msg_send, sel, AllocAnyThread};
    use objc2_foundation::{NSArray, NSBundle, NSError, NSSet, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
        UNNotificationActionOptions, UNNotificationCategory, UNNotificationCategoryOptions,
        UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
        UNNotificationSound, UNTextInputNotificationAction, UNUserNotificationCenter,
        UNUserNotificationCenterDelegate,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;

    const CATEGORY: &str = "papeo.reply";
    const REPLY_ACTION: &str = "reply";
    const REMINDER_CATEGORY: &str = "papeo.reminder";

    /// O usuário permitiu as notificações do Papeo.
    static AUTHORIZED: AtomicBool = AtomicBool::new(false);
    /// Delegate único, nunca liberado (a central só guarda referência fraca).
    static DELEGATE: OnceLock<usize> = OnceLock::new();

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "PapeoNotificationDelegate"]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            /// Com o Papeo em primeiro plano, mostra igual.
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                completion.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound,));
            }

            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                completion: &DynBlock<dyn Fn()>,
            ) {
                let id = response.notification().request().identifier().to_string();
                let action = response.actionIdentifier().to_string();
                if action == REPLY_ACTION {
                    if let Some(text) = user_text(response) {
                        super::replied(&id, text);
                    }
                } else if action == "snooze" || action == "done" {
                    super::reminder_action(&id, &action);
                } else if action == "com.apple.UNNotificationDefaultActionIdentifier" {
                    super::opened(&id);
                }
                completion.call(());
            }
        }
    );

    /// Texto digitado (`UNTextInputNotificationResponse.userText`).
    fn user_text(response: &UNNotificationResponse) -> Option<String> {
        // SAFETY: `userText` só é chamado se o objeto responde a ele.
        unsafe {
            let has: bool = msg_send![response, respondsToSelector: sel!(userText)];
            if !has {
                return None;
            }
            let text: Option<Retained<NSString>> = msg_send![response, userText];
            text.map(|t| t.to_string())
        }
    }

    /// Só dentro de um `.app` (o app instalado): fora dele a central de
    /// notificações encerra o processo.
    fn bundled() -> bool {
        NSBundle::mainBundle()
            .bundlePath()
            .to_string()
            .ends_with(".app")
    }

    pub fn init() {
        if !bundled() {
            return;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = *DELEGATE.get_or_init(|| {
            // SAFETY: `init` de NSObject numa classe sem ivars.
            let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(), init] };
            Retained::into_raw(delegate) as usize
        }) as *const Delegate;
        // SAFETY: o delegate vive até o fim do processo (nunca liberado).
        let delegate = unsafe { &*delegate };
        center.setDelegate(Some(ProtocolObject::from_ref(delegate)));

        let action = UNTextInputNotificationAction::actionWithIdentifier_title_options_textInputButtonTitle_textInputPlaceholder(
            &NSString::from_str(REPLY_ACTION),
            &NSString::from_str("Responder"),
            // Com o Mac bloqueado, responder pede a senha/Touch ID antes.
            UNNotificationActionOptions::AuthenticationRequired,
            &NSString::from_str("Enviar"),
            &NSString::from_str("Mensagem"),
        );
        let actions: Retained<NSArray<UNNotificationAction>> =
            NSArray::from_retained_slice(&[Retained::into_super(action)]);
        let category =
            UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                &NSString::from_str(CATEGORY),
                &actions,
                &NSArray::new(),
                UNNotificationCategoryOptions::empty(),
            );
        let button = |id: &str, title: &str| {
            UNNotificationAction::actionWithIdentifier_title_options(
                &NSString::from_str(id),
                &NSString::from_str(title),
                UNNotificationActionOptions::empty(),
            )
        };
        let reminder_actions: Retained<NSArray<UNNotificationAction>> =
            NSArray::from_retained_slice(&[
                button("snooze", "Adiar 1 h"),
                button("done", "Concluir"),
            ]);
        let reminder =
            UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                &NSString::from_str(REMINDER_CATEGORY),
                &reminder_actions,
                &NSArray::new(),
                UNNotificationCategoryOptions::empty(),
            );
        center.setNotificationCategories(&NSSet::from_retained_slice(&[category, reminder]));

        let done = RcBlock::new(|granted: Bool, _error: *mut NSError| {
            AUTHORIZED.store(granted.as_bool(), Ordering::Relaxed);
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge,
            &done,
        );
    }

    /// Mostra pelo caminho nativo; `false` = use o alternativo.
    pub fn show(id: &str, title: &str, body: &str, sound: bool, target: &Target) -> bool {
        if !AUTHORIZED.load(Ordering::Relaxed) {
            return false;
        }
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        if sound {
            content.setSound(Some(&UNNotificationSound::defaultSound()));
        }
        // Agrupa por conta na Central de Notificações (lembretes juntos).
        let thread = if target.task.is_some() {
            "papeo.tasks"
        } else {
            target.service_id.as_str()
        };
        content.setThreadIdentifier(&NSString::from_str(thread));
        if target.task.is_some() {
            content.setCategoryIdentifier(&NSString::from_str(REMINDER_CATEGORY));
        } else if target.reply && target.chat.is_some() {
            content.setCategoryIdentifier(&NSString::from_str(CATEGORY));
        }
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(id),
            &content,
            None,
        );
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, None);
        true
    }
}

#[cfg(windows)]
mod win {
    use super::Target;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use tauri::AppHandle;
    use windows::core::{Interface, Ref, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::{IReference, TypedEventHandler};
    use windows::UI::Notifications::{
        ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
    };

    /// O Windows só chama o `Activated` enquanto o objeto do toast existe.
    static ALIVE: Mutex<VecDeque<ToastNotification>> = Mutex::new(VecDeque::new());
    const KEEP: usize = 32;

    fn escape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                c if c.is_control() && c != '\n' => {}
                c => out.push(c),
            }
        }
        out
    }

    fn xml(title: &str, body: &str, sound: bool, reply: bool, reminder: bool) -> String {
        let audio = if sound {
            ""
        } else {
            r#"<audio silent="true"/>"#
        };
        let actions = if reminder {
            r#"<actions><action content="Adiar 1 h" arguments="snooze" activationType="foreground"/><action content="Concluir" arguments="done" activationType="foreground"/></actions>"#
        } else if reply {
            r#"<actions><input id="reply" type="text" placeHolderContent="Responder…"/><action content="Enviar" arguments="reply" hint-inputId="reply" activationType="foreground"/></actions>"#
        } else {
            ""
        };
        format!(
            r#"<toast launch="open"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{audio}{actions}</toast>"#,
            escape(title),
            escape(body)
        )
    }

    /// Mesmo AppUserModelID do plugin: o do app instalado; rodando de
    /// `target/` (desenvolvimento), o do PowerShell, que já é registrado.
    fn app_id(app: &AppHandle) -> String {
        let installed = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.display().to_string()))
            .is_some_and(|dir| !dir.contains("\\target\\"));
        if installed {
            app.config().identifier.clone()
        } else {
            r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe"
                .to_owned()
        }
    }

    fn user_text(args: &ToastActivatedEventArgs) -> Option<String> {
        let value = args
            .UserInput()
            .ok()?
            .Lookup(&HSTRING::from("reply"))
            .ok()?;
        let text = value.cast::<IReference<HSTRING>>().ok()?.Value().ok()?;
        Some(text.to_string())
    }

    pub fn show(
        app: &AppHandle,
        id: &str,
        title: &str,
        body: &str,
        sound: bool,
        target: &Target,
    ) -> windows::core::Result<()> {
        let reply = target.reply && target.chat.is_some();
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml(
            title,
            body,
            sound,
            reply,
            target.task.is_some(),
        )))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        let id = id.to_owned();
        toast.Activated(&TypedEventHandler::new(
            move |_toast: Ref<ToastNotification>, args: Ref<windows::core::IInspectable>| {
                let Some(args) = args
                    .as_ref()
                    .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
                else {
                    return Ok(());
                };
                let arguments = args.Arguments().map(|a| a.to_string()).unwrap_or_default();
                if arguments == "reply" {
                    if let Some(text) = user_text(&args) {
                        super::replied(&id, text);
                    }
                } else if arguments == "snooze" || arguments == "done" {
                    super::reminder_action(&id, &arguments);
                } else {
                    super::opened(&id);
                }
                Ok(())
            },
        ))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id(app)))?
            .Show(&toast)?;
        let mut alive = ALIVE.lock().unwrap_or_else(|e| e.into_inner());
        alive.push_back(toast);
        while alive.len() > KEEP {
            alive.pop_front();
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn xml_is_escaped() {
            let xml = super::xml("Ana & <Bia>", "\"oi\"", false, true, false);
            assert!(xml.contains("Ana &amp; &lt;Bia&gt;"));
            assert!(xml.contains("&quot;oi&quot;"));
            assert!(xml.contains(r#"hint-inputId="reply""#));
            assert!(xml.contains(r#"silent="true""#));
            let reminder = super::xml("Lembrete", "ligar", true, false, true);
            assert!(reminder.contains(r#"arguments="snooze""#));
            assert!(reminder.contains(r#"arguments="done""#));
            assert!(!reminder.contains("reply"));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn task_id_travels_in_the_notification_id() {
        let task = "0123456789abcdef0123456789abcdef";
        let id = super::remember(super::Target {
            service_id: String::new(),
            chat: None,
            reply: false,
            task: Some(task.into()),
            remind_at: Some(1_800_000_000),
        });
        assert_eq!(
            super::task_of(&id),
            Some((task.to_owned(), Some(1_800_000_000)))
        );
        assert_eq!(super::task_of("papeo-123-4"), None);
        assert_eq!(super::task_of("papeo-task-../x-1-2"), None);
    }
}
