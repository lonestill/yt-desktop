use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategyPreset {
    pub id: String,
    pub name: String,
    pub isp_hint: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub win_args: Vec<String>,
}

pub fn get_available_strategies() -> Vec<StrategyPreset> {
    vec![
        StrategyPreset {
            id: "rostelecom".to_string(),
            name: "Ростелеком / Дом.ру".to_string(),
            isp_hint: "split=1 + disorder=3+s (лучший под ТСПУ Ростелеком)".to_string(),
            args: vec![
                "--split-pos=midsld".to_string(),
                "--disorder=tls".to_string(),
            ],
            win_args: vec![
                "-U".to_string(),
                "-s".to_string(),
                "1".to_string(),
                "-d".to_string(),
                "3+s".to_string(),
            ],
        },
        StrategyPreset {
            id: "mts_megafon".to_string(),
            name: "МТС / Мегафон / Yota".to_string(),
            isp_hint: "split=1+s + disoob=3+s (обход DPI через TCP Disoob)".to_string(),
            args: vec![
                "--split-pos=midsld".to_string(),
                "--oob=tls".to_string(),
            ],
            win_args: vec![
                "-U".to_string(),
                "-s".to_string(),
                "1+s".to_string(),
                "-q".to_string(),
                "3+s".to_string(),
            ],
        },
        StrategyPreset {
            id: "beeline_mgts".to_string(),
            name: "Билайн / МГТС / Теле2".to_string(),
            isp_hint: "split=1+s + disorder=3+s (первый байт SNI + disorder)".to_string(),
            args: vec![
                "--split-pos=1".to_string(),
                "--disorder=tls".to_string(),
            ],
            win_args: vec![
                "-U".to_string(),
                "-s".to_string(),
                "1+s".to_string(),
                "-d".to_string(),
                "3+s".to_string(),
            ],
        },
        StrategyPreset {
            id: "tlsrec_midsld".to_string(),
            name: "TLS Disoob (SLD)".to_string(),
            isp_hint: "split=1 + disoob=1 (рассинхронизация сессии)".to_string(),
            args: vec![
                "--split-pos=midsld".to_string(),
                "--tlsrec=midsld".to_string(),
            ],
            win_args: vec![
                "-U".to_string(),
                "-s".to_string(),
                "1".to_string(),
                "-q".to_string(),
                "1".to_string(),
            ],
        },
        StrategyPreset {
            id: "tlsrec_sni".to_string(),
            name: "Fake Packet (TTL)".to_string(),
            isp_hint: "fake packet с низким TTL (обход через фейковый SNI)".to_string(),
            args: vec![
                "--split-pos=1".to_string(),
                "--tlsrec=sniext+2".to_string(),
            ],
            win_args: vec![
                "-U".to_string(),
                "-f".to_string(),
                "-1".to_string(),
                "-t".to_string(),
                "4".to_string(),
            ],
        },
        StrategyPreset {
            id: "direct".to_string(),
            name: "Прямое подключение".to_string(),
            isp_hint: "Без модификации пакетов (для VPN)".to_string(),
            args: vec![],
            win_args: vec![],
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResult {
    pub strategy_id: String,
    pub strategy_name: String,
    pub success: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Tests a specific strategy against https://www.youtube.com using a temporary proxy instance
pub fn probe_strategy(tpws_bin: &PathBuf, preset: &StrategyPreset, test_port: u16, conn_ip: Option<&str>) -> BenchmarkResult {
    let is_ciadpi = tpws_bin
        .file_name()
        .and_then(|f| f.to_str())
        .map(|s| s.contains("ciadpi"))
        .unwrap_or(false);

    let mut cmd = Command::new(tpws_bin);
    if is_ciadpi {
        cmd.args(["-i", "127.0.0.1", "-p", &test_port.to_string()]);
        if let Some(ip) = conn_ip {
            cmd.args(["-I", ip]);
        }
        let args_to_use = if !preset.win_args.is_empty() {
            &preset.win_args
        } else {
            &preset.args
        };
        for arg in args_to_use {
            cmd.arg(arg);
        }
    } else {
        cmd.args([
            "--socks",
            "--bind-addr=127.0.0.1",
            &format!("--port={test_port}"),
        ]);
        for arg in &preset.args {
            cmd.arg(arg);
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return BenchmarkResult {
                strategy_id: preset.id.clone(),
                strategy_name: preset.name.clone(),
                success: false,
                latency_ms: None,
                error: Some(format!("Spawn err: {e}")),
            }
        }
    };

    let mut ready = false;
    for _ in 0..50 {
        if let Ok(Some(status)) = child.try_wait() {
            return BenchmarkResult {
                strategy_id: preset.id.clone(),
                strategy_name: preset.name.clone(),
                success: false,
                latency_ms: None,
                error: Some(format!("Exited early: {status}")),
            };
        }
        if std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], test_port)),
            Duration::from_millis(15),
        ).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }

    if !ready {
        let _ = child.kill();
        let _ = child.wait();
        return BenchmarkResult {
            strategy_id: preset.id.clone(),
            strategy_name: preset.name.clone(),
            success: false,
            latency_ms: None,
            error: Some("Port bind timeout".into()),
        };
    }

    let start = Instant::now();
    let mut curl_cmd = Command::new("curl");
    curl_cmd.args([
        "-s",
        "-I",
        "-o",
        if cfg!(target_os = "windows") { "NUL" } else { "/dev/null" },
        "-w",
        "%{http_code}",
        "--connect-timeout",
        "2",
        "--max-time",
        "3",
        "-x",
        &format!("socks5h://127.0.0.1:{test_port}"),
        "https://www.youtube.com",
    ]);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        curl_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let curl_output = curl_cmd.output();
    // A reachable homepage alone does not establish access to Google's video service.
    let media_output = if curl_output.as_ref().map(|out| out.status.success()).unwrap_or(false) {
        let mut media_cmd = Command::new("curl");
        media_cmd.args(["-s", "-o", if cfg!(target_os = "windows") { "NUL" } else { "/dev/null" },
            "-w", "%{http_code}", "--connect-timeout", "3", "--max-time", "5",
            "-x", &format!("socks5h://127.0.0.1:{test_port}"), "https://redirector.googlevideo.com/"]);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            media_cmd.creation_flags(0x08000000);
        }
        media_cmd.output().ok()
    } else { None };
    let media_reachable = media_output.as_ref().map(|out| {
        out.status.success() && String::from_utf8_lossy(&out.stdout).trim() != "000"
    }).unwrap_or(false);

    let _ = child.kill();
    let _ = child.wait();

    match curl_output {
        Ok(out) => {
            let code_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let elapsed_ms = start.elapsed().as_millis() as u64;
            if media_reachable && out.status.success() && (code_str == "200" || code_str.starts_with("30")) {
                BenchmarkResult {
                    strategy_id: preset.id.clone(),
                    strategy_name: preset.name.clone(),
                    success: true,
                    latency_ms: Some(elapsed_ms),
                    error: None,
                }
            } else {
                BenchmarkResult {
                    strategy_id: preset.id.clone(),
                    strategy_name: preset.name.clone(),
                    success: false,
                    latency_ms: None,
                    error: Some(if code_str.is_empty() {
                        "Timeout/Blocked".to_string()
                    } else {
                        format!("HTTP {code_str}")
                    }),
                }
            }
        }
        Err(e) => BenchmarkResult {
            strategy_id: preset.id.clone(),
            strategy_name: preset.name.clone(),
            success: false,
            latency_ms: None,
            error: Some(format!("Curl exec err: {e}")),
        },
    }
}
