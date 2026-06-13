//! AI Usage Bar — Tauri application entry point.
//!
//! Wires providers → aggregator → tray, runs the background poll loop, and
//! handles tray/menu interaction. The single tray icon shows the *active*
//! provider (user-selected or auto-picked by highest utilization).

mod commands;
mod notify;
pub mod providers;
mod settings;
mod state;
mod tray;
pub mod usage;
mod windows;

use crate::providers::claude::ClaudeProvider;
use crate::providers::codex::CodexProvider;
use crate::providers::Provider;
use crate::settings::{ActiveProvider, DisplayStyle, Settings};
use crate::state::AppState;
use crate::tray::menu::ids;
use crate::usage::selector;
use crate::usage::types::{DisplayMode, UsageSnapshot};
use std::sync::Arc;
use tauri::image::Image;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;

const TRAY_ID: &str = "main-tray";

/// Set when the user picks "Quit" so the `ExitRequested` guard below lets the
/// process actually exit. Without this, the guard that keeps the app alive when
/// a window closes would also veto a deliberate quit. See `on_menu_event`.
static QUITTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("build http client");

    // Build providers.
    let provider_list: Vec<Arc<dyn Provider>> = vec![
        Arc::new(ClaudeProvider::new(http.clone())),
        Arc::new(CodexProvider::new(http.clone())),
    ];

    // Load settings. On first run all providers are enabled by default; the
    // background poll loop classifies each as connected / sign-in-needed once
    // it has fetched. We deliberately do NOT probe credentials synchronously
    // here: reading the macOS Keychain can block on a permission prompt and
    // must never sit on the app's startup path.
    let settings = settings::load();
    if is_first_run() {
        let _ = settings::save(&settings);
    }

    let aggregator = usage::aggregator::Aggregator::new(provider_list);
    let app_state = AppState::new(settings, aggregator, http);

    tauri::Builder::default()
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir { file_name: None },
                ))
                .build(),
        )
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::get_usage,
            commands::refresh_now,
            commands::open_terminal,
        ])
        .setup(|app| {
            // Build the initial menu/tray from settings loaded directly off
            // disk. We avoid locking the async settings mutex here: `setup`
            // runs on the main thread and blocking on the Tokio runtime during
            // startup can deadlock.
            let initial_settings = settings::load();

            // macOS agent app: hide the Dock icon even during `tauri dev`
            // (the Info.plist `LSUIElement` only applies to the bundled .app).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let menu = tray::menu::build_menu(app.handle(), &initial_settings, "Loading…")?;

            let _tray = TrayIconBuilder::with_id(TRAY_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(on_menu_event)
                .on_tray_icon_event(on_tray_icon_event)
                .build(app)?;

            // Give the floating panel a genuine macOS vibrancy backing
            // (NSVisualEffectView) rounded to match the CSS card, so it reads as
            // a Tahoe "liquid glass" surface instead of a flat opaque rectangle.
            // The rounded effect view is what removes the corner "slop": the
            // glass layer itself clips to the radius, so nothing pokes past it.
            #[cfg(target_os = "macos")]
            if let Some(panel) = app.get_webview_window("panel") {
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
                let _ = apply_vibrancy(
                    &panel,
                    NSVisualEffectMaterial::Popover,
                    Some(NSVisualEffectState::Active),
                    Some(14.0),
                );
            }

            // Keep the settings/panel windows hidden until invoked, and hide
            // (rather than close) them so the app keeps running.
            for label in ["settings", "panel"] {
                if let Some(win) = app.get_webview_window(label) {
                    let win_clone = win.clone();
                    win.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let _ = win_clone.hide();
                        }
                    });
                }
            }

            // Spawn the background poll loop.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                poll_loop(handle).await;
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building app")
        .run(|_app, event| {
            // Keep the app alive when a window (e.g. Settings) closes — but let
            // a deliberate Quit through, signalled by the QUITTING flag.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if !QUITTING.load(std::sync::atomic::Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
        });
}

/// Spawn the user's preferred terminal application. Best-effort; failures are
/// logged but otherwise ignored (there is no UI surface to report them to from
/// the tray menu). Shared by the tray menu handler and the `open_terminal`
/// IPC command.
pub(crate) fn launch_terminal(terminal: crate::settings::TerminalApp) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .args(["-a", terminal.macos_app()])
        .spawn();

    #[cfg(target_os = "windows")]
    let result = {
        let _ = terminal; // app choice is macOS-only for now
        std::process::Command::new("cmd")
            .args(["/C", "start", "", "wt.exe"])
            .spawn()
            .or_else(|_| {
                std::process::Command::new("cmd")
                    .args(["/C", "start", "", "cmd.exe"])
                    .spawn()
            })
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let result = {
        let _ = terminal;
        std::process::Command::new("x-terminal-emulator").spawn()
    };

    if let Err(e) = result {
        log::warn!("failed to open terminal: {e}");
    }
}

fn is_first_run() -> bool {
    // Heuristic: settings file does not yet exist.
    directories::ProjectDirs::from("dev", "calebterry", "ai-usage-bar")
        .map(|d| !d.config_dir().join("settings.json").exists())
        .unwrap_or(false)
}

/// Background loop: poll on the configured interval and refresh the tray when
/// the active provider's display changes.
async fn poll_loop<R: Runtime>(app: AppHandle<R>) {
    loop {
        let (settings, changed) = {
            let state = app.state::<AppState>();
            let settings = state.settings.lock().await.clone();
            let mut agg = state.aggregator.lock().await;
            let changed = agg.poll_enabled(&settings).await;
            (settings, changed)
        };

        // Edge-triggered quota notifications from the freshly cached snapshots.
        {
            let state = app.state::<AppState>();
            let snapshots = state.aggregator.lock().await.all_cached().clone();
            let pending = state.notify.lock().await.evaluate(&settings, &snapshots);
            for n in pending {
                use tauri_plugin_notification::NotificationExt;
                let _ = app.notification().builder().title(n.title).body(n.body).show();
            }
        }

        // Always refresh on the first pass; afterwards only when the active
        // provider changed.
        let active = {
            let state = app.state::<AppState>();
            let agg = state.aggregator.lock().await;
            selector::resolve_active(&settings, agg.all_cached())
        };
        if active.map(|a| changed.contains(&a)).unwrap_or(false) || !changed.is_empty() {
            update_tray(&app, &settings).await;
        }

        // Notify any open UI windows that data refreshed.
        let _ = app.emit("usage-updated", ());

        tokio::time::sleep(settings.poll_interval()).await;
    }
}

/// Re-render the tray icon, title, tooltip, and menu for the active provider.
async fn update_tray<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    let state = app.state::<AppState>();
    let snapshot = {
        let agg = state.aggregator.lock().await;
        let active = selector::resolve_active(settings, agg.all_cached());
        active.and_then(|id| agg.cached(id).cloned())
    };

    let Some(snapshot) = snapshot else { return };
    let appearance = tray::theme::detect();

    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    // macOS: provider glyph (template) + the 5h percentage as the menu-bar title.
    // Other platforms have no separate title, so keep the self-contained bitmap
    // that bakes the numbers/bars into the icon.
    #[cfg(target_os = "macos")]
    {
        let rendered = tray::render_provider_glyph(snapshot.provider, appearance);
        let image = Image::new_owned(rendered.rgba, rendered.width, rendered.height);
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_icon_as_template(rendered.is_template);
        let _ = tray.set_title(Some(tray_title(&snapshot, settings)));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let rendered = tray::render_icon(&snapshot, settings, appearance);
        let image = Image::new_owned(rendered.rgba, rendered.width, rendered.height);
        let _ = tray.set_icon(Some(image));
    }

    let _ = tray.set_tooltip(Some(tooltip(&snapshot, settings)));

    // Rebuild the menu so checkmarks + status header stay current.
    if let Ok(menu) = tray::menu::build_menu(app, settings, &status_line(&snapshot, settings)) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Short title shown next to the macOS menu bar glyph: only the 5-hour session
/// percentage, e.g. "45%". Respects the user's used/remaining preference via
/// `display_pct`. Weekly is intentionally omitted here — it lives in the panel.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn tray_title(snapshot: &UsageSnapshot, settings: &Settings) -> String {
    match &snapshot.mode {
        DisplayMode::Session { primary, .. } => {
            format!("{}%", settings.display_pct(primary.utilization).round() as i32)
        }
        DisplayMode::SpendCap { utilization, .. } => {
            format!("{}%", settings.display_pct(*utilization).round() as i32)
        }
        DisplayMode::Unauthenticated => "—".to_string(),
        DisplayMode::ApiKeyOnly => "key".to_string(),
    }
}

fn tooltip(snapshot: &UsageSnapshot, settings: &Settings) -> String {
    status_line(snapshot, settings)
}

/// Full one-line status, used in tooltip and menu header.
fn status_line(snapshot: &UsageSnapshot, settings: &Settings) -> String {
    let provider = snapshot.provider.label();
    let plan = if snapshot.plan_label.is_empty() {
        String::new()
    } else {
        format!(" · {}", snapshot.plan_label)
    };
    let stale = if snapshot.stale { " (stale)" } else { "" };
    let label = if settings.show_remaining {
        "left"
    } else {
        "used"
    };

    let body = match &snapshot.mode {
        DisplayMode::Session { primary, secondary } => {
            let p = settings.display_pct(primary.utilization).round() as i32;
            match secondary {
                Some(s) => format!(
                    "5h {p}% {label} · Wk {}% {label}",
                    settings.display_pct(s.utilization).round() as i32
                ),
                None => format!("5h {p}% {label}"),
            }
        }
        DisplayMode::SpendCap { utilization, .. } => {
            format!(
                "Spend cap {}% {label}",
                settings.display_pct(*utilization).round() as i32
            )
        }
        DisplayMode::Unauthenticated => "Sign in required".to_string(),
        DisplayMode::ApiKeyOnly => "API key mode — no subscription limits".to_string(),
    };

    format!("{provider}{plan} · {body}{stale}")
}

/// Handle context-menu clicks.
fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref().to_string();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match id.as_str() {
            ids::QUIT => {
                // Mark the exit as intentional so the ExitRequested guard
                // doesn't veto it, then tear the process down.
                QUITTING.store(true, std::sync::atomic::Ordering::SeqCst);
                app.exit(0);
            }
            ids::REFRESH => {
                let settings = app.state::<AppState>().settings.lock().await.clone();
                {
                    let state = app.state::<AppState>();
                    let mut agg = state.aggregator.lock().await;
                    agg.poll_enabled(&settings).await;
                }
                update_tray(&app, &settings).await;
                let _ = app.emit("usage-updated", ());
            }
            ids::OPEN_TERMINAL => {
                let terminal = app.state::<AppState>().settings.lock().await.default_terminal;
                launch_terminal(terminal);
            }
            ids::SETTINGS => {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            ids::PROVIDER_AUTO => set_active(&app, ActiveProvider::Auto).await,
            ids::PROVIDER_CLAUDE => set_active(&app, ActiveProvider::Claude).await,
            ids::PROVIDER_CODEX => set_active(&app, ActiveProvider::Codex).await,
            ids::STYLE_NUMBERS => set_style(&app, DisplayStyle::Numbers).await,
            ids::STYLE_BARS => set_style(&app, DisplayStyle::Bars).await,
            ids::TOGGLE_REMAINING => {
                mutate_settings(&app, |s| s.show_remaining = !s.show_remaining).await;
            }
            ids::LAUNCH_AT_LOGIN => {
                let enable = {
                    let state = app.state::<AppState>();
                    let mut s = state.settings.lock().await;
                    s.launch_at_login = !s.launch_at_login;
                    let _ = settings::save(&s);
                    s.launch_at_login
                };
                let mgr = app.autolaunch();
                let _ = if enable { mgr.enable() } else { mgr.disable() };
                let settings = app.state::<AppState>().settings.lock().await.clone();
                update_tray(&app, &settings).await;
            }
            _ => {}
        }
    });
}

async fn set_active<R: Runtime>(app: &AppHandle<R>, value: ActiveProvider) {
    mutate_settings(app, |s| s.active_provider = value).await;
}

async fn set_style<R: Runtime>(app: &AppHandle<R>, value: DisplayStyle) {
    mutate_settings(app, |s| s.display_style = value).await;
}

/// Apply a settings change, persist it, refresh the tray, and notify the UI.
async fn mutate_settings<R, F>(app: &AppHandle<R>, f: F)
where
    R: Runtime,
    F: FnOnce(&mut Settings),
{
    let settings = {
        let state = app.state::<AppState>();
        let mut s = state.settings.lock().await;
        f(&mut s);
        let _ = settings::save(&s);
        s.clone()
    };
    update_tray(app, &settings).await;
    let _ = app.emit("usage-updated", ());
}

/// Handle left-click on the tray icon (toggle the detail panel).
fn on_tray_icon_event<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, event: TrayIconEvent) {
    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        let app = tray.app_handle().clone();
        let _ = windows::float_panel::toggle(&app);
    }
}
