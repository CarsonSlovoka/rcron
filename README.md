# rcron (rust-crontab)


## 開發動機

由於mac從26開始，似乎對crontab做了一些異動，使得原本的內容都不再執行

為了讓原本的內容可以繼續作業，所以才開發此專案

## Install

```sh
cargo install --git https://github.com/CarsonSlovoka/rcron.git
cargo install --list | grep rcron # 查看版本
```


## USAGE


```sh

# 啟動並設定日誌等級為 info
RUST_LOG=info cargo run -- ~/.crontab
# 此時程式會開始監聽 `/tmp/rcron.sock` 並依序執行任務

# 接著可以再開一個終端機來執行以下命令
# 顯示每一個任務，其接下來會執行的5個時間
cargo run -- -l
cargo run -- -l 2  # 同上，但每個任務顯示的數量改為2個

# 離開
cargo run -- -q
```

---

```sh
cargo build --release

cargo install --path .
cargo install --list | grep rcron


RUST_LOG=info rcron example.crontab # 啟動，並指定檔案(預設檔案為: ~/.crontab )
RUST_LOG=info rcron example.crontab & # 可以繼續動作
jobs # 可以看到背景正在執行的工作

# 接下來可以在開一個終端機來做互動
rcron -h
rcron -l
rcron -l 2
rcron -q
```

> [!NOTE] 透過[fg](https://man.archlinux.org/man/fg.1p)可以切換工作


> [!NOTE] 使用[bg](https://man.archlinux.org/man/bg.1p)可以再背景作業

