use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

static CATALOGS: OnceLock<HashMap<&'static str, Value>> = OnceLock::new();

fn catalogs() -> &'static HashMap<&'static str, Value> {
    CATALOGS.get_or_init(|| {
        [
            ("en", include_str!("../../frontend/src/locales/en.json")),
            ("es", include_str!("../../frontend/src/locales/es.json")),
            ("pt", include_str!("../../frontend/src/locales/pt.json")),
            ("de", include_str!("../../frontend/src/locales/de.json")),
            ("fr", include_str!("../../frontend/src/locales/fr.json")),
        ]
        .into_iter()
        .map(|(language, source)| {
            (
                language,
                serde_json::from_str(source).expect("bundled locale catalog must be valid JSON"),
            )
        })
        .collect()
    })
}

pub fn text(language: &str, key: &str) -> String {
    let available = catalogs();
    available
        .get(language)
        .and_then(|catalog| catalog.get(key))
        .or_else(|| available.get("en").and_then(|catalog| catalog.get(key)))
        .and_then(Value::as_str)
        .unwrap_or(key)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::text;

    #[test]
    fn native_text_uses_selected_catalog_and_falls_back_to_english() {
        assert_eq!(text("fr", "tray.quit"), "Quitter ClipVault");
        assert_eq!(text("fr", "tray.about"), "À propos de ClipVault");
        assert_eq!(text("unsupported", "tray.quit"), "Quit ClipVault");
        assert_eq!(text("en", "missing.key"), "missing.key");
    }
}
