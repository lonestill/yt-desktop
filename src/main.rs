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
use wry::{BackgroundThrottlingPolicy, ProxyConfig, ProxyEndpoint, WebContext, WebViewBuilder};

const INJECT_CSS: &str = include_str!("styles.css");
const INJECT_JS: &str = include_str!("inject.js");

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum MainIpcMessage {
    #[serde(rename = "title")]
    Title { value: String },
    #[serde(rename = "log")]
    Log { level: String, message: String },
    #[serde(rename = "toggle_hud")]
    ToggleHud,
    #[serde(rename = "open_settings")]
    OpenSettings,
    #[serde(rename = "fetch_video_meta")]
    FetchVideoMeta { video_id: String },
    #[serde(rename = "get_hud_state")]
    GetHudState,
    #[serde(rename = "save_config")]
    SaveConfig { config: AppConfig },
    #[serde(rename = "set_dpi_strategy")]
    SetDpiStrategy { strategy_id: String },
    #[serde(rename = "run_dpi_benchmark")]
    RunDpiBenchmark,
    #[serde(rename = "open_config_folder")]
    OpenConfigFolder,
    #[serde(rename = "complete_onboarding")]
    CompleteOnboarding { strategy_id: String, config: AppConfig },
    #[serde(rename = "show_hello")]
    ShowHello,
}

enum AppEvent {
    ChangeTitle(String),
    Log { level: String, message: String },
    ToggleHud,
    ShowHello,
    MenuItemClicked(String),
    UpdateHudState(serde_json::Value, serde_json::Value, bool),
    BenchmarkDone(Vec<BenchmarkResult>, serde_json::Value),
    VideoMetaLoaded {
        video_id: String,
        dislikes: serde_json::Value,
        segments: serde_json::Value,
    },
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

    // Check CLI flags (--first / -f / --welcome forces welcome screen)
    let force_first_run = std::env::args().any(|a| a == "--first" || a == "-f" || a == "--welcome" || a == "--hello");

    // 1. Load persistent configuration
    let (mut initial_config, is_new_config) = load_config();
    let is_first_run = force_first_run || is_new_config || !initial_config.first_run_completed;
    if force_first_run {
        initial_config.first_run_completed = false;
    }

    // Benchmark all strategies on startup in parallel to pick the fastest working route
    let initial_bench_results = if initial_config.selected_strategy == "auto" || is_first_run {
        if let Some(tpws_bin) = crate::dpi::find_tpws_binary() {
            let results = DpiManager::benchmark_and_select_best(&tpws_bin, &mut initial_config);
            save_config(&initial_config);
            Some(results)
        } else {
            None
        }
    } else {
        None
    };

    let config = Arc::new(Mutex::new(initial_config));

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

    // 4. Initialize native macOS Top App MenuBar
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

    // Hook native macOS menu clicks into Tao event loop
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
        .with_min_inner_size(LogicalSize::new(720.0, 480.0));

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
        window.__YT_FIRST_RUN__ = {};
        {}
        "#,
        serde_json::to_string(INJECT_CSS).unwrap_or_else(|_| "\"\"".to_string()),
        serde_json::to_string(&is_first_run).unwrap_or_else(|_| "false".to_string()),
        INJECT_JS
    );

    let proxy_port = dpi_manager.port;

    // Clones for IPC
    let ipc_dpi = Arc::clone(&dpi_manager);
    let ipc_cfg = Arc::clone(&config);
    let ipc_proxy = proxy.clone();
    let initial_bench_clone = initial_bench_results.clone();

    // Build Main WebView
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
                        let _ = ipc_proxy.send_event(AppEvent::ChangeTitle(value));
                    }
                    MainIpcMessage::Log { level, message } => {
                        let _ = ipc_proxy.send_event(AppEvent::Log { level, message });
                    }
                    MainIpcMessage::ToggleHud | MainIpcMessage::OpenSettings => {
                        let _ = ipc_proxy.send_event(AppEvent::ToggleHud);
                    }
                    MainIpcMessage::GetHudState => {
                        let cfg = ipc_cfg.lock().unwrap();
                        let status = ipc_dpi.get_status_data(&cfg);
                        let cfg_json = serde_json::to_value(&*cfg).unwrap_or_default();
                        let _ = ipc_proxy.send_event(AppEvent::UpdateHudState(cfg_json, status.clone(), is_first_run));
                        if let Some(ref res) = initial_bench_clone {
                            let _ = ipc_proxy.send_event(AppEvent::BenchmarkDone(res.clone(), status));
                        }
                    }
                    MainIpcMessage::SaveConfig { config: new_cfg } => {
                        save_config(&new_cfg);
                        let mut cfg = ipc_cfg.lock().unwrap();
                        *cfg = new_cfg;
                        let status = ipc_dpi.get_status_data(&cfg);
                        let cfg_json = serde_json::to_value(&*cfg).unwrap_or_default();
                        let _ = ipc_proxy.send_event(AppEvent::UpdateHudState(cfg_json, status, false));
                    }
                    MainIpcMessage::SetDpiStrategy { strategy_id } => {
                        let mgr = Arc::clone(&ipc_dpi);
                        let cfg_lock = Arc::clone(&ipc_cfg);
                        let p = ipc_proxy.clone();
                        thread::spawn(move || {
                            let (cfg_val, status) = {
                                let mut cfg = cfg_lock.lock().unwrap();
                                let _ = mgr.apply_strategy(&mut cfg, &strategy_id);
                                save_config(&cfg);
                                (serde_json::to_value(&*cfg).unwrap_or_default(), mgr.get_status_data(&cfg))
                            };
                            let _ = p.send_event(AppEvent::UpdateHudState(cfg_val, status, false));
                        });
                    }
                    MainIpcMessage::RunDpiBenchmark => {
                        let mgr = Arc::clone(&ipc_dpi);
                        let cfg_lock = Arc::clone(&ipc_cfg);
                        let p = ipc_proxy.clone();
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
                            let _ = p.send_event(AppEvent::UpdateHudState(cfg_val, status, false));
                        });
                    }
                    MainIpcMessage::OpenConfigFolder => {
                        let dir = get_config_dir();
                        let _ = std::process::Command::new("open").arg(dir).spawn();
                    }
                    MainIpcMessage::CompleteOnboarding { strategy_id, config: mut new_cfg } => {
                        new_cfg.first_run_completed = true;
                        save_config(&new_cfg);
                        let mgr = Arc::clone(&ipc_dpi);
                        let cfg_lock = Arc::clone(&ipc_cfg);
                        let p = ipc_proxy.clone();
                        thread::spawn(move || {
                            let (cfg_val, status) = {
                                let mut cfg = cfg_lock.lock().unwrap();
                                *cfg = new_cfg;
                                let _ = mgr.apply_strategy(&mut cfg, &strategy_id);
                                save_config(&cfg);
                                (serde_json::to_value(&*cfg).unwrap_or_default(), mgr.get_status_data(&cfg))
                            };
                            let _ = p.send_event(AppEvent::UpdateHudState(cfg_val, status, false));
                        });
                    }
                    MainIpcMessage::ShowHello => {
                        let _ = ipc_proxy.send_event(AppEvent::ShowHello);
                    }
                    MainIpcMessage::FetchVideoMeta { video_id } => {
                        let p = ipc_proxy.clone();
                        thread::spawn(move || {
                            println!("[META-FETCH] 🔍 Requesting metadata for video_id={video_id}...");
                            let dislikes_out = std::process::Command::new("/usr/bin/curl")
                                .args([
                                    "-s",
                                    "-A", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 yt-desktop/0.1.0",
                                    "--connect-timeout", "4",
                                    "--max-time", "6",
                                    &format!("https://returnyoutubedislikeapi.com/votes?videoId={video_id}"),
                                ])
                                .output();

                            let dislikes_val: serde_json::Value = dislikes_out.ok()
                                .and_then(|o| serde_json::from_slice(&o.stdout).ok())
                                .unwrap_or(serde_json::Value::Null);

                            let sponsor_out = std::process::Command::new("/usr/bin/curl")
                                .args([
                                    "-s",
                                    "-A", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 yt-desktop/0.1.0",
                                    "--connect-timeout", "4",
                                    "--max-time", "6",
                                    &format!("https://sponsor.ajay.app/api/skipSegments?videoID={video_id}&categories=%5B%22sponsor%22%2C%22selfpromo%22%2C%22interaction%22%2C%22intro%22%2C%22outro%22%5D"),
                                ])
                                .output();

                            let segments_val: serde_json::Value = sponsor_out.ok()
                                .and_then(|o| serde_json::from_slice(&o.stdout).ok())
                                .unwrap_or_else(|| serde_json::json!([]));

                            let d_count = dislikes_val.get("dislikes").and_then(|v| v.as_i64()).unwrap_or(-1);
                            let s_count = segments_val.as_array().map(|a| a.len()).unwrap_or(0);
                            println!("[META-FETCH] ✅ Results for {video_id}: dislikes={d_count}, segments={s_count}");

                            let _ = p.send_event(AppEvent::VideoMetaLoaded {
                                video_id,
                                dislikes: dislikes_val,
                                segments: segments_val,
                            });
                        });
                    }
                }
            } else {
                eprintln!("[MAIN-IPC] ❌ Failed to parse IPC message: {}", body);
            }
        })
        .with_url("https://www.youtube.com")
        .build(&main_window)
        .expect("Failed to initialize Main WebKit Webview");

    // Apply initial config to main webview
    {
        let cfg = config.lock().unwrap();
        let status = dpi_manager.get_status_data(&cfg);
        let cfg_json = serde_json::to_value(&*cfg).unwrap_or_default();
        let _ = main_webview.evaluate_script(&format!(
            "if (window.__onHudState) window.__onHudState({}, {}, {});",
            cfg_json, status, is_first_run
        ));
        if let Some(ref results) = initial_bench_results {
            let res_json = serde_json::to_string(results).unwrap_or_else(|_| "[]".to_string());
            let _ = main_webview.evaluate_script(&format!(
                "if (window.__onBenchmarkDone) window.__onBenchmarkDone({}, {});",
                res_json, status
            ));
        }
        if is_first_run {
            let _ = main_webview.evaluate_script("if (window.__showHelloScreen) window.__showHelloScreen();");
        }
    }

    let app_dir_clone = app_dir.clone();
    let _dpi_guard = dpi_manager;
    let _menu_guard = app_menu;
    let dpi_clone = Arc::clone(&_dpi_guard);
    let config_clone = Arc::clone(&config);

    event_loop.run(move |event, _, control_flow| {
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
            Event::UserEvent(AppEvent::ToggleHud) => {
                let _ = main_webview.evaluate_script("if (window.__toggleHud) window.__toggleHud();");
            }
            Event::UserEvent(AppEvent::ShowHello) => {
                let _ = main_webview.evaluate_script("if (window.__showHelloScreen) window.__showHelloScreen();");
            }
            Event::UserEvent(AppEvent::MenuItemClicked(id)) => {
                match id.as_str() {
                    "app.preferences" => {
                        let _ = proxy.send_event(AppEvent::ToggleHud);
                    }
                    "app.welcome" => {
                        let _ = proxy.send_event(AppEvent::ShowHello);
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
                            let _ = p.send_event(AppEvent::UpdateHudState(cfg_val, status, false));
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
                            let _ = p.send_event(AppEvent::UpdateHudState(cfg_val, status, false));
                        });
                    }
                    _ => ()
                }
            }
            Event::UserEvent(AppEvent::UpdateHudState(cfg_val, status, is_first)) => {
                let js = format!("if (window.__onHudState) window.__onHudState({}, {}, {});", cfg_val, status, is_first);
                let _ = main_webview.evaluate_script(&js);
                if let Some(ref m) = _menu_guard {
                    if let Some(active) = status.get("active_strategy").and_then(|v| v.as_str()) {
                        m.update_active_strategy(active);
                    }
                }
            }
            Event::UserEvent(AppEvent::BenchmarkDone(results, status)) => {
                let res_json = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
                let js = format!("if (window.__onBenchmarkDone) window.__onBenchmarkDone({}, {});", res_json, status);
                let _ = main_webview.evaluate_script(&js);
            }
            Event::UserEvent(AppEvent::VideoMetaLoaded { video_id, dislikes, segments }) => {
                let js = format!(
                    "if (window.__onVideoMeta) window.__onVideoMeta('{}', {}, {});",
                    video_id, dislikes, segments
                );
                println!("[MAIN-DELIVERY] 📨 Delivering metadata to webview for {video_id}");
                if let Err(e) = main_webview.evaluate_script(&js) {
                    eprintln!("[MAIN-DELIVERY] ❌ Failed to evaluate __onVideoMeta script: {e}");
                }
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if window_id == main_window_id {
                    save_window_state(&app_dir_clone, &main_window);
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => (),
        }
    });
}
