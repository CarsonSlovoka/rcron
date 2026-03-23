# bash cargo.io.sh

echo "🟧 test"
cargo test

echo "🟧 clippy"
cargo clippy # 是 Rust 官方提供的程式碼檢查工具（Linter），透過靜態分析檢查代碼中的問題，並提供最佳實踐建議
# cargo clippy --all-targets --all-features

echo "🟧 fmt"
cargo fmt --check # --check Run rustfmt in check mode
