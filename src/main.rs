mod config;
mod dpi;
mod menu;

use config::{get_config_dir, load_config, save_config, AppConfig};
use dpi::strategy::BenchmarkResult;
use dpi::DpiManager;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::macos::{ActivationPolicy, EventLoopExtMacOS},
    window::{Theme, Window, WindowBuilder},
};
use wry::{BackgroundThrottlingPolicy, ProxyConfig, ProxyEndpoint, WebContext, WebView, WebViewBuilder};

const INJECT_CSS: &str = include_str!("styles.css");
const INJECT_JS: &str = include_str!("inject.js");
const SETTINGS_HTML: &str = include_str!("settings.html");

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum MainIpcMessage {
    #[serde(rename = "title")]
    Title { value: String },
    #[serde(rename = "log")]
    Log { level: String, message: String },
    #[serde(rename = "open_settings")]
    OpenSettings,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum SettingsIpcMessage {
    #[serde(rename = "get_settings_state")]
    GetSettingsState,
    #[serde(rename = "save_config")]
    SaveConfig { config: AppConfig },
    #[serde(rename = "set_dpi_strategy")]
    SetDpiStrategy { strategy_id: String },
    #[serde(rename = "run_dpi_benchmark")]
    RunDpiBenchmark,
    #[serde(rename = "close_settings")]
    CloseSettings,
    #[serde(rename = "open_config_folder")]
    OpenConfigFolder,
}

enum AppEvent {
    ChangeTitle(String),
    Log { level: String, message: String },
    OpenSettings,
    CloseSettings,
    MenuItemClicked(String),
    UpdateSettingsUi(serde_json::Value, serde_json::Value),
    BenchmarkDone(Vec<BenchmarkResult>, serde_json::Value),
    ApplyConfigToMain(serde_json::Value),
}

#[derive(Serialize, Deserialize, Default)]
struct WindowState {
    width: f64,
    height: f64,
    x: Option<f64>,
    y: Option<f64>,
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
    let app_dir = get_config_dir();
    let window_state = load_window_state(&app_dir);

    // 1. Load persistent configuration
    let config = Arc::new(Mutex::new(load_config()));

    // 2. Initialize embedded DPI bypass proxy engine
    let dpi_manager = {
        let cfg = config.lock().unwrap();
        match DpiManager::new(&cfg) {
            Ok(mgr) => mgr,
            Err(e) => {
                eprintln!("[DPI-BYPASS] ❌ Fatal: failed to initialize DPI engine: {e}");
                std::process::exit(1);
            }
        }
    };

    // 3. Create Event Loop with explicit macOS Regular Activation Policy
    let mut event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    event_loop.set_activation_policy(ActivationPolicy::Regular);
    event_loop.set_activate_ignoring_other_apps(true);
    event_loop.set_dock_visibility(true);

    let proxy = event_loop.create_proxy();

    // 4. Initialize native macOS Top App MenuBar ("YouTube", "Anti-DPI", "Правка", "Вид", "Окно")
    let app_menu = match menu::AppMenu::init() {
        Ok(m) => {
            let active = config.lock().unwrap().active_strategy.clone();
            m.update_active_strategy(&active);
            Some(m)
        }
        Err(e) => {
            eprintln!("[MENU] ⚠️ Warning: Failed to initialize native menu: {e}");
            None
        }
    };

    // Hook native macOS menu item clicks into Tao event loop
    let menu_proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
        let id_str = event.id().as_ref().to_string();
        let _ = menu_proxy.send_event(AppEvent::MenuItemClicked(id_str));
    }));

    // 5. Create Main YouTube Window
    let mut main_builder = WindowBuilder::new()
        .with_title("YouTube")
        .with_theme(Some(Theme::Dark))
        .with_inner_size(LogicalSize::new(window_state.width, window_state.height))
        .with_min_inner_size(LogicalSize::new(640.0, 420.0));

    if let (Some(x), Some(y)) = (window_state.x, window_state.y) {
        main_builder = main_builder.with_position(LogicalPosition::new(x, y));
    }

    let main_window = main_builder.build(&event_loop).expect("Failed to create native window");
    main_window.set_visible(true);
    main_window.set_focus();
    let main_window_id = main_window.id();

    // Persist web profile for main webview
    let web_data_dir = app_dir.join("web_profile");
    let _ = fs::create_dir_all(&web_data_dir);
    let mut main_web_context = WebContext::new(Some(web_data_dir));

    let init_script = format!(
        r#"
        window.__YT_DESKTOP_CSS__ = {};
        {}
        "#,
        serde_json::to_string(INJECT_CSS).unwrap_or_else(|_| "\"\"".to_string()),
        INJECT_JS
    );

    let proxy_port = dpi_manager.port;

    // Build Main WebView
    let main_ipc_proxy = proxy.clone();
    let main_webview = WebViewBuilder::new_with_web_context(&mut main_web_context)
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
        }))
        .with_ipc_handler(move |req| {
            let body = req.body();
            if let Ok(msg) = serde_json::from_str::<MainIpcMessage>(body) {
                match msg {
                    MainIpcMessage::Title { value } => {
                        let _ = main_ipc_proxy.send_event(AppEvent::ChangeTitle(value));
                    }
                    MainIpcMessage::Log { level, message } => {
                        let _ = main_ipc_proxy.send_event(AppEvent::Log { level, message });
                    }
                    MainIpcMessage::OpenSettings => {
                        let _ = main_ipc_proxy.send_event(AppEvent::OpenSettings);
                    }
                }
            }
        })
        .with_url("https://www.youtube.com")
        .build(&main_window)
        .expect("Failed to initialize Main WebKit Webview");

    // Apply initial config to main webview
    {
        let cfg = config.lock().unwrap();
        if let Ok(cfg_json) = serde_json::to_string(&*cfg) {
            let _ = main_webview.evaluate_script(&format!("if (window.__applyConfig) window.__applyConfig({});", cfg_json));
        }
    }

    let app_dir_clone = app_dir.clone();
    let _dpi_guard = dpi_manager;
    let _menu_guard = app_menu;
    let dpi_clone = Arc::clone(&_dpi_guard);
    let config_clone = Arc::clone(&config);

    // Settings window will be lazily created on demand
    let mut settings_state: Option<(Window, WebView)> = None;

    event_loop.run(move |event, event_target, control_flow| {
        let _ = (&_dpi_guard, &_menu_guard);
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
                main_window.set_title(&title);
            }
            Event::UserEvent(AppEvent::Log { level, message }) => {
                println!("[WEBVIEW-{level}] {message}");
            }
            Event::UserEvent(AppEvent::OpenSettings) => {
                let cfg = config_clone.lock().unwrap();
                let status = dpi_clone.get_status_data(&cfg);
                let cfg_json = serde_json::to_value(&*cfg).unwrap_or_default();

                if settings_state.is_none() {
                    let (pos_x, pos_y) = if let Ok(main_pos) = main_window.outer_position() {
                        let scale = main_window.scale_factor();
                        let logical_pos = main_pos.to_logical::<f64>(scale);
                        let size = main_window.inner_size().to_logical::<f64>(scale);
                        (
                            (logical_pos.x + (size.width - 660.0) / 2.0).max(50.0),
                            (logical_pos.y + (size.height - 480.0) / 2.0).max(50.0),
                        )
                    } else {
                        (150.0, 150.0)
                    };

                    let win = WindowBuilder::new()
                        .with_title("Настройки YouTube")
                        .with_theme(Some(Theme::Dark))
                        .with_inner_size(LogicalSize::new(660.0, 480.0))
                        .with_position(LogicalPosition::new(pos_x, pos_y))
                        .with_resizable(false)
                        .build(event_target)
                        .expect("Failed to create settings window");

                    let s_proxy = proxy.clone();
                    let s_dpi = Arc::clone(&dpi_clone);
                    let s_cfg = Arc::clone(&config_clone);

                    let init_js = format!(
                        r#"
                        window.__INITIAL_CONFIG__ = {};
                        window.__INITIAL_DPI_STATUS__ = {};
                        "#,
                        cfg_json, status
                    );

                    let view = WebViewBuilder::new()
                        .with_autoplay(false)
                        .with_devtools(true)
                        .with_initialization_script(&init_js)
                        .with_html(SETTINGS_HTML)
                        .with_ipc_handler(move |req| {
                            let body = req.body();
                            if let Ok(msg) = serde_json::from_str::<SettingsIpcMessage>(body) {
                                match msg {
                                    SettingsIpcMessage::GetSettingsState => {
                                        let cfg = s_cfg.lock().unwrap();
                                        let status = s_dpi.get_status_data(&cfg);
                                        let cfg_json = serde_json::to_value(&*cfg).unwrap_or_default();
                                        let _ = s_proxy.send_event(AppEvent::UpdateSettingsUi(cfg_json, status));
                                    }
                                    SettingsIpcMessage::SaveConfig { config: new_cfg } => {
                                        {
                                            let mut cfg = s_cfg.lock().unwrap();
                                            *cfg = new_cfg.clone();
                                            save_config(&cfg);
                                        }
                                        let cfg_val = serde_json::to_value(&new_cfg).unwrap_or_default();
                                        let _ = s_proxy.send_event(AppEvent::ApplyConfigToMain(cfg_val));
                                    }
                                    SettingsIpcMessage::SetDpiStrategy { strategy_id } => {
                                        let mgr = Arc::clone(&s_dpi);
                                        let cfg_lock = Arc::clone(&s_cfg);
                                        let p = s_proxy.clone();
                                        thread::spawn(move || {
                                            let (cfg_val, status) = {
                                                let mut cfg = cfg_lock.lock().unwrap();
                                                let _ = mgr.apply_strategy(&mut cfg, &strategy_id);
                                                save_config(&cfg);
                                                (serde_json::to_value(&*cfg).unwrap_or_default(), mgr.get_status_data(&cfg))
                                            };
                                            let _ = p.send_event(AppEvent::UpdateSettingsUi(cfg_val, status));
                                        });
                                    }
                                    SettingsIpcMessage::RunDpiBenchmark => {
                                        let mgr = Arc::clone(&s_dpi);
                                        let cfg_lock = Arc::clone(&s_cfg);
                                        let p = s_proxy.clone();
                                        thread::spawn(move || {
                                            let results = mgr.run_benchmark();
                                            let winner = results.iter().filter(|r| r.success).min_by_key(|r| r.latency_ms.unwrap_or(u64::MAX));
                                            let (cfg_val, status) = {
                                                let mut cfg = cfg_lock.lock().unwrap();
                                                if let Some(w) = winner {
                                                    let _ = mgr.apply_strategy(&mut cfg, &w.strategy_id);
                                                    save_config(&cfg);
                                                }
                                                (serde_json::to_value(&*cfg).unwrap_or_default(), mgr.get_status_data(&cfg))
                                            };
                                            let _ = p.send_event(AppEvent::BenchmarkDone(results, status.clone()));
                                            let _ = p.send_event(AppEvent::UpdateSettingsUi(cfg_val, status));
                                        });
                                    }
                                    SettingsIpcMessage::CloseSettings => {
                                        let _ = s_proxy.send_event(AppEvent::CloseSettings);
                                    }
                                    SettingsIpcMessage::OpenConfigFolder => {
                                        let dir = get_config_dir();
                                        let _ = std::process::Command::new("open").arg(dir).spawn();
                                    }
                                }
                            }
                        })
                        .build(&win)
                        .expect("Failed to initialize Settings webview");

                    win.set_visible(true);
                    win.set_focus();
                    settings_state = Some((win, view));
                } else if let Some((ref win, ref view)) = settings_state {
                    win.set_visible(true);
                    win.set_focus();
                    let js = format!("if (window.__onConfigLoaded) window.__onConfigLoaded({}, {});", cfg_json, status);
                    let _ = view.evaluate_script(&js);
                }
            }
            Event::UserEvent(AppEvent::CloseSettings) => {
                if let Some((ref win, _)) = settings_state {
                    win.set_visible(false);
                }
            }
            Event::UserEvent(AppEvent::MenuItemClicked(id)) => {
                match id.as_str() {
                    "app.preferences" => {
                        let _ = proxy.send_event(AppEvent::OpenSettings);
                    }
                    "view.reload" => {
                        let _ = main_webview.evaluate_script("window.location.reload();");
                    }
                    "dpi.benchmark" => {
                        let mgr = Arc::clone(&dpi_clone);
                        let cfg_lock = Arc::clone(&config_clone);
                        let p = proxy.clone();
                        thread::spawn(move || {
                            let results = mgr.run_benchmark();
                            let winner = results.iter().filter(|r| r.success).min_by_key(|r| r.latency_ms.unwrap_or(u64::MAX));
                            let (cfg_val, status) = {
                                let mut cfg = cfg_lock.lock().unwrap();
                                if let Some(w) = winner {
                                    let _ = mgr.apply_strategy(&mut cfg, &w.strategy_id);
                                    save_config(&cfg);
                                }
                                (serde_json::to_value(&*cfg).unwrap_or_default(), mgr.get_status_data(&cfg))
                            };
                            let _ = p.send_event(AppEvent::BenchmarkDone(results, status.clone()));
                            let _ = p.send_event(AppEvent::UpdateSettingsUi(cfg_val, status));
                        });
                    }
                    strat if strat.starts_with("dpi.strategy.") => {
                        let strat_id = strat.strip_prefix("dpi.strategy.").unwrap().to_string();
                        let mgr = Arc::clone(&dpi_clone);
                        let cfg_lock = Arc::clone(&config_clone);
                        let p = proxy.clone();
                        thread::spawn(move || {
                            let (cfg_val, status) = {
                                let mut cfg = cfg_lock.lock().unwrap();
                                let _ = mgr.apply_strategy(&mut cfg, &strat_id);
                                save_config(&cfg);
                                (serde_json::to_value(&*cfg).unwrap_or_default(), mgr.get_status_data(&cfg))
                            };
                            let _ = p.send_event(AppEvent::UpdateSettingsUi(cfg_val, status));
                        });
                    }
                    _ => ()
                }
            }
            Event::UserEvent(AppEvent::UpdateSettingsUi(cfg_val, status)) => {
                if let Some((_, ref view)) = settings_state {
                    let js = format!("if (window.__onConfigLoaded) window.__onConfigLoaded({}, {});", cfg_val, status);
                    let _ = view.evaluate_script(&js);
                }
                if let Some(ref m) = _menu_guard {
                    if let Some(active) = status.get("active_strategy").and_then(|v| v.as_str()) {
                        m.update_active_strategy(active);
                    }
                }
            }
            Event::UserEvent(AppEvent::BenchmarkDone(results, status)) => {
                if let Some((_, ref view)) = settings_state {
                    let res_json = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
                    let js = format!("if (window.__onBenchmarkDone) window.__onBenchmarkDone({}, {});", res_json, status);
                    let _ = view.evaluate_script(&js);
                }
            }
            Event::UserEvent(AppEvent::ApplyConfigToMain(cfg_val)) => {
                let js = format!("if (window.__applyConfig) window.__applyConfig({});", cfg_val);
                let _ = main_webview.evaluate_script(&js);
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => {
                let is_settings = settings_state.as_ref().map(|(w, _)| w.id()) == Some(window_id);
                if is_settings {
                    if let Some((ref win, _)) = settings_state {
                        win.set_visible(false);
                    }
                } else if window_id == main_window_id {
                    save_window_state(&app_dir_clone, &main_window);
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => (),
        }
    });
}
