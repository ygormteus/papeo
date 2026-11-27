//! Idioma das mensagens do Papeo (spec 20). Estilo gettext: a frase em
//! português é a chave; `en.rs` traz a versão em inglês. Sem tradução, fica
//! o português. O idioma vale para o processo todo (bandeja, menu,
//! notificações e erros).

use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

mod en;
mod en_app;
mod en_core;

/// Escolha guardada nas configurações.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Segue o idioma do sistema.
    #[default]
    System,
    Pt,
    En,
}

/// Idioma valendo agora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Pt,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Português para qualquer variante (`pt`, `pt-BR`, `pt_PT`); inglês para o
/// resto.
pub fn lang_of_locale(tag: &str) -> Lang {
    let tag = tag.trim().to_ascii_lowercase();
    if tag == "pt" || tag.starts_with("pt-") || tag.starts_with("pt_") {
        Lang::Pt
    } else {
        Lang::En
    }
}

/// Idioma efetivo da escolha; `system` é o idioma do sistema, se conhecido
/// (sem ele, português).
pub fn resolve(choice: Language, system: Option<&str>) -> Lang {
    match choice {
        Language::Pt => Lang::Pt,
        Language::En => Lang::En,
        Language::System => system.map_or(Lang::Pt, lang_of_locale),
    }
}

pub fn set_lang(lang: Lang) {
    CURRENT.store(matches!(lang, Lang::En) as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::Pt
    }
}

/// A frase no idioma atual.
pub fn tr(pt: &'static str) -> &'static str {
    tr_in(lang(), pt)
}

/// Marca uma frase para o catálogo sem traduzir agora (traduzida depois com
/// `tr(valor)`), como o `N_()` do gettext.
pub const fn tk(pt: &'static str) -> &'static str {
    pt
}

/// A frase num idioma dado.
pub fn tr_in(lang: Lang, pt: &'static str) -> &'static str {
    match lang {
        Lang::Pt => pt,
        Lang::En => en::get(pt).unwrap_or(pt),
    }
}

/// A frase no idioma atual com `{nome}` trocado pelos valores.
pub fn trf(pt: &'static str, vars: &[(&str, &dyn Display)]) -> String {
    fill(tr(pt), vars)
}

fn fill(text: &str, vars: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match vars.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(&v.to_string()),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_locale() {
        assert_eq!(lang_of_locale("pt-BR"), Lang::Pt);
        assert_eq!(lang_of_locale("PT_pt"), Lang::Pt);
        assert_eq!(lang_of_locale("pt"), Lang::Pt);
        assert_eq!(lang_of_locale("en-US"), Lang::En);
        assert_eq!(lang_of_locale("es"), Lang::En);
        assert_eq!(lang_of_locale("ptx"), Lang::En);
        assert_eq!(resolve(Language::System, Some("de-DE")), Lang::En);
        assert_eq!(resolve(Language::System, None), Lang::Pt);
        assert_eq!(resolve(Language::Pt, Some("en")), Lang::Pt);
        assert_eq!(resolve(Language::En, Some("pt-BR")), Lang::En);
    }

    #[test]
    fn fills_named_values() {
        let n = 3;
        assert_eq!(
            fill("{n} de {total}", &[("n", &n), ("total", &5)]),
            "3 de 5"
        );
        assert_eq!(fill("{x} e {", &[]), "{x} e {");
        assert_eq!(tr_in(Lang::En, "frase sem tradução"), "frase sem tradução");
    }
}
