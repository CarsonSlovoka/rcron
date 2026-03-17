cargo new crontab

cargo add tokio -F full
cargo add chrono
cargo add cron
cargo add anyhow
cargo add log
cargo add env_logger
cargo add serde -F derive
cargo add serde_json

cargo add colored


# 本地化
cargo add rust-i18n # https://github.com/longbridge/rust-i18n  # https://crates.io/crates/rust-i18n
cargo add sys-locale # https://github.com/1Password/sys-locale # https://crates.io/crates/sys-locale  # 系統可以自動偵測
