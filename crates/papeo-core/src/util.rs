//! Funções puras usadas pelo app: não lidas, textos e nomes de arquivo.

use std::path::{Path, PathBuf};

/// Extrai a contagem de não lidas de títulos como "(3) WhatsApp" ou "Telegram (12)".
pub fn parse_unread(title: &str) -> u32 {
    let mut rest = title;
    while let Some(start) = rest.find('(') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find(')') {
            let inner = after[..end].trim().trim_end_matches('+');
            if let Ok(n) = inner.parse::<u32>() {
                return n;
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    0
}

/// Limita o texto vindo dos sites (título e corpo das notificações).
pub fn clip(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    }
}

/// Nome de arquivo seguro a partir do sugerido pelo site: sem pastas,
/// separadores, caracteres de controle ou nomes reservados.
pub fn safe_file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .filter(|c| !is_format_char(*c))
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = shorten_keeping_extension(cleaned.trim().trim_matches('.').trim(), 150);
    let stem = cleaned
        .split('.')
        .next()
        .unwrap_or_default()
        .trim()
        .to_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            n.chars().count() == 1 && n.chars().all(|c| c.is_ascii_digit() || "¹²³".contains(c))
        })
    };
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || numbered("COM")
        || numbered("LPT");
    if cleaned.is_empty() || reserved {
        format!(
            "download{}",
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("-{cleaned}")
            }
        )
    } else {
        cleaned
    }
}

/// Caracteres invisíveis de direção de texto (podem disfarçar `exe.pdf`).
fn is_format_char(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Encurta preservando a extensão (`relatorio-muito….pdf`).
fn shorten_keeping_extension(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        return name.to_owned();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 12 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let room = max_chars.saturating_sub(ext.chars().count() + 1);
    let cut: String = stem.chars().take(room).collect();
    format!("{}…{ext}", cut.trim_end())
}

/// `dir/nome.ext`, ou `dir/nome (2).ext` etc. se já existir no disco ou em
/// `reserved` (downloads em andamento que ainda não criaram o arquivo).
pub fn unique_path(dir: &Path, file_name: &str, reserved: &[PathBuf]) -> PathBuf {
    let free = |p: &PathBuf| !p.exists() && !reserved.contains(p);
    let candidate = dir.join(file_name);
    if free(&candidate) {
        return candidate;
    }
    let (stem, ext) = match file_name.rfind('.') {
        Some(i) if i > 0 => (&file_name[..i], &file_name[i..]),
        _ => (file_name, ""),
    };
    (2..10_000)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(free)
        .unwrap_or(candidate)
}

/// Como um anexo aparece na visualização do Papeo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentView {
    /// Quick Look do macOS (PDF, Office, imagens, vídeo).
    QuickLook,
    /// Webview só com o arquivo (Windows: PDF, imagem, áudio e vídeo).
    Web,
    /// Sem visualização aqui: só salvar ou abrir no app padrão.
    External,
}

/// Abrem na página (visualizador do navegador do sistema). Sem HTML nem SVG:
/// um anexo nunca roda script.
const WEB_EXT: &[&str] = &[
    "pdf", "png", "jpg", "jpeg", "gif", "webp", "bmp", "mp4", "mp3", "m4a", "wav", "txt",
];
/// Documentos e mídias que o Quick Look mostra; no Windows, "abrir no app".
const DOC_EXT: &[&str] = &[
    "doc", "docx", "xls", "xlsx", "xlsm", "ppt", "pptx", "odt", "ods", "odp", "rtf", "csv",
    "pages", "numbers", "key", "heic", "heif", "tif", "tiff", "mov",
];

/// Visualização de um anexo pelo nome; `None` = tipo sem visualização (o
/// arquivo é salvo direto, como antes).
pub fn attachment_view(file_name: &str, quick_look: bool) -> Option<AttachmentView> {
    let ext = file_name.rsplit_once('.')?.1.to_ascii_lowercase();
    let web = WEB_EXT.contains(&ext.as_str());
    if !web && !DOC_EXT.contains(&ext.as_str()) {
        return None;
    }
    Some(if quick_look {
        AttachmentView::QuickLook
    } else if web {
        AttachmentView::Web
    } else {
        AttachmentView::External
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_views() {
        use AttachmentView::*;
        assert_eq!(attachment_view("Planilha.XLSX", true), Some(QuickLook));
        assert_eq!(attachment_view("Planilha.xlsx", false), Some(External));
        assert_eq!(attachment_view("nota.pdf", false), Some(Web));
        assert_eq!(attachment_view("foto.jpeg", true), Some(QuickLook));
        assert_eq!(attachment_view("pagina.html", true), None);
        assert_eq!(attachment_view("desenho.svg", false), None);
        assert_eq!(attachment_view("app.exe", false), None);
        assert_eq!(attachment_view("sem-extensao", true), None);
    }

    #[test]
    fn unread_from_titles() {
        assert_eq!(parse_unread("(3) WhatsApp"), 3);
        assert_eq!(parse_unread("Telegram (12)"), 12);
        assert_eq!(parse_unread("(99+) Mensagens"), 99);
        assert_eq!(parse_unread("WhatsApp"), 0);
        assert_eq!(parse_unread("Grupo (família) (4)"), 4);
    }

    #[test]
    fn clip_limits_chars() {
        assert_eq!(clip("  oi  ", 10), "oi");
        assert_eq!(clip("ação longa demais", 4), "ação…");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("foto.jpg"), "foto.jpg");
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("..\\..\\win.ini"), "win.ini");
        assert_eq!(safe_file_name("a:b?.pdf"), "a_b_.pdf");
        assert_eq!(safe_file_name(".."), "download");
        assert_eq!(safe_file_name(""), "download");
        assert_eq!(safe_file_name("CON.txt"), "download-CON.txt");
        assert_eq!(safe_file_name("com1"), "download-com1");
        assert_eq!(safe_file_name("comum.txt"), "comum.txt");
        assert_eq!(safe_file_name("CON .txt"), "download-CON .txt");
        assert_eq!(safe_file_name("lpt¹"), "download-lpt¹");
        assert_eq!(safe_file_name("fatura\u{202E}fdp.exe"), "faturafdp.exe");
        let long = format!("{}.pdf", "a".repeat(300));
        let short = safe_file_name(&long);
        assert!(short.ends_with("….pdf") && short.chars().count() <= 150);
    }

    #[test]
    fn unique_path_adds_suffix() {
        let dir = std::env::temp_dir().join(format!("papeo-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique_path(&dir, "nota.pdf", &[]);
        assert_eq!(first, dir.join("nota.pdf"));
        std::fs::write(&first, b"x").unwrap();
        assert_eq!(unique_path(&dir, "nota.pdf", &[]), dir.join("nota (2).pdf"));
        // Download em andamento com o mesmo nome ainda não existe no disco.
        let pending = vec![dir.join("nota (2).pdf")];
        assert_eq!(
            unique_path(&dir, "nota.pdf", &pending),
            dir.join("nota (3).pdf")
        );
        std::fs::write(dir.join("LEIAME"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "LEIAME", &[]), dir.join("LEIAME (2)"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
