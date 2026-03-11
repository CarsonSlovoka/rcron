use anyhow::{Context, Result};
use chrono::{Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast};
use tokio::time::{sleep, Duration};

#[derive(Serialize, Deserialize, Debug)]
enum DaemonCommand {
    List(usize),
    Quit,
}

#[derive(Clone)]
struct Job {
    schedule: Schedule,
    cmd: String,
}

const SOCKET_PATH: &str = "/tmp/rcron.sock";

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1].starts_with('-') {
        return run_client(&args).await;
    }

    run_server().await
}

async fn run_client(args: &[String]) -> Result<()> {
    let cmd = match args[1].as_str() {
        "-q" => DaemonCommand::Quit,
        "-l" => {
            let n = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            DaemonCommand::List(n)
        }
        _ => {
            println!("未知參數。用法:\n  -q\t\t停止伺服器\n  -l [N]\t列出接下來 N 個任務");
            return Ok(());
        }
    };

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("無法連接到 Daemon，請確認伺服器是否正在執行")?;

    let payload = serde_json::to_vec(&cmd)?;
    stream.write_all(&payload).await?;
    stream.shutdown().await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    println!("{}", response);

    Ok(())
}

async fn run_server() -> Result<()> {
    log::info!("Rust Crontab Daemon 啟動！");

    let crontab_path = env::args()
        .nth(1)
        .filter(|a| !a.starts_with('-'))
        .unwrap_or_else(|| format!("{}/.crontab", env::var("HOME").unwrap()));

    let content = fs::read_to_string(&crontab_path)
        .with_context(|| format!("無法讀取 crontab 檔案: {}", crontab_path))?;

    let mut jobs = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // 格式: 秒 分 時 日 月 週 指令
        // 前 6 個部分組成 Cron 表達式，剩下的部分是指令
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 7 {
            let schedule_str = parts[0..6].join(" ");
            let cmd_str = parts[6..].join(" ");

            match Schedule::from_str(&schedule_str) {
                Ok(schedule) => {
                    jobs.push(Job {
                        schedule,
                        cmd: cmd_str,
                    });
                }
                Err(e) => log::error!("解析失敗 行 '{}': {}", line, e),
            }
        } else {
            log::warn!("格式不正確 (需要 6 位時間格式 + 指令): {}", line);
        }
    }

    if jobs.is_empty() {
        log::warn!("警告：沒有載入任何有效的任務！");
    }

    let shared_jobs = Arc::new(jobs);
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    for job in shared_jobs.iter() {
        let job = job.clone();
        let mut stop = shutdown_tx.subscribe();
        tokio::spawn(async move {
            loop {
                if let Some(next) = job.schedule.upcoming(Utc).next() {
                    let now = Utc::now();
                    let sleep_duration = if next > now {
                        (next - now).to_std().unwrap_or(Duration::from_secs(0))
                    } else {
                        Duration::from_secs(0)
                    };

                    tokio::select! {
                        _ = sleep(sleep_duration) => {
                            log::info!("執行任務: {}", job.cmd);
                            if let Err(e) = Command::new("sh").arg("-c").arg(&job.cmd).status() {
                                log::error!("任務執行報錯: {}", e);
                            }
                            sleep(Duration::from_secs(1)).await;
                        }
                        _ = stop.recv() => break,
                    }
                }
            }
        });
    }

    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }
    let listener = UnixListener::bind(SOCKET_PATH)?;

    loop {
        tokio::select! {
            Ok((mut stream, _)) = listener.accept() => {
                let jobs_ref = shared_jobs.clone();
                let tx = shutdown_tx.clone();
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    stream.read_to_end(&mut buffer).await.ok();
                    if let Ok(cmd) = serde_json::from_slice::<DaemonCommand>(&buffer) {
                        match cmd {
                            DaemonCommand::Quit => {
                                let _ = tx.send(());
                                stream.write_all(b"Daemon closed.\n").await.ok();
                                sleep(Duration::from_millis(100)).await;
                                std::process::exit(0);
                            },
                            DaemonCommand::List(n) => {
                                let mut res = String::from("Scheduled Jobs:\n");
                                if jobs_ref.is_empty() {
                                    res.push_str("(No jobs loaded)\n");
                                }
                                for job in jobs_ref.iter() {
                                    res.push_str(&format!("- Command: {}\n", job.cmd));
                                    for time in job.schedule.upcoming(Utc).take(n) {
                                        res.push_str(&format!("  Next: {}\n", time.to_rfc3339()));
                                    }
                                }
                                stream.write_all(res.as_bytes()).await.ok();
                            }
                        }
                    }
                });
            }
        }
    }
}
