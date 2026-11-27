//! Catálogo inglês das mensagens do lado Rust (spec 20): português → inglês,
//! em dois arquivos (`en_app.rs`: app Tauri; `en_core.rs`: núcleo).
//! `tests/i18n.test.mjs` confere que cada `tr`/`trf`/`tk` do código está
//! num deles. Uma entrada por linha, no formato `"pt" => "en",`.

pub(super) fn get(pt: &str) -> Option<&'static str> {
    super::en_app::get(pt).or_else(|| super::en_core::get(pt))
}
