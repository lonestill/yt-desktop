pub mod strategy;

use crate::config::AppConfig;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use strategy::{get_available_strategies, probe_strategy, BenchmarkResult};

fn find_tpws_binary() -> Option<PathBuf> {
    // 1. Next to current executable (inside YouTube.app/Contents/MacOS/tpws)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("tpws");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    // 2. Local workspace candidates
    let candidate1 = Path::new("tpws/tpws");
    if candidate1.is_file() {
        return Some(candidate1.to_path_buf());
    }
    let candidate2 = Path::new("target/release/tpws");
    if candidate2.is_file() {
        return Some(candidate2.to_path_buf());
    }
    None
}

pub struct DpiManager {
    pub port: u16,
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
    pub fn new(initial_config: &AppConfig) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let tpws_bin = find_tpws_binary()
            .ok_or_else(|| "Native tpws binary not found in bundle or workspace".to_string())?;

        let port = {
            let l = TcpListener::bind("127.0.0.1:0")?;
            l.local_addr()?.port()
        };

        let manager = Arc::new(Self {
            port,
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
            let _ = manager.spawn_tpws("rostelecom", "");
        }

        Ok(manager)
    }

    pub fn spawn_tpws(&self, preset_id: &str, custom_args: &str) -> Result<(), String> {
        let presets = get_available_strategies();
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
            preset.args.clone()
        };

        let mut lock = self.child.lock().map_err(|e| e.to_string())?;

        if let Some(mut old_child) = lock.take() {
            let _ = old_child.kill();
            let _ = old_child.wait();
            thread::sleep(Duration::from_millis(50));
        }

        let mut cmd = Command::new(&self.tpws_bin);
        cmd.args([
            "--socks",
            "--bind-addr=127.0.0.1",
            &format!("--port={}", self.port),
            "--debug=1",
        ]);
        for arg in &args_to_use {
            cmd.arg(arg);
        }

        match cmd.spawn() {
            Ok(child) => {
                println!(
                    "[DPI-BYPASS] ⚡ Activated strategy '{}' on port {}",
                    preset_id, self.port
                );
                *lock = Some(child);
                thread::sleep(Duration::from_millis(60));
                Ok(())
            }
            Err(e) => Err(format!("Failed to spawn tpws: {e}")),
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

        println!("[DPI-BENCHMARK] 🚀 Probing {} strategies in parallel...", presets.len());

        let mut handles = Vec::new();
        for preset in presets {
            let bin = tpws_bin.clone();
            handles.push(thread::spawn(move || probe_strategy(&bin, &preset)));
        }

        let mut results = Vec::new();
        for h in handles {
            if let Ok(res) = h.join() {
                results.push(res);
            }
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
