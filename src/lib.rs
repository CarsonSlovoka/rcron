// 👇 如果沒有加在rust_i18n::available_locales!()會有錯誤: not found in the crate root
rust_i18n::i18n!("locales", fallback = "en");
pub mod i18n;
