use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, FixedOffset, Local, Utc};
use colored::*;
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
use tokio::sync::broadcast;
use tokio::time::{Duration, sleep};

rust_i18n::i18n!("locales", fallback = "en");
pub use rust_i18n::t;
// use lib::{i18n::init};

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

/// 定義時區模式
#[derive(Clone, Copy)]
enum TimeMode {
    Local,
    Utc(FixedOffset),
}

impl TimeMode {
    /// 獲取當前時間（根據設定的時區）
    fn now(&self) -> DateTime<FixedOffset> {
        match self {
            TimeMode::Local => {
                let local_now = Local::now();
                let offset = *local_now.offset();
                local_now.with_timezone(&offset)
            }
            TimeMode::Utc(offset) => Utc::now().with_timezone(offset),
        }
    }
}

const SOCKET_PATH: &str = "/tmp/rcron.sock";

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    lib::i18n::init();

    let args: Vec<String> = env::args().collect();

    // 檢查是否為客戶端模式
    if args.len() > 1 && args[1].starts_with('-') && args[1] != "-utc" {
        return run_client(&args).await;
    }

    run_server(args).await
}

async fn run_client(args: &[String]) -> Result<()> {
    let help = format!(
        "
{}
-q\t{}
-l [N]\t{}
",
        t!("usage.usage"),
        t!("usage.stop_server"),
        t!("usage.l_help"),
    );
    let cmd = match args[1].as_str() {
        "-q" => DaemonCommand::Quit,
        "-l" => {
            let n = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
            DaemonCommand::List(n)
        }
        "-h" | "--help" => {
            println!("{:?}", help);
            return Ok(());
        }

        _ => {
            println!("{}", t!("err.unknown_para", usage = help));
            return Ok(());
        }
    };

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context(t!("err.socket_connect"))?;

    let payload = serde_json::to_vec(&cmd)?;
    stream.write_all(&payload).await?;
    stream.shutdown().await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    println!("{}", response);

    Ok(())
}

async fn run_server(args: Vec<String>) -> Result<()> {
    println!("{}", t!("msg.server-running", help = "rcron -h".green()));

    // 解析時區參數與檔案路徑
    let mut time_mode = TimeMode::Local;
    let mut _crontab_path = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-utc" => {
                let offset_hours = if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    let val = args[i + 1]
                        .parse::<i32>()
                        .context(t!("err.invalid_timezone_offset").red())?;
                    i += 1;
                    val
                } else {
                    0 // 預設 UTC+0
                };
                let offset = FixedOffset::east_opt(offset_hours * 3600)
                    .ok_or_else(|| anyhow!(t!("err.timezone_overflow").red()))?;
                time_mode = TimeMode::Utc(offset);
            }
            path if !path.starts_with('-') => {
                _crontab_path = Some(path.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    match time_mode {
        TimeMode::Local => log::info!("{}", t!("msg.which-timezone", local = "Local")),
        // TimeMode::Utc(o) => log::info!("使用時區: UTC{:?}({}秒)", o, o.local_minus_utc()),
        TimeMode::Utc(o) => log::info!(
            "{}",
            t!(
                "msg.which-timezone",
                local = format!("UTC{:?} ({}{})", o, o.local_minus_utc(), t!("time.sec"))
            )
        ),
    }

    let crontab_path = env::args()
        .nth(1)
        .filter(|a| !a.starts_with('-'))
        .unwrap_or_else(|| format!("{}/.crontab", env::var("HOME").unwrap()));

    let content = fs::read_to_string(&crontab_path).with_context(|| {
        format!(
            "{}",
            t!(
                "err.unable_read_file",
                filename = "crontab",
                path = crontab_path.red(),
            )
        )
    })?;

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
            let mut cron_parts: Vec<String> = parts[0..6].iter().map(|s| s.to_string()).collect();

            // 取得「週」這個欄位 (索引為 5)
            let dow = &cron_parts[5]; // day of week

            // 不改前的對應: 1-7 對應 日一二三...六
            //                 要改成 一二三...  日
            // 即1=>2, 2=>3, ...6=>7
            // https://crontab.guru/#5_4_*_*_0
            // https://crontab.guru/#5_4_*_*_7 👈 這個有些cron是不支持的.
            if let Ok(val) = dow.parse::<u32>() {
                let converted_dow = match val {
                    0 | 7 => 1,       // 0,7都是指星期日(Sun)
                    1..=6 => val + 1, // 1-6 變 2-7
                    _ => val,         // 超出範圍交給 Schedule::from_str 報錯
                };
                cron_parts[5] = converted_dow.to_string();
            }

            let schedule_str = cron_parts.join(" ");
            let cmd_str = parts[6..].join(" ");

            match Schedule::from_str(&schedule_str) {
                Ok(schedule) => {
                    jobs.push(Job {
                        schedule,
                        cmd: cmd_str,
                    });
                }
                Err(e) => log::error!("{}", t!("err.parse_error", line = line, err = e)),
            }
        } else {
            log::warn!("{}", t!("err.unknown_timeformat", line = line.red()));
        }
    }

    if jobs.is_empty() {
        log::warn!("{}", t!("warn.do_not_import_anything").yellow());
    }

    let shared_jobs = Arc::new(jobs);
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    for job in shared_jobs.iter() {
        let job = job.clone();
        let mut stop = shutdown_tx.subscribe();
        let current_time_mode = time_mode;

        // 每一個任務配一個coroutines來處理. 併發跑
        tokio::spawn(async move {
            loop {
                let now = current_time_mode.now();

                // job.schedule.upcoming會計算出下一次需要執行的時間
                // if let Some(next) = job.schedule.upcoming(Utc).next() {
                if let Some(next) = job
                    .schedule
                    .upcoming(current_time_mode.now().timezone())
                    .next()
                {
                    // 計算出需要等待的時間，避免每秒檢查
                    let sleep_duration = if next > now {
                        (next - now).to_std().unwrap_or(Duration::from_secs(0))
                    } else {
                        Duration::from_secs(0)
                    };

                    tokio::select! {
                        // 該任務會sleep直到需要執行的時候再喚起
                        _ = sleep(sleep_duration) => {
                            log::info!("{}", t!("msg.run-command", cmd = job.cmd.green()));
                            if let Err(e) = Command::new("sh").arg("-c").arg(&job.cmd).status() {
                                log::error!("{}", t!("err.run_cmd_error", err=format!("{}", e).red()));
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
                let current_time_mode = time_mode;
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
                                let now = current_time_mode.now();
                                for job in jobs_ref.iter() {
                                    res.push_str(&format!("- Command: {}\n", job.cmd));
                                    // for time in job.schedule.upcoming(Utc).take(n) {
                                    for time in job.schedule.upcoming(now.timezone()).take(n) {
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
