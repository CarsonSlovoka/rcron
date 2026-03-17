use rust_i18n::{self};

pub fn init() {
    // 建議的語言優先順序（可依需求調整）
    let preferred = vec![
        // 1. 環境變數（最優先，方便測試）
        std::env::var("RUST_I18N_LOCALE").ok(),
        std::env::var("LANG").ok(),
        std::env::var("LC_ALL").ok(),
        // 2. 系統語言
        sys_locale::get_locale(),
        // 3. 預設
        Some("en".to_string()),
    ];

    let mut locale = "en".to_string();

    for loc in preferred.into_iter().flatten() {
        let loc = loc
            .split('.')
            .next()
            .unwrap()
            .replace('_', "-")
            .to_lowercase();

        if rust_i18n::available_locales!().contains(&loc.as_str()) {
            // zh-tw, en-us, ...
            locale = loc;
            break;
        }

        if let Some(short) = loc.split('-').next() {
            // zh, en, ...
            if rust_i18n::available_locales!().contains(&short) {
                locale = short.to_string();
                break;
            }
        }
    }

    log::debug!("locale: {}", locale); // 記得env_logger::init()才可以有作用

    rust_i18n::set_locale(&locale);
    // eprintln!("i18n initialized with locale: {}", locale);  // debug 用
}
