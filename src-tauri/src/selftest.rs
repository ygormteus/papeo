//! Autoteste no app de verdade (só com `--features selftest`; o app
//! publicado não tem este código). Roda com dados temporários, sobe um
//! servidor local com uma "conta" de teste e confere, nas webviews reais
//! (WebKit no Mac, WebView2 no Windows):
//! - os jeitos de os sites abrirem links (link comum, o do WhatsApp que
//!   cancela o clique e clica num link invisível, janela em branco,
//!   `window.open` direto) chegam ao Papeo uma vez só;
//! - links do próprio mensageiro ficam com ele;
//! - conversas sem contador na lista do site (lidas lá ou no celular)
//!   saem das não lidas, e o site consegue falar com o Papeo mesmo numa
//!   porta fora da padrão;
//! - a prévia abre com o link e fecha com ⌘W/Ctrl+W vindo da página;
//! - um anexo (PDF) baixado com "Visualizar antes" abre na janela de anexo
//!   (Quick Look no Mac, página no Windows) sem ir para os downloads; salvar
//!   copia para lá e fechar apaga o temporário.
//!
//! Uso: `pnpm build && cargo run -p papeo --features selftest`. Sai com
//! código 0 se tudo passou. Os links "abertos no navegador" só são anotados.

use crate::state::AppState;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Links que iriam para o navegador.
static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Caminhos pedidos ao servidor local.
static REQUESTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Pasta temporária com tudo deste autoteste (nada dos dados reais).
pub fn base_dir() -> PathBuf {
    std::env::temp_dir().join(format!("papeo-selftest-{}", std::process::id()))
}

/// No lugar do navegador: só anota.
pub fn opened(url: &str) {
    lock(&OPENED).push(url.to_owned());
}

const SERVICE_PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Autoteste</title><body>
<script>
// "Outro site": a mesma máquina noutra porta (origem diferente).
const other = "http://127.0.0.1:" + new URLSearchParams(location.search).get("other");
let cancelAll = false;
// Como o WhatsApp: cancela todo clique em link e trata por conta própria.
document.addEventListener("click", (e) => { if (cancelAll && e.target.closest("a")) e.preventDefault(); }, true);
const anchor = (href) => {
  const a = document.createElement("a");
  a.href = href; a.target = "_blank"; a.rel = "noopener noreferrer"; a.textContent = href;
  document.body.appendChild(a);
  return a;
};
window.runCase = (c) => {
  cancelAll = c === "whatsapp" || c === "internal";
  if (c === "anchor") anchor(other + "/anchor").click();
  if (c === "whatsapp") {
    const a = anchor(other + "/whatsapp");
    a.addEventListener("click", () => setTimeout(() => {
      const hidden = anchor(other + "/whatsapp");
      hidden.style.display = "none";
      hidden.click();
      hidden.remove();
    }, 300));
    a.click();
  }
  if (c === "blank") { const w = window.open(""); w.opener = null; w.location = other + "/blank"; }
  if (c === "direct") window.open(other + "/direct", "_blank", "noopener");
  if (c === "internal") anchor("https://wa.me/5585999999999").click();
  if (c === "preview") anchor(other + "/target?preview").click();
  if (c === "attachment") {
    const a = document.createElement("a");
    a.href = "/anexo.pdf"; a.download = "anexo.pdf";
    document.body.appendChild(a);
    a.click();
  }
  // Lista de conversas como a do WhatsApp: "Ana" sem contador (lida),
  // "Bruno" com contador.
  if (c === "list") {
    const pane = document.createElement("div");
    pane.id = "pane-side";
    pane.innerHTML =
      '<div role="row"><span title="Ana">Ana</span></div>' +
      '<div role="row"><span title="Bruno">Bruno</span><span aria-label="2 mensagens não lidas">2</span></div>';
    document.body.appendChild(pane);
  }
};
addEventListener("load", () => fetch("/loaded"));
</script>"#;

/// PDF mínimo de uma página em branco.
const ATTACHMENT_PDF: &str = "%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n";

const TARGET_PAGE: &str =
    "<!doctype html><meta charset=\"utf-8\"><title>Alvo da prévia</title><h1>Alvo</h1>";

fn serve(listener: TcpListener) {
    // Uma thread por conexão: o WebKit abre conexões antecipadas que podem
    // nunca mandar pedido, e não podem travar as outras.
    for stream in listener.incoming().flatten() {
        std::thread::spawn(move || respond(stream));
    }
}

fn respond(stream: std::net::TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
    lock(&REQUESTS).push(path.clone());
    let (body, headers) = if path == "/" || path.starts_with("/?") {
        (SERVICE_PAGE, "Content-Type: text/html; charset=utf-8")
    } else if path == "/anexo.pdf" {
        (
            ATTACHMENT_PDF,
            "Content-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"anexo.pdf\"",
        )
    } else {
        (TARGET_PAGE, "Content-Type: text/html; charset=utf-8")
    };
    let mut stream = &stream;
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\n{headers}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
}

fn wait_for(what: &str, timeout: Duration, mut ok: impl FnMut() -> bool) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("tempo esgotado esperando: {what}"))
}

fn count_opened(suffix: &str) -> usize {
    lock(&OPENED).iter().filter(|u| u.ends_with(suffix)).count()
}

fn eval(app: &AppHandle, label: &str, js: &str) -> Result<(), String> {
    app.get_webview(label)
        .ok_or_else(|| format!("webview {label} não existe"))?
        .eval(js)
        .map_err(|e| e.to_string())
}

/// Roda um caso de link e confere quantas vezes ele foi para o navegador.
fn link_case(
    app: &AppHandle,
    id: &str,
    case: &str,
    suffix: &str,
    want: usize,
) -> Result<(), String> {
    eval(app, id, &format!("runCase({case:?})"))?;
    if want > 0 {
        wait_for(
            &format!("{case}: link aberto"),
            Duration::from_secs(10),
            || count_opened(suffix) >= want,
        )?;
    }
    // Tempo para uma abertura repetida (o clique invisível vem depois).
    std::thread::sleep(Duration::from_millis(1500));
    let got = count_opened(suffix);
    if got != want {
        return Err(format!("{case}: aberto {got} vez(es), esperado {want}"));
    }
    println!("SELFTEST ok   {case}");
    Ok(())
}

fn run(app: &AppHandle) -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let other = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let other_port = other.local_addr().map_err(|e| e.to_string())?.port();
    std::thread::spawn(move || serve(listener));
    std::thread::spawn(move || serve(other));

    let service = papeo_core::Service::new(
        papeo_core::ServiceKind::Custom,
        Some("Autoteste".into()),
        Some(format!("http://127.0.0.1:{port}/?other={other_port}")),
    )?;
    let id = service.id.clone();
    app.state::<AppState>().update(|inner| {
        inner.config.services.push(service);
        Ok(())
    })?;
    crate::webviews::activate(app, Some(id.clone()))?;
    wait_for("página da conta carregar", Duration::from_secs(30), || {
        lock(&REQUESTS).iter().any(|p| p == "/loaded")
    })?;
    println!("SELFTEST ok   conta de teste carregada");

    link_case(app, &id, "anchor", "/anchor", 1)?;
    link_case(app, &id, "whatsapp", "/whatsapp", 1)?;
    link_case(app, &id, "blank", "/blank", 1)?;
    link_case(app, &id, "direct", "/direct", 1)?;
    link_case(app, &id, "internal", "wa.me/5585999999999", 0)?;

    // Não lidas seguindo a lista do site.
    {
        let state = app.state::<AppState>();
        let mut inner = state.lock();
        let old = crate::state::now_secs() - 60;
        for title in ["Ana", "Bruno"] {
            inner.push_recent(
                &id,
                title,
                crate::state::ChatMessage {
                    body: "Oi".into(),
                    at: old,
                    mine: false,
                },
            );
        }
    }
    eval(app, &id, "runCase('list')")?;
    let unread = |title: &str| {
        crate::state::snapshot(app)
            .recent
            .iter()
            .find(|v| v.chat == title)
            .map(|v| v.unread)
    };
    wait_for(
        "lista do site marcar Ana como lida",
        Duration::from_secs(15),
        || unread("Ana") == Some(false),
    )
    .map_err(|e| {
        let recent: Vec<_> = crate::state::snapshot(app)
            .recent
            .iter()
            .map(|v| (v.chat.clone(), v.unread))
            .collect();
        format!("{e} (recentes: {recent:?})")
    })?;
    if unread("Bruno") != Some(true) {
        return Err("lista: Bruno, com contador, deixou de ser não lida".into());
    }
    println!("SELFTEST ok   não lidas seguem a lista do site");

    // Prévia: abre com o link e fecha com ⌘W/Ctrl+W vindo da página.
    app.state::<AppState>().update(|inner| {
        inner.config.settings.open_links = papeo_core::LinkTarget::Preview;
        Ok(())
    })?;
    eval(app, &id, "runCase('preview')")?;
    wait_for("prévia abrir o link", Duration::from_secs(15), || {
        app.get_window(crate::preview::PREVIEW).is_some()
            && lock(&REQUESTS).iter().any(|p| p == "/target?preview")
    })
    .map_err(|e| {
        format!(
            "{e} (janela: {}, página: {:?}, pedidos: {:?}, navegador: {:?})",
            app.get_window(crate::preview::PREVIEW).is_some(),
            app.get_webview("preview-page")
                .map(|w| w.url().map(|u| u.to_string())),
            lock(&REQUESTS),
            lock(&OPENED)
        )
    })?;
    if count_opened("/target?preview") != 0 {
        return Err("prévia: o link também foi para o navegador".into());
    }
    println!("SELFTEST ok   prévia abriu o link");
    eval(
        app,
        "preview-page",
        r#"dispatchEvent(new KeyboardEvent("keydown", { key: "w", metaKey: true, ctrlKey: true, bubbles: true }))"#,
    )?;
    wait_for(
        "prévia fechar com ⌘W/Ctrl+W",
        Duration::from_secs(10),
        || app.get_window(crate::preview::PREVIEW).is_none(),
    )?;
    println!("SELFTEST ok   prévia fechou pelo atalho");

    attachment_case(app, &id)
}

/// Anexo com "Visualizar antes": abre na janela, salva e limpa.
fn attachment_case(app: &AppHandle, id: &str) -> Result<(), String> {
    let downloads = base_dir().join("downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    app.state::<AppState>().update(|inner| {
        let s = &mut inner.config.settings;
        s.attachments = papeo_core::AttachmentMode::Preview;
        s.downloads_dir = Some(downloads.to_string_lossy().into_owned());
        Ok(())
    })?;
    eval(app, id, "runCase('attachment')")?;
    let mut seen = None;
    wait_for("anexo abrir na janela", Duration::from_secs(20), || {
        seen = crate::attachment::inspect(app);
        seen.as_ref()
            .is_some_and(|(_, temp, shown)| *shown && temp.is_file())
    })
    .map_err(|e| format!("{e} (visto: {seen:?}, pedidos: {:?})", lock(&REQUESTS)))?;
    let (name, temp, _) = seen.unwrap_or_default();
    if name != "anexo.pdf" {
        return Err(format!("anexo: nome {name:?}"));
    }
    let saved = || downloads.join("anexo.pdf").is_file();
    if saved() || !crate::state::snapshot(app).downloads.is_empty() {
        return Err("anexo: foi para os downloads antes de salvar".into());
    }
    println!("SELFTEST ok   anexo abriu na visualização");
    tauri::async_runtime::block_on(crate::attachment::attachment_save(app.clone()))?;
    if !saved() || crate::state::snapshot(app).downloads.len() != 1 {
        return Err("anexo: salvar não copiou para os downloads".into());
    }
    tauri::async_runtime::block_on(crate::attachment::attachment_close(app.clone()));
    wait_for("janela do anexo fechar", Duration::from_secs(10), || {
        app.get_window(crate::attachment::ATTACHMENT).is_none()
    })?;
    if temp.exists() {
        return Err("anexo: o arquivo temporário ficou depois de fechar".into());
    }
    println!("SELFTEST ok   anexo salvo e temporário apagado");
    Ok(())
}

/// Roda o autoteste numa thread e fecha o app com o resultado.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let code = match run(&app) {
            Ok(()) => {
                println!("SELFTEST passou");
                0
            }
            Err(e) => {
                println!("SELFTEST FALHOU: {e}");
                1
            }
        };
        let _ = std::fs::remove_dir_all(base_dir());
        app.exit(code);
    });
}
