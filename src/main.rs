mod dpi;

use dpi::strategy::BenchmarkResult;
use dpi::DpiManager;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Theme, Window, WindowBuilder},
};
use wry::{BackgroundThrottlingPolicy, ProxyConfig, ProxyEndpoint, WebContext, WebViewBuilder};

const INJECT_CSS: &str = include_str!("styles.css");
const INJECT_JS: &str = include_str!("inject.js");

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum IpcMessage {
    #[serde(rename = "title")]
    Title { value: String },
    #[serde(rename = "log")]
    Log { level: String, message: String },
    #[serde(rename = "get_dpi_status")]
    GetDpiStatus,
    #[serde(rename = "set_dpi_strategy")]
    SetDpiStrategy { strategy_id: String },
    #[serde(rename = "run_dpi_benchmark")]
    RunDpiBenchmark,
}

enum AppEvent {
    ChangeTitle(String),
    Log { level: String, message: String },
    SendDpiStatus(serde_json::Value),
    SendBenchmarkResults(Vec<BenchmarkResult>, serde_json::Value),
}

#[derive(Serialize, Deserialize, Default)]
struct WindowState {
    width: f64,
    height: f64,
    x: Option<f64>,
    y: Option<f64>,
}

fn get_app_dir() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("yt-desktop");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn load_window_state(app_dir: &PathBuf) -> WindowState {
    let state_file = app_dir.join("window_state.json");
    if let Ok(data) = fs::read_to_string(state_file) {
        if let Ok(state) = serde_json::from_str(&data) {
            return state;
        }
    }
    WindowState {
        width: 1280.0,
        height: 820.0,
        x: None,
        y: None,
    }
}

fn save_window_state(app_dir: &PathBuf, window: &Window) {
    let size = window.inner_size().to_logical::<f64>(window.scale_factor());
    let pos = window.outer_position().ok().map(|p| p.to_logical::<f64>(window.scale_factor()));

    let state = WindowState {
        width: size.width,
        height: size.height,
        x: pos.as_ref().map(|p| p.x),
        y: pos.as_ref().map(|p| p.y),
    };

    let state_file = app_dir.join("window_state.json");
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(state_file, json);
    }
}

fn main() {
    let app_dir = get_app_dir();
    let window_state = load_window_state(&app_dir);

    // 1. Initialize embedded DPI bypass proxy engine
    let dpi_manager = match DpiManager::new() {
        Ok(mgr) => mgr,
        Err(e) => {
            eprintln!("[DPI-BYPASS] ❌ Fatal: failed to initialize DPI engine: {e}");
            std::process::exit(1);
        }
    };

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let mut builder = WindowBuilder::new()
        .with_title("YouTube")
        .with_theme(Some(Theme::Dark))
        .with_inner_size(LogicalSize::new(window_state.width, window_state.height))
        .with_min_inner_size(LogicalSize::new(640.0, 420.0));

    if let (Some(x), Some(y)) = (window_state.x, window_state.y) {
        builder = builder.with_position(LogicalPosition::new(x, y));
    }

    let window = builder.build(&event_loop).expect("Failed to create native window");

    // Persist web profile (cookies, localstorage, cache)
    let web_data_dir = app_dir.join("web_profile");
    let _ = fs::create_dir_all(&web_data_dir);
    let mut web_context = WebContext::new(Some(web_data_dir));

    // Combine CSS injection and JS runtime into initialization script
    let init_script = format!(
        r#"
        window.__YT_DESKTOP_CSS__ = {};
        {}
        "#,
        serde_json::to_string(INJECT_CSS).unwrap_or_else(|_| "\"\"".to_string()),
        INJECT_JS
    );

    let proxy_port = dpi_manager.port;

    let webview_builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_autoplay(true)
        .with_back_forward_navigation_gestures(true)
        .with_accept_first_mouse(true)
        .with_devtools(true)
        .with_background_throttling(BackgroundThrottlingPolicy::Disabled)
        .with_user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15",
        )
        .with_initialization_script(&init_script)
        .with_proxy_config(ProxyConfig::Socks5(ProxyEndpoint {
            host: "127.0.0.1".to_string(),
            port: proxy_port.to_string(),
        }));

    let dpi_clone = Arc::clone(&dpi_manager);
    let ipc_proxy = proxy.clone();

    let webview = webview_builder
        .with_ipc_handler(move |req| {
            let body = req.body();
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(body) {
                match msg {
                    IpcMessage::Title { value } => {
                        let _ = ipc_proxy.send_event(AppEvent::ChangeTitle(value));
                    }
                    IpcMessage::Log { level, message } => {
                        let _ = ipc_proxy.send_event(AppEvent::Log { level, message });
                    }
                    IpcMessage::GetDpiStatus => {
                        let status = dpi_clone.get_status_data();
                        let _ = ipc_proxy.send_event(AppEvent::SendDpiStatus(status));
                    }
                    IpcMessage::SetDpiStrategy { strategy_id } => {
                        let mgr = Arc::clone(&dpi_clone);
                        let p = ipc_proxy.clone();
                        thread::spawn(move || {
                            let _ = mgr.set_strategy(&strategy_id);
                            let status = mgr.get_status_data();
                            let _ = p.send_event(AppEvent::SendDpiStatus(status));
                        });
                    }
                    IpcMessage::RunDpiBenchmark => {
                        let mgr = Arc::clone(&dpi_clone);
                        let p = ipc_proxy.clone();
                        thread::spawn(move || {
                            let results = mgr.run_benchmark();
                            let winner = results.iter().filter(|r| r.success).min_by_key(|r| r.latency_ms.unwrap_or(u64::MAX));
                            if let Some(w) = winner {
                                let _ = mgr.set_strategy(&w.strategy_id);
                            }
                            let status = mgr.get_status_data();
                            let _ = p.send_event(AppEvent::SendBenchmarkResults(results, status));
                        });
                    }
                }
            }
        })
        .with_url("https://www.youtube.com")
        .build(&window)
        .expect("Failed to initialize WebKit Webview");

    let app_dir_clone = app_dir.clone();
    let _dpi_guard = dpi_manager;

    event_loop.run(move |event, _, control_flow| {
        let _ = &_dpi_guard;
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(AppEvent::ChangeTitle(mut title)) => {
                if !title.ends_with("YouTube") {
                    if title.is_empty() {
                        title = "YouTube".to_string();
                    } else {
                        title = format!("{title} - YouTube");
                    }
                }
                window.set_title(&title);
            }
            Event::UserEvent(AppEvent::Log { level, message }) => {
                println!("[WEBVIEW-{level}] {message}");
            }
            Event::UserEvent(AppEvent::SendDpiStatus(status)) => {
                let js = format!("if (window.__onDpiStatus) window.__onDpiStatus({});", status);
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(AppEvent::SendBenchmarkResults(results, status)) => {
                let res_json = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
                let js = format!(
                    "if (window.__onDpiBenchmarkDone) window.__onDpiBenchmarkDone({}, {});",
                    res_json, status
                );
                let _ = webview.evaluate_script(&js);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                save_window_state(&app_dir_clone, &window);
                *control_flow = ControlFlow::Exit;
            }
            _ => (),
        }
    });
}
