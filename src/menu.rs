use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
};

#[allow(dead_code)]
pub struct AppMenu {
    pub menu: Menu,
    pub prefs_id: String,
    pub benchmark_id: String,
    pub auto_id: String,
    pub rostelecom_id: String,
    pub mts_id: String,
    pub beeline_id: String,
    pub tlsrec_id: String,
    pub direct_id: String,
}

impl AppMenu {
    pub fn init() -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();

        // 1. App Menu
        let app_menu = Submenu::new("YouTube", true);
        let prefs_item = MenuItem::with_id(
            "app.preferences",
            "Настройки...",
            true,
            Some(Accelerator::new(Modifiers::META, Code::Comma)),
        );
        app_menu.append(&PredefinedMenuItem::about(Some("О YouTube"), None))?;
        app_menu.append(&PredefinedMenuItem::separator())?;
        app_menu.append(&prefs_item)?;
        app_menu.append(&PredefinedMenuItem::separator())?;
        app_menu.append(&PredefinedMenuItem::hide(Some("Скрыть YouTube")))?;
        app_menu.append(&PredefinedMenuItem::hide_others(Some("Скрыть остальные")))?;
        app_menu.append(&PredefinedMenuItem::show_all(Some("Показать все")))?;
        app_menu.append(&PredefinedMenuItem::separator())?;
        app_menu.append(&PredefinedMenuItem::quit(Some("Завершить YouTube")))?;

        // 2. Anti-DPI Menu
        let dpi_menu = Submenu::new("Anti-DPI", true);
        let bench_item = MenuItem::with_id(
            "dpi.benchmark",
            "🚀 Авто-подбор (Benchmark)",
            true,
            Some(Accelerator::new(Modifiers::META | Modifiers::SHIFT, Code::KeyB)),
        );
        dpi_menu.append(&bench_item)?;
        dpi_menu.append(&PredefinedMenuItem::separator())?;

        let auto_item = CheckMenuItem::with_id("dpi.strategy.auto", "⚡ Авто-подбор", true, true, None);
        let rostelecom_item = CheckMenuItem::with_id("dpi.strategy.rostelecom", "Ростелеком / Дом.ру", true, false, None);
        let mts_item = CheckMenuItem::with_id("dpi.strategy.mts_megafon", "МТС / Мегафон / Yota", true, false, None);
        let beeline_item = CheckMenuItem::with_id("dpi.strategy.beeline_mgts", "Билайн / МГТС / Теле2", true, false, None);
        let tlsrec_item = CheckMenuItem::with_id("dpi.strategy.tlsrec_midsld", "TLS Record (SLD Split)", true, false, None);
        let direct_item = CheckMenuItem::with_id("dpi.strategy.direct", "Прямое подключение (Direct)", true, false, None);

        dpi_menu.append(&auto_item)?;
        dpi_menu.append(&rostelecom_item)?;
        dpi_menu.append(&mts_item)?;
        dpi_menu.append(&beeline_item)?;
        dpi_menu.append(&tlsrec_item)?;
        dpi_menu.append(&direct_item)?;

        // 3. Edit Menu (Copy/Paste/Undo in webview inputs)
        let edit_menu = Submenu::new("Правка", true);
        edit_menu.append(&PredefinedMenuItem::undo(Some("Отменить")))?;
        edit_menu.append(&PredefinedMenuItem::redo(Some("Повторить")))?;
        edit_menu.append(&PredefinedMenuItem::separator())?;
        edit_menu.append(&PredefinedMenuItem::cut(Some("Вырезать")))?;
        edit_menu.append(&PredefinedMenuItem::copy(Some("Скопировать")))?;
        edit_menu.append(&PredefinedMenuItem::paste(Some("Вставить")))?;
        edit_menu.append(&PredefinedMenuItem::select_all(Some("Выбрать всё")))?;

        // 4. View Menu
        let view_menu = Submenu::new("Вид", true);
        view_menu.append(&MenuItem::with_id(
            "view.reload",
            "Перезагрузить страницу",
            true,
            Some(Accelerator::new(Modifiers::META, Code::KeyR)),
        ))?;
        view_menu.append(&PredefinedMenuItem::separator())?;
        view_menu.append(&PredefinedMenuItem::fullscreen(Some("Полноэкранный режим")))?;

        // 5. Window Menu
        let window_menu = Submenu::new("Окно", true);
        window_menu.append(&PredefinedMenuItem::minimize(Some("Убрать в Dock")))?;
        window_menu.append(&PredefinedMenuItem::close_window(Some("Закрыть окно")))?;

        menu.append(&app_menu)?;
        menu.append(&dpi_menu)?;
        menu.append(&edit_menu)?;
        menu.append(&view_menu)?;
        menu.append(&window_menu)?;

        #[cfg(target_os = "macos")]
        menu.init_for_nsapp();

        muda::MenuEvent::set_event_handler(Some(|event: muda::MenuEvent| {
            println!("Menu event: {}", event.id().as_ref());
        }));

        Ok(Self {
            menu,
            prefs_id: "app.preferences".to_string(),
            benchmark_id: "dpi.benchmark".to_string(),
            auto_id: "dpi.strategy.auto".to_string(),
            rostelecom_id: "dpi.strategy.rostelecom".to_string(),
            mts_id: "dpi.strategy.mts_megafon".to_string(),
            beeline_id: "dpi.strategy.beeline_mgts".to_string(),
            tlsrec_id: "dpi.strategy.tlsrec_midsld".to_string(),
            direct_id: "dpi.strategy.direct".to_string(),
        })
    }
}
