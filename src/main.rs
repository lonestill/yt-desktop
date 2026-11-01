use std::fs;
use std::path::PathBuf;
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Theme, Window, WindowBuilder},
};
use wry::{BackgroundThrottlingPolicy, WebContext, WebViewBuilder};
use serde::{Deserialize, Serialize};

const INJECT_CSS: &str = include_str!("styles.css");
const INJECT_JS: &str = include_str!("inject.js");

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum IpcMessage {
    #[serde(rename = "title")]
    Title { value: String },
}

enum AppEvent {
    ChangeTitle(String),
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

    let _webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_autoplay(true)
        .with_back_forward_navigation_gestures(true)
        .with_accept_first_mouse(true)
        .with_devtools(true)
        .with_background_throttling(BackgroundThrottlingPolicy::Disabled)
        .with_user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15",
        )
        .with_initialization_script(&init_script)
        .with_ipc_handler(move |req| {
            let body = req.body();
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(body) {
                match msg {
                    IpcMessage::Title { value } => {
                        let _ = proxy.send_event(AppEvent::ChangeTitle(value));
                    }
                }
            }
        })
        .with_url("https://www.youtube.com")
        .build(&window)
        .expect("Failed to initialize WebKit Webview");

    let app_dir_clone = app_dir.clone();

    event_loop.run(move |event, _, control_flow| {
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
