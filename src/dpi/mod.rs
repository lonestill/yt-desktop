pub mod strategy;

use crate::config::AppConfig;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use strategy::{get_available_strategies, probe_strategy, BenchmarkResult};

pub fn find_tpws_binary() -> Option<PathBuf> {
    let binary_names = ["tpws", "tpws.exe", "ciadpi.exe", "ciadpi"];

    // 1. Next to current executable (inside YouTube.app/Contents/MacOS/ or next to yt-desktop)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for name in &binary_names {
                let p = parent.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    // 2. Local workspace and build directory candidates
    for dir in &["tpws", "target/release", "target/debug", "."] {
        for name in &binary_names {
            let candidate = Path::new(dir).join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

pub fn get_physical_ipv4() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("ipconfig");
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        if let Ok(output) = cmd.output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut in_ignored_adapter = false;
            for line in text.lines() {
                let trimmed = line.trim();
                let lower = line.to_lowercase();
                if lower.contains("adapter") || lower.contains("адаптер") {
                    in_ignored_adapter = lower.contains("prizrak")
                        || lower.contains("tunnel")
                        || lower.contains("vpn")
                        || lower.contains("wintun")
                        || lower.contains("tap")
                        || lower.contains("tailscale")
                        || lower.contains("wireguard")
                        || lower.contains("loopback");
                } else if !in_ignored_adapter && (trimmed.starts_with("IPv4") || trimmed.contains("IPv4-адрес") || trimmed.contains("IPv4 Address")) {
                    if let Some(idx) = trimmed.rfind(':') {
                        let ip = trimmed[idx + 1..].trim();
                        if !ip.starts_with("127.") && !ip.starts_with("169.254.") && !ip.starts_with("198.18.") {
                            return Some(ip.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

pub struct DpiManager {
    pub port: u16,
    pub conn_ip: Option<String>,
    tpws_bin: PathBuf,
    child: Mutex<Option<Child>>,
}

impl Drop for DpiManager {
    fn drop(&mut self) {
        if let Ok(mut lock) = self.child.lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

impl DpiManager {
    pub fn stop(&self) {
        if let Ok(mut lock) = self.child.lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    pub fn new(initial_config: &AppConfig) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let tpws_bin = find_tpws_binary()
            .ok_or_else(|| "Anti-DPI binary (tpws / ciadpi.exe) not found in bundle or workspace".to_string())?;

        let port = {
            let l = TcpListener::bind("127.0.0.1:0")?;
            l.local_addr()?.port()
        };

        let conn_ip = get_physical_ipv4();
        if let Some(ref ip) = conn_ip {
            println!("[DPI-BYPASS] 🌐 Physical interface IP detected: {ip}");
        }

        let manager = Arc::new(Self {
            port,
            conn_ip,
            tpws_bin,
            child: Mutex::new(None),
        });

        let active_id = if initial_config.selected_strategy == "auto" {
            &initial_config.active_strategy
        } else {
            &initial_config.selected_strategy
        };

        if let Err(e) = manager.spawn_tpws(active_id, &initial_config.custom_tpws_args) {
            eprintln!("[DPI-BYPASS] ⚠️ Initial spawn of strategy '{active_id}' failed: {e}");
            if let Err(e2) = manager.spawn_tpws("rostelecom", "") {
                eprintln!("[DPI-BYPASS] ⚠️ Fallback to rostelecom failed: {e2}. Falling back to direct...");
                let _ = manager.spawn_tpws("direct", "");
            }
        }

        Ok(manager)
    }

    pub fn spawn_tpws(&self, preset_id: &str, custom_args: &str) -> Result<(), String> {
        let presets = get_available_strategies();
        let is_ciadpi = self
            .tpws_bin
            .file_name()
            .and_then(|f| f.to_str())
            .map(|s| s.contains("ciadpi"))
            .unwrap_or(false);

        let args_to_use: Vec<String> = if preset_id == "custom" {
            custom_args
                .split_whitespace()
                .map(|s| s.to_string())
                .collect()
        } else {
            let preset = presets
                .iter()
                .find(|p| p.id == preset_id)
                .ok_or_else(|| format!("Unknown strategy ID: {preset_id}"))?;
            if is_ciadpi && !preset.win_args.is_empty() {
                preset.win_args.clone()
            } else {
                preset.args.clone()
            }
        };

        let mut lock = self.child.lock().map_err(|e| e.to_string())?;

        if let Some(mut old_child) = lock.take() {
            let _ = old_child.kill();
            let _ = old_child.wait();
            thread::sleep(Duration::from_millis(50));
        }

        let mut cmd = Command::new(&self.tpws_bin);
        if is_ciadpi {
            cmd.args([
                "-i",
                "127.0.0.1",
                "-p",
                &self.port.to_string(),
            ]);
            if let Some(ref ip) = self.conn_ip {
                cmd.args(["-I", ip]);
            }
        } else {
            cmd.args([
                "--socks",
                "--bind-addr=127.0.0.1",
                &format!("--port={}", self.port),
                "--debug=1",
            ]);
        }

        for arg in &args_to_use {
            cmd.arg(arg);
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.spawn() {
            Ok(mut child) => {
                let mut ready = false;
                for _ in 0..100 {
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        return Err(format!("DPI engine exited before becoming ready: {status}"));
                    }
                    if std::net::TcpStream::connect_timeout(
                        &std::net::SocketAddr::from(([127, 0, 0, 1], self.port)),
                        Duration::from_millis(20),
                    ).is_ok() {
                        ready = true;
                        break;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                if !ready {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("DPI engine did not open its SOCKS port".into());
                }
                println!(
                    "[DPI-BYPASS] ⚡ Activated strategy '{}' (engine: {}) on port {}",
                    preset_id,
                    if is_ciadpi { "ciadpi" } else { "tpws" },
                    self.port
                );
                *lock = Some(child);
                thread::sleep(Duration::from_millis(60));
                Ok(())
            }
            Err(e) => Err(format!("Failed to spawn DPI engine: {e}")),
        }
    }

    pub fn apply_strategy(&self, config: &mut AppConfig, target_id: &str) -> Result<String, String> {
        if target_id == "auto" {
            let results = self.run_benchmark();
            let winner = results.iter().filter(|r| r.success).min_by_key(|r| r.latency_ms.unwrap_or(u64::MAX));

            if let Some(w) = winner {
                self.spawn_tpws(&w.strategy_id, &config.custom_tpws_args)?;
                config.selected_strategy = "auto".to_string();
                config.active_strategy = w.strategy_id.clone();
                config.last_latency_ms = w.latency_ms;
                Ok(w.strategy_id.clone())
            } else {
                Err("Все стратегии заблокированы ТСПУ".to_string())
            }
        } else {
            self.spawn_tpws(target_id, &config.custom_tpws_args)?;
            config.selected_strategy = target_id.to_string();
            config.active_strategy = target_id.to_string();
            Ok(target_id.to_string())
        }
    }

    pub fn run_benchmark(&self) -> Vec<BenchmarkResult> {
        let presets = get_available_strategies();
        let tpws_bin = self.tpws_bin.clone();
        let pid = std::process::id() as u16;
        let conn_ip = self.conn_ip.clone();

        println!("[DPI-BENCHMARK] 🚀 Probing {} strategies in parallel...", presets.len());

        let mut handles = Vec::new();
        for (i, preset) in presets.into_iter().enumerate() {
            let bin = tpws_bin.clone();
            let ip_clone = conn_ip.clone();
            let test_port = 52000 + (pid % 5000) + (i as u16 * 10);
            handles.push(thread::spawn(move || probe_strategy(&bin, &preset, test_port, ip_clone.as_deref())));
        }

        let mut results = Vec::new();
        for h in handles {
            if let Ok(res) = h.join() {
                results.push(res);
            }
        }

        results
    }

    pub fn benchmark_and_select_best(tpws_bin: &PathBuf, config: &mut AppConfig) -> Vec<BenchmarkResult> {
        let presets = get_available_strategies();
        let pid = std::process::id() as u16;
        let conn_ip = get_physical_ipv4();

        println!("[DPI-STARTUP] 🚀 Probing {} strategies in parallel to find best route...", presets.len());

        let mut handles = Vec::new();
        for (i, preset) in presets.into_iter().enumerate() {
            let bin = tpws_bin.clone();
            let ip_clone = conn_ip.clone();
            let test_port = 52000 + (pid % 5000) + (i as u16 * 10);
            handles.push(thread::spawn(move || probe_strategy(&bin, &preset, test_port, ip_clone.as_deref())));
        }

        let mut results = Vec::new();
        for h in handles {
            if let Ok(res) = h.join() {
                results.push(res);
            }
        }

        let winner = results.iter().filter(|r| r.success).min_by_key(|r| r.latency_ms.unwrap_or(u64::MAX)).cloned();
        if let Some(w) = winner {
            println!("[DPI-STARTUP] 🏆 Best route selected: '{}' (latency: {:?} ms)", w.strategy_id, w.latency_ms);
            config.active_strategy = w.strategy_id.clone();
            config.last_latency_ms = w.latency_ms;
        } else {
            println!("[DPI-STARTUP] ⚠️ No working routes responded to probe, keeping '{}'", config.active_strategy);
        }

        results
    }

    pub fn get_status_data(&self, config: &AppConfig) -> serde_json::Value {
        let presets = get_available_strategies();

        serde_json::json!({
            "port": self.port,
            "selected_strategy": config.selected_strategy,
            "active_strategy": config.active_strategy,
            "last_latency_ms": config.last_latency_ms,
            "custom_tpws_args": config.custom_tpws_args,
            "presets": presets,
        })
    }
}
