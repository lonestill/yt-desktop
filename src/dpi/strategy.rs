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
}

pub fn get_available_strategies() -> Vec<StrategyPreset> {
    vec![
        StrategyPreset {
            id: "rostelecom".to_string(),
            name: "Ростелеком / Дом.ру".to_string(),
            isp_hint: "midsld + disorder=tls (лучший под ТСПУ Ростелеком)".to_string(),
            args: vec![
                "--split-pos=midsld".to_string(),
                "--disorder=tls".to_string(),
            ],
        },
        StrategyPreset {
            id: "mts_megafon".to_string(),
            name: "МТС / Мегафон / Yota".to_string(),
            isp_hint: "midsld + oob=tls (обход DPI через TCP OOB)".to_string(),
            args: vec![
                "--split-pos=midsld".to_string(),
                "--oob=tls".to_string(),
            ],
        },
        StrategyPreset {
            id: "beeline_mgts".to_string(),
            name: "Билайн / МГТС / Теле2".to_string(),
            isp_hint: "split=1 + disorder=tls (первый байт + disorder)".to_string(),
            args: vec![
                "--split-pos=1".to_string(),
                "--disorder=tls".to_string(),
            ],
        },
        StrategyPreset {
            id: "tlsrec_midsld".to_string(),
            name: "TLS Record (SLD)".to_string(),
            isp_hint: "midsld + tlsrec=midsld (деление на TLS записи)".to_string(),
            args: vec![
                "--split-pos=midsld".to_string(),
                "--tlsrec=midsld".to_string(),
            ],
        },
        StrategyPreset {
            id: "tlsrec_sni".to_string(),
            name: "TLS Record (SNI Ext)".to_string(),
            isp_hint: "split=1 + tlsrec=sniext+2".to_string(),
            args: vec![
                "--split-pos=1".to_string(),
                "--tlsrec=sniext+2".to_string(),
            ],
        },
        StrategyPreset {
            id: "direct".to_string(),
            name: "Прямое подключение".to_string(),
            isp_hint: "Без модификации пакетов (для VPN)".to_string(),
            args: vec![],
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

/// Tests a specific strategy against https://www.youtube.com using a temporary tpws instance
pub fn probe_strategy(tpws_bin: &PathBuf, preset: &StrategyPreset, test_port: u16) -> BenchmarkResult {
    let mut cmd = Command::new(tpws_bin);
    cmd.args([
        "--socks",
        "--bind-addr=127.0.0.1",
        &format!("--port={test_port}"),
    ]);
    for arg in &preset.args {
        cmd.arg(arg);
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

    std::thread::sleep(Duration::from_millis(80));

    let start = Instant::now();
    let curl_output = Command::new("curl")
        .args([
            "-s",
            "-I",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--connect-timeout",
            "2",
            "--max-time",
            "3",
            "-x",
            &format!("socks5h://127.0.0.1:{test_port}"),
            "https://www.youtube.com",
        ])
        .output();

    let _ = child.kill();
    let _ = child.wait();

    match curl_output {
        Ok(out) => {
            let code_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let elapsed_ms = start.elapsed().as_millis() as u64;
            if code_str == "200" || code_str.starts_with("30") {
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
                    error: Some(if code_str.is_empty() { "Timeout/Blocked".to_string() } else { format!("HTTP {code_str}") }),
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
