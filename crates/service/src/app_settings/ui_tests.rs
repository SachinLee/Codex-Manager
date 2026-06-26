use super::{normalize_ui_locale, DEFAULT_UI_LOCALE};

#[test]
fn ui_locale_normalization_defaults_to_chinese() {
    assert_eq!(normalize_ui_locale(None), DEFAULT_UI_LOCALE);
    assert_eq!(normalize_ui_locale(Some("")), DEFAULT_UI_LOCALE);
    assert_eq!(normalize_ui_locale(Some("unknown")), DEFAULT_UI_LOCALE);
}

#[test]
fn ui_locale_normalization_accepts_supported_aliases() {
    assert_eq!(normalize_ui_locale(Some("zh-cn")), "zh-CN");
    assert_eq!(normalize_ui_locale(Some("EN-US")), "en");
    assert_eq!(normalize_ui_locale(Some("ru-RU")), "ru");
    assert_eq!(normalize_ui_locale(Some("ko-kr")), "ko");
}
