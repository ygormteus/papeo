//! Idioma do app (spec 20): resolve a escolha das configurações com o idioma
//! do sistema e avisa as janelas que não recebem o estado.

use papeo_core::{Lang, Language};

/// Aplica a escolha ao processo todo; devolve o idioma efetivo.
pub fn apply(choice: Language) -> Lang {
    let lang = papeo_core::i18n::resolve(choice, sys_locale::get_locale().as_deref());
    papeo_core::i18n::set_lang(lang);
    lang
}

fn code() -> &'static str {
    match papeo_core::i18n::lang() {
        Lang::Pt => "pt",
        Lang::En => "en",
    }
}

/// Troca o idioma de uma barra já aberta (ver `main.ts`).
pub fn set_script() -> String {
    format!(
        "window.__PAPEO_LANG = '{0}'; window.__papeoSetLang?.('{0}');",
        code()
    )
}

/// Script de abertura das janelas auxiliares (prévia, anexo, mini janela).
pub fn view_script(view: &str) -> String {
    format!(
        "window.__PAPEO_VIEW = '{view}'; window.__PAPEO_LANG = '{}';",
        code()
    )
}
