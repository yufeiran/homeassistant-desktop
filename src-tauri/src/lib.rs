use mdns_sd::{ServiceDaemon, ServiceEvent};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::{
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, LogicalPosition, LogicalSize, Manager, State, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const SHORTCUT: &str = "CommandOrControl+Alt+X";
const FULLSCREEN_SHORTCUT: &str = "CommandOrControl+Alt+Enter";
const GITHUB_URL: &str = "https://github.com/iprodanovbg/homeassistant-desktop";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    auto_update: bool,
    automatic_switching: bool,
    detached_mode: bool,
    disable_hover: bool,
    stay_on_top: bool,
    full_screen: bool,
    shortcut_enabled: bool,
    all_instances: Vec<String>,
    window_size: Option<[u32; 2]>,
    current_instance: Option<usize>,
    window_position: Option<[i32; 2]>,
    window_size_detached: Option<[u32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // Kept for compatibility with the Electron config. Updates are deliberately
            // manual until a fork configures a signed Tauri update feed.
            auto_update: true,
            automatic_switching: true,
            detached_mode: false,
            disable_hover: false,
            stay_on_top: false,
            full_screen: false,
            shortcut_enabled: false,
            all_instances: Vec::new(),
            window_size: None,
            current_instance: None,
            window_position: None,
            window_size_detached: None,
        }
    }
}

impl Settings {
    fn current_url(&self) -> Option<&str> {
        self.current_instance
            .and_then(|index| self.all_instances.get(index))
            .map(String::as_str)
    }

    fn normalize(&mut self) {
        let current = self.current_url().map(str::to_owned);
        self.all_instances.retain(|item| valid_ha_url(item).is_ok());
        let mut seen = BTreeSet::new();
        self.all_instances.retain(|item| seen.insert(item.clone()));
        self.current_instance =
            current.and_then(|address| self.all_instances.iter().position(|item| item == &address));
    }
}

struct AppState {
    config_path: PathBuf,
    settings: Mutex<Settings>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientState {
    instances: Vec<String>,
    current_url: Option<String>,
    platform: &'static str,
    webview_name: &'static str,
    webview_version: Option<String>,
    webview_runtime_policy: &'static str,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("homeassistant-desktop")
        .join("config.json")
}

fn load_settings(path: &Path) -> Settings {
    let mut settings = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
        .unwrap_or_default();
    settings.normalize();
    settings
}

fn save_settings(state: &AppState) -> Result<(), String> {
    if let Some(parent) = state.config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text =
        serde_json::to_string_pretty(&*state.settings.lock()).map_err(|error| error.to_string())?;
    fs::write(&state.config_path, text).map_err(|error| error.to_string())
}

fn valid_ha_url(input: &str) -> Result<Url, String> {
    let mut url = Url::parse(input.trim()).map_err(|_| "Please enter a valid URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Home Assistant URLs must use http:// or https://".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Credentials must not be embedded in the URL".into());
    }
    url.set_fragment(None);
    Ok(url)
}

fn local_page(name: &str) -> Url {
    #[cfg(target_os = "windows")]
    let address = format!("http://tauri.localhost/{name}");
    #[cfg(not(target_os = "windows"))]
    let address = format!("tauri://localhost/{name}");
    Url::parse(&address).expect("valid local application URL")
}

fn navigate(window: &WebviewWindow, address: &str) -> Result<(), String> {
    window
        .navigate(valid_ha_url(address)?)
        .map_err(|error| error.to_string())
}

fn navigate_current(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let target = state.settings.lock().current_url().map(str::to_owned);
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable".to_string())?;
    if let Some(target) = target {
        navigate(&window, &target)
    } else {
        window
            .navigate(local_page("index.html"))
            .map_err(|error| error.to_string())
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            show_window(app);
        }
    }
}

fn apply_window_settings(window: &WebviewWindow, settings: &Settings) {
    let detached = settings.detached_mode;
    let _ = window.set_decorations(detached && !cfg!(target_os = "macos"));
    let _ = window.set_skip_taskbar(!detached);
    let _ = window.set_always_on_top(settings.stay_on_top || settings.full_screen);
    let _ = window.set_fullscreen(settings.full_screen);

    let size = if detached {
        settings.window_size_detached
    } else {
        settings.window_size
    };
    if let Some([width, height]) = size {
        let _ = window.set_size(LogicalSize::new(width, height));
    }
    if detached {
        if let Some([x, y]) = settings.window_position {
            let _ = window.set_position(LogicalPosition::new(x, y));
        }
    }
}

fn set_shortcuts(app: &AppHandle, enabled: bool) {
    let manager = app.global_shortcut();
    let _ = manager.unregister(SHORTCUT);
    let _ = manager.unregister(FULLSCREEN_SHORTCUT);
    if enabled {
        let _ = manager.register(SHORTCUT);
    }
    let _ = manager.register(FULLSCREEN_SHORTCUT);
}

fn checked_item(
    app: &AppHandle,
    id: &str,
    label: &str,
    enabled: bool,
    checked: bool,
    accelerator: Option<&str>,
) -> tauri::Result<CheckMenuItem<tauri::Wry>> {
    CheckMenuItem::with_id(app, id, label, enabled, checked, accelerator)
}

fn build_tray_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().clone();
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    let mut menu = MenuBuilder::new(app).text("toggle", "Show/Hide Window");

    if settings.current_url().is_some() {
        menu = menu.text("open_browser", "Open in Browser").separator();
    }
    for (index, instance) in settings.all_instances.iter().enumerate() {
        let item = checked_item(
            app,
            &format!("instance:{index}"),
            instance,
            true,
            settings.current_instance == Some(index),
            None,
        )?;
        menu = menu.item(&item);
    }

    let auto_switch = checked_item(
        app,
        "automatic_switching",
        "Automatic Switching",
        settings.all_instances.len() > 1,
        settings.automatic_switching,
        None,
    )?;
    let hover = checked_item(
        app,
        "hover",
        "Hover to Show",
        !cfg!(target_os = "linux") && !settings.detached_mode,
        !settings.disable_hover,
        None,
    )?;
    let stay_on_top = checked_item(
        app,
        "stay_on_top",
        "Stay on Top",
        true,
        settings.stay_on_top,
        None,
    )?;
    let start_at_login = checked_item(app, "autostart", "Start at Login", true, autostart, None)?;
    let shortcut = checked_item(
        app,
        "shortcut",
        "Enable Shortcut",
        true,
        settings.shortcut_enabled,
        Some(SHORTCUT),
    )?;
    let detached = checked_item(
        app,
        "detached",
        "Use Detached Window",
        true,
        settings.detached_mode,
        None,
    )?;
    let fullscreen = checked_item(
        app,
        "fullscreen",
        "Use Fullscreen",
        true,
        settings.full_screen,
        Some(FULLSCREEN_SHORTCUT),
    )?;
    let version = MenuItem::with_id(
        app,
        "version",
        format!("v{} · system WebView", app.package_info().version),
        false,
        None::<&str>,
    )?;
    let update = MenuItem::with_id(
        app,
        "updates",
        "Updates: manual until a signed feed is configured",
        false,
        None::<&str>,
    )?;

    menu.separator()
        .text("add_instance", "Add Another Instance…")
        .item(&auto_switch)
        .separator()
        .item(&hover)
        .item(&stay_on_top)
        .item(&start_at_login)
        .item(&shortcut)
        .separator()
        .item(&detached)
        .item(&fullscreen)
        .separator()
        .item(&version)
        .item(&update)
        .text("github", "Open on GitHub")
        .separator()
        .text("reset_window", "Reset Window Layout")
        .text("restart", "Restart Application")
        .separator()
        .text("quit", "Quit")
        .build()
}

fn refresh_tray_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build_tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn toggle_setting(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    {
        let mut settings = state.settings.lock();
        match id {
            "automatic_switching" => settings.automatic_switching = !settings.automatic_switching,
            "hover" => settings.disable_hover = !settings.disable_hover,
            "stay_on_top" => settings.stay_on_top = !settings.stay_on_top,
            "shortcut" => settings.shortcut_enabled = !settings.shortcut_enabled,
            "detached" => settings.detached_mode = !settings.detached_mode,
            "fullscreen" => settings.full_screen = !settings.full_screen,
            _ => return,
        }
    }
    let _ = save_settings(&state);
    let settings = state.settings.lock().clone();
    if let Some(window) = app.get_webview_window("main") {
        apply_window_settings(&window, &settings);
    }
    set_shortcuts(app, settings.shortcut_enabled);
    refresh_tray_menu(app);
}

fn handle_menu(app: &AppHandle, id: &str) {
    if let Some(index) = id
        .strip_prefix("instance:")
        .and_then(|text| text.parse::<usize>().ok())
    {
        let state = app.state::<AppState>();
        if index < state.settings.lock().all_instances.len() {
            state.settings.lock().current_instance = Some(index);
            let _ = save_settings(&state);
            let _ = navigate_current(app);
            show_window(app);
            refresh_tray_menu(app);
        }
        return;
    }

    match id {
        "toggle" => toggle_window(app),
        "open_browser" => {
            if let Some(url) = app.state::<AppState>().settings.lock().current_url() {
                let _ = open::that_detached(url);
            }
        }
        "add_instance" => {
            let _ = app
                .get_webview_window("main")
                .map(|window| window.navigate(local_page("index.html")));
            show_window(app);
        }
        "automatic_switching"
        | "hover"
        | "stay_on_top"
        | "shortcut"
        | "detached"
        | "fullscreen" => toggle_setting(app, id),
        "autostart" => {
            let manager = app.autolaunch();
            if manager.is_enabled().unwrap_or(false) {
                let _ = manager.disable();
            } else {
                let _ = manager.enable();
            }
            refresh_tray_menu(app);
        }
        "github" => {
            let _ = open::that_detached(GITHUB_URL);
        }
        "reset_window" => {
            let state = app.state::<AppState>();
            {
                let mut settings = state.settings.lock();
                settings.window_size = None;
                settings.window_size_detached = None;
                settings.window_position = None;
                settings.detached_mode = false;
                settings.full_screen = false;
            }
            let _ = save_settings(&state);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_fullscreen(false);
                let _ = window.set_size(LogicalSize::new(420_u32, 460_u32));
                let _ = window.center();
                apply_window_settings(&window, &state.settings.lock());
            }
            refresh_tray_menu(app);
        }
        "restart" => app.restart(),
        "quit" => app.exit(0),
        _ => {}
    }
}

fn create_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Home Assistant")
        .inner_size(420.0, 460.0)
        .min_inner_size(420.0, 460.0)
        .visible(false)
        .skip_taskbar(true)
        .resizable(true)
        .on_new_window(|url, _features| {
            let _ = open::that_detached(url.as_str());
            tauri::webview::NewWindowResponse::Deny
        })
        .build()
}

fn setup_window_events(window: &WebviewWindow) {
    let app = window.app_handle().clone();
    window.on_window_event(move |event| {
        let state = app.state::<AppState>();
        match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            WindowEvent::Focused(false) => {
                let settings = state.settings.lock();
                if !settings.detached_mode && !settings.stay_on_top {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
            WindowEvent::Resized(size) => {
                let mut settings = state.settings.lock();
                if settings.full_screen {
                    return;
                }
                let value = Some([size.width, size.height]);
                if settings.detached_mode {
                    settings.window_size_detached = value;
                } else {
                    settings.window_size = value;
                }
                drop(settings);
                let _ = save_settings(&state);
            }
            WindowEvent::Moved(position) => {
                let mut settings = state.settings.lock();
                if settings.detached_mode {
                    settings.window_position = Some([position.x, position.y]);
                    drop(settings);
                    let _ = save_settings(&state);
                }
            }
            _ => {}
        }
    });
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_tray_menu(app)?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Home Assistant")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu(app, event.id().0.as_str()))
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } => {
                    if let Some(window) = app.get_webview_window("main") {
                        let settings = app.state::<AppState>().settings.lock().clone();
                        if !settings.detached_mode {
                            if let Ok(size) = window.outer_size() {
                                let scale = window.scale_factor().unwrap_or(1.0);
                                let tray_position = rect.position.to_physical::<i32>(scale);
                                let tray_size = rect.size.to_physical::<u32>(scale);
                                let x = tray_position.x
                                    + (tray_size.width as i32 - size.width as i32) / 2;
                                let y = tray_position.y - size.height as i32;
                                let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                            }
                        }
                    }
                    toggle_window(app);
                }
                TrayIconEvent::Enter { .. } => {
                    let settings = app.state::<AppState>().settings.lock().clone();
                    if !cfg!(target_os = "linux")
                        && !settings.detached_mode
                        && !settings.stay_on_top
                        && !settings.disable_hover
                    {
                        show_window(app);
                    }
                }
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

async fn instance_available(address: &str) -> bool {
    let Ok(url) = valid_ha_url(address) else {
        return false;
    };
    let Ok(endpoint) = url.join("/auth/providers") else {
        return false;
    };
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
    else {
        return false;
    };
    client
        .get(endpoint)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success())
}

fn start_availability_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut consecutive_failures = 0_u8;
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let settings = app.state::<AppState>().settings.lock().clone();
            let Some(current) = settings.current_url().map(str::to_owned) else {
                consecutive_failures = 0;
                continue;
            };

            if instance_available(&current).await {
                consecutive_failures = 0;
                if let Some(window) = app.get_webview_window("main") {
                    if window
                        .url()
                        .is_ok_and(|url| url.path().ends_with("error.html"))
                    {
                        let _ = navigate(&window, &current);
                    }
                }
                continue;
            }

            consecutive_failures = consecutive_failures.saturating_add(1);
            if consecutive_failures < 2 {
                continue;
            }

            let mut replacement = None;
            if settings.automatic_switching {
                for (index, address) in settings.all_instances.iter().enumerate() {
                    if address != &current && instance_available(address).await {
                        replacement = Some((index, address.clone()));
                        break;
                    }
                }
            }

            if let Some((index, address)) = replacement {
                let state = app.state::<AppState>();
                state.settings.lock().current_instance = Some(index);
                let _ = save_settings(&state);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = navigate(&window, &address);
                }
                refresh_tray_menu(&app);
                consecutive_failures = 0;
            } else if let Some(window) = app.get_webview_window("main") {
                if !window
                    .url()
                    .is_ok_and(|url| url.path().ends_with("error.html"))
                {
                    let _ = window.navigate(local_page("error.html"));
                }
            }
        }
    });
}

#[tauri::command]
fn get_client_state(state: State<'_, AppState>) -> ClientState {
    let settings = state.settings.lock();
    let (webview_name, webview_version, policy) = webview_info();
    ClientState {
        instances: settings.all_instances.clone(),
        current_url: settings.current_url().map(str::to_owned),
        platform: std::env::consts::OS,
        webview_name,
        webview_version,
        webview_runtime_policy: policy,
    }
}

#[tauri::command]
async fn discover_instances() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let daemon = ServiceDaemon::new().map_err(|error| error.to_string())?;
        let receiver = daemon
            .browse("_home-assistant._tcp.local.")
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let mut found = BTreeSet::new();
        while started.elapsed() < Duration::from_millis(1800) {
            match receiver.recv_timeout(Duration::from_millis(180)) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    for key in ["internal_url", "external_url"] {
                        if let Some(property) = info.get_property(key) {
                            let value = property.val_str();
                            if valid_ha_url(value).is_ok() {
                                found.insert(value.to_string());
                            }
                        }
                    }
                }
                Ok(_) | Err(_) => {}
            }
        }
        let _ = daemon.stop_browse("_home-assistant._tcp.local.");
        let _ = daemon.shutdown();
        Ok(found.into_iter().collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn verify_instance(address: String) -> Result<(), String> {
    let url = valid_ha_url(&address)?;
    let endpoint = url
        .join("/auth/providers")
        .map_err(|error| error.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(endpoint)
        .send()
        .await
        .map_err(|error| format!("Home Assistant did not respond: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Home Assistant returned HTTP {}",
            response.status()
        ));
    }
    let body = response.text().await.map_err(|error| error.to_string())?;
    if !body.contains("homeassistant") {
        return Err("The address did not identify itself as Home Assistant".into());
    }
    Ok(())
}

#[tauri::command]
async fn add_instance(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    address: String,
) -> Result<(), String> {
    verify_instance(address.clone()).await?;
    let url = valid_ha_url(&address)?.to_string();
    {
        let mut settings = state.settings.lock();
        let index = settings
            .all_instances
            .iter()
            .position(|item| item == &url)
            .unwrap_or_else(|| {
                settings.all_instances.push(url.clone());
                settings.all_instances.len() - 1
            });
        settings.current_instance = Some(index);
        if settings.all_instances.len() == 1 {
            settings.disable_hover = false;
        }
    }
    save_settings(&state)?;
    refresh_tray_menu(&app);
    window
        .navigate(Url::parse(&url).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reconnect(app: AppHandle) -> Result<(), String> {
    navigate_current(&app)
}

#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
}

#[cfg(target_os = "windows")]
fn webview_info() -> (&'static str, Option<String>, &'static str) {
    use winreg::{enums::*, RegKey};
    let roots = [
        RegKey::predef(HKEY_LOCAL_MACHINE),
        RegKey::predef(HKEY_CURRENT_USER),
    ];
    let paths = [
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients",
        r"SOFTWARE\Microsoft\EdgeUpdate\Clients",
    ];
    for root in roots {
        for path in paths {
            if let Ok(clients) = root.open_subkey_with_flags(path, KEY_READ) {
                for child in clients.enum_keys().flatten() {
                    if let Ok(key) = clients.open_subkey_with_flags(child, KEY_READ) {
                        let name: String = key.get_value("name").unwrap_or_default();
                        if name.contains("WebView2") {
                            let version = key.get_value::<String, _>("pv").ok();
                            return ("Microsoft Edge WebView2", version, "Evergreen Runtime; the Windows installer downloads it when missing");
                        }
                    }
                }
            }
        }
    }
    (
        "Microsoft Edge WebView2",
        None,
        "Missing; the Windows installer will offer the Microsoft bootstrapper",
    )
}

#[cfg(target_os = "macos")]
fn webview_info() -> (&'static str, Option<String>, &'static str) {
    (
        "Apple WKWebView",
        None,
        "Built into macOS and updated with the operating system",
    )
}

#[cfg(target_os = "linux")]
fn webview_info() -> (&'static str, Option<String>, &'static str) {
    (
        "WebKitGTK 4.1",
        None,
        "Provided and updated by the Linux distribution package manager",
    )
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let text = shortcut.to_string();
                    if text.eq_ignore_ascii_case(SHORTCUT) {
                        toggle_window(app);
                    } else if text.eq_ignore_ascii_case(FULLSCREEN_SHORTCUT) {
                        toggle_setting(app, "fullscreen");
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_client_state,
            discover_instances,
            verify_instance,
            add_instance,
            reconnect,
            restart_app
        ])
        .setup(|app| {
            let path = config_path();
            let settings = load_settings(&path);
            app.manage(AppState {
                config_path: path,
                settings: Mutex::new(settings.clone()),
            });
            let window = create_window(app.handle())?;
            apply_window_settings(&window, &settings);
            setup_window_events(&window);
            setup_tray(app.handle())?;
            set_shortcuts(app.handle(), settings.shortcut_enabled);
            start_availability_monitor(app.handle().clone());
            if settings.current_url().is_some() {
                navigate_current(app.handle()).map_err(anyhow::Error::msg)?;
            } else {
                window.show()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Home Assistant Desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_legacy_electron_configuration_shape() {
        let json = r#"{
          "autoUpdate": true,
          "automaticSwitching": false,
          "detachedMode": true,
          "disableHover": true,
          "stayOnTop": false,
          "fullScreen": false,
          "shortcutEnabled": true,
          "allInstances": ["http://homeassistant.local:8123/"],
          "currentInstance": 0,
          "windowSize": [420, 460],
          "windowPosition": [30, 40],
          "windowSizeDetached": [900, 700]
        }"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(
            settings.current_url(),
            Some("http://homeassistant.local:8123/")
        );
        assert!(settings.detached_mode);
        assert!(settings.shortcut_enabled);
    }

    #[test]
    fn keeps_selected_instance_when_removing_duplicates() {
        let mut settings = Settings {
            all_instances: vec![
                "http://one.local:8123/".into(),
                "http://two.local:8123/".into(),
                "http://one.local:8123/".into(),
            ],
            current_instance: Some(1),
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(settings.current_url(), Some("http://two.local:8123/"));
        assert_eq!(settings.all_instances.len(), 2);
    }

    #[test]
    fn rejects_embedded_credentials_and_non_http_schemes() {
        assert!(valid_ha_url("file:///etc/passwd").is_err());
        assert!(valid_ha_url("https://user:secret@example.test").is_err());
        assert!(valid_ha_url("http://homeassistant.local:8123").is_ok());
    }
}
