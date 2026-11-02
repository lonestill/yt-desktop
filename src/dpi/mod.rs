pub mod strategy;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use strategy::{
    get_available_strategies, load_dpi_settings, probe_strategy, save_dpi_settings,
    BenchmarkResult, DpiSettings,
};

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
    pub settings: Mutex<DpiSettings>,
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
    pub fn new() -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let tpws_bin = find_tpws_binary()
            .ok_or_else(|| "Native tpws binary not found in bundle or workspace".to_string())?;

        // Allocate a persistent local port that WebKit will use for SOCKS5
        let port = {
            let l = TcpListener::bind("127.0.0.1:0")?;
            l.local_addr()?.port()
        };

        let settings = load_dpi_settings();

        let manager = Arc::new(Self {
            port,
            tpws_bin,
            child: Mutex::new(None),
            settings: Mutex::new(settings),
        });

        // Initialize active strategy
        let active_id = {
            let s = manager.settings.lock().unwrap();
            if s.selected_strategy == "auto" {
                s.active_strategy.clone()
            } else {
                s.selected_strategy.clone()
            }
        };

        if let Err(e) = manager.spawn_tpws(&active_id) {
            eprintln!("[DPI-BYPASS] ⚠️ Initial spawn of strategy '{active_id}' failed: {e}");
            // Fallback to default rostelecom
            let _ = manager.spawn_tpws("rostelecom");
        }

        Ok(manager)
    }

    fn spawn_tpws(&self, preset_id: &str) -> Result<(), String> {
        let presets = get_available_strategies();
        let preset = presets
            .iter()
            .find(|p| p.id == preset_id)
            .ok_or_else(|| format!("Unknown strategy ID: {preset_id}"))?;

        let mut lock = self.child.lock().map_err(|e| e.to_string())?;

        // Terminate existing child process
        if let Some(mut old_child) = lock.take() {
            let _ = old_child.kill();
            let _ = old_child.wait();
            // Tiny pause to ensure kernel socket teardown
            thread::sleep(Duration::from_millis(50));
        }

        let mut cmd = Command::new(&self.tpws_bin);
        cmd.args([
            "--socks",
            "--bind-addr=127.0.0.1",
            &format!("--port={}", self.port),
            "--debug=1",
        ]);
        for arg in &preset.args {
            cmd.arg(arg);
        }

        match cmd.spawn() {
            Ok(child) => {
                println!(
                    "[DPI-BYPASS] ⚡ Activated strategy '{}' ({}) on port {}",
                    preset.name, preset.id, self.port
                );
                *lock = Some(child);

                // Update settings
                if let Ok(mut s) = self.settings.lock() {
                    s.active_strategy = preset.id.clone();
                    save_dpi_settings(&s);
                }
                thread::sleep(Duration::from_millis(80));
                Ok(())
            }
            Err(e) => Err(format!("Failed to spawn tpws: {e}")),
        }
    }

    pub fn set_strategy(&self, target_id: &str) -> Result<String, String> {
        if target_id == "auto" {
            // Run parallel benchmark and apply winner
            let results = self.run_benchmark();
            let winner = results.iter().filter(|r| r.success).min_by_key(|r| r.latency_ms.unwrap_or(u64::MAX));

            if let Some(w) = winner {
                self.spawn_tpws(&w.strategy_id)?;
                if let Ok(mut s) = self.settings.lock() {
                    s.selected_strategy = "auto".to_string();
                    s.active_strategy = w.strategy_id.clone();
                    s.last_latency_ms = w.latency_ms;
                    save_dpi_settings(&s);
                }
                Ok(w.strategy_id.clone())
            } else {
                Err("Все стратегии не смогли подключиться к YouTube".to_string())
            }
        } else {
            self.spawn_tpws(target_id)?;
            if let Ok(mut s) = self.settings.lock() {
                s.selected_strategy = target_id.to_string();
                s.active_strategy = target_id.to_string();
                save_dpi_settings(&s);
            }
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
                println!(
                    "[DPI-BENCHMARK] -> {}: success={}, latency={:?}, error={:?}",
                    res.strategy_id, res.success, res.latency_ms, res.error
                );
                results.push(res);
            }
        }

        results
    }

    pub fn get_status_data(&self) -> serde_json::Value {
        let settings = self.settings.lock().unwrap().clone();
        let presets = get_available_strategies();

        serde_json::json!({
            "port": self.port,
            "selected_strategy": settings.selected_strategy,
            "active_strategy": settings.active_strategy,
            "last_latency_ms": settings.last_latency_ms,
            "presets": presets,
        })
    }
}
