//! AI Usage Bar — Tauri application entry point.
//!
//! Wires providers → aggregator → tray, runs the background poll loop, and
//! handles tray/menu interaction. The single tray icon shows the *active*
//! provider (user-selected or auto-picked by highest utilization).

mod commands;
pub mod cost;
mod notify;
pub mod providers;
pub mod settings;
mod state;
pub mod status;
mod tray;
pub mod usage;
mod windows;

use crate::providers::claude::ClaudeProvider;
use crate::providers::codex::CodexProvider;
use crate::providers::credits::CreditsProvider;
use crate::providers::Provider;
use crate::settings::{ActiveProvider, DisplayStyle, Settings};
use crate::state::AppState;
use crate::tray::menu::ids;
use crate::usage::selector;
use crate::usage::types::{ProviderId, ProviderKind, UsageSnapshot};
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

    // Build providers: the two OAuth subscription providers plus a generic
    // API-key credits provider for every remaining `ProviderId`. Driving the
    // API-key set off `ProviderId::ALL` keeps adding a provider to a single
    // enum edit rather than a list mutation here.
    let mut provider_list: Vec<Arc<dyn Provider>> = vec![
        Arc::new(ClaudeProvider::new(http.clone())),
        Arc::new(CodexProvider::new(http.clone())),
    ];
    for id in ProviderId::ALL {
        if id.kind() == ProviderKind::ApiKeyCredits {
            provider_list.push(Arc::new(CreditsProvider::new(http.clone(), id)));
        }
    }

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
            commands::reset_settings,
            commands::get_usage,
            commands::refresh_now,
            commands::open_terminal,
            commands::get_cost_summary,
            commands::set_api_key,
            commands::api_key_status,
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

            let menu = tray::menu::build_menu(app.handle(), &initial_settings, "Loading…", None)?;

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
                use window_vibrancy::{
                    apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState,
                };
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
                    // The detail panel is a popover: dismiss it on blur (click
                    // outside) so it behaves like the menu-bar surface it mimics.
                    // The settings window is a normal window, so it stays put.
                    let dismiss_on_blur = label == "panel";
                    win.on_window_event(move |event| match event {
                        tauri::WindowEvent::CloseRequested { api, .. } => {
                            api.prevent_close();
                            let _ = win_clone.hide();
                        }
                        tauri::WindowEvent::Focused(false) if dismiss_on_blur => {
                            let _ = win_clone.hide();
                        }
                        _ => {}
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
                          // Spawn the executables directly rather than via `cmd /C start`. `start`
                          // returns success as soon as the shell launches — even when `wt.exe` is
                          // absent — so an `.or_else` chained on it would never reach the fallback.
                          // Spawning the binary itself yields a real io::Result we can branch on.
        std::process::Command::new("wt.exe").spawn().or_else(|_| {
            std::process::Command::new("cmd.exe")
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
    // Heuristic: settings file does not yet exist. Keys off the same path
    // `settings::load`/`save` use so the two can't drift.
    !settings::settings_path().exists()
}

/// Background loop: poll on the configured interval and refresh the tray when
/// the active provider's display changes.
async fn poll_loop<R: Runtime>(app: AppHandle<R>) {
    loop {
        let settings = app.state::<AppState>().settings.lock().await.clone();
        refresh_and_render(&app, &settings).await;
        tokio::time::sleep(settings.poll_interval()).await;
    }
}

/// The single canonical "fetch fresh data and reflect it everywhere" chain:
/// poll enabled providers → fire edge-triggered quota notifications → refresh
/// service-status incidents → redraw the tray if anything visible changed →
/// emit `usage-updated` to open windows.
///
/// Every place that wants fresh data — the background poll loop, the tray
/// "Refresh" item, and the `refresh_now` / `set_api_key` IPC commands — calls
/// this so the side effects never drift (an earlier hand-rolled copy in the
/// tray handler silently skipped notifications and incident refresh; the API-key
/// command skipped the tray redraw and window emit entirely).
pub(crate) async fn refresh_and_render<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    let changed = {
        let state = app.state::<AppState>();
        let mut agg = state.aggregator.lock().await;
        agg.poll_enabled(settings).await
    };

    // Edge-triggered quota notifications from the freshly cached snapshots.
    {
        let state = app.state::<AppState>();
        let snapshots = state.aggregator.lock().await.all_cached().clone();
        let pending = state.notify.lock().await.evaluate(settings, &snapshots);
        for n in pending {
            use tauri_plugin_notification::NotificationExt;
            let _ = app
                .notification()
                .builder()
                .title(n.title)
                .body(n.body)
                .show();
        }
    }

    // Provider service-status polling (best-effort; never blocks usage).
    // When disabled we clear any stale incidents so the badge disappears.
    // Track whether the incident set changed: the tray's incident header is
    // rebuilt in `update_tray`, so a change here must force a redraw even
    // when no provider's *usage* changed (esp. in pinned mode below).
    let incidents_changed = {
        let state = app.state::<AppState>();
        let incidents = if settings.check_provider_status {
            let http = state.http.clone();
            status::fetch_many(&http, &settings.enabled_providers).await
        } else {
            Vec::new()
        };
        let mut guard = state.incidents.lock().await;
        let changed = *guard != incidents;
        *guard = incidents;
        changed
    };

    // Resolve who's active now so we can both gate the usage redraw and detect
    // the "nothing to show" case below.
    let active = {
        let state = app.state::<AppState>();
        let agg = state.aggregator.lock().await;
        selector::resolve_active(settings, agg.all_cached())
    };

    // Redraw the tray only when a change could affect what it displays.
    // In Auto mode any provider's change can flip the selection or update
    // the shown figure, so any non-empty `changed` set qualifies. With an
    // explicitly pinned provider, only that provider's own change matters.
    // Either way, an incident-set change repaints the header.
    let usage_redraw = match settings.active_provider {
        crate::settings::ActiveProvider::Auto => !changed.is_empty(),
        _ => active.map(|a| changed.contains(&a)).unwrap_or(false),
    };

    // When no provider resolves (every provider disabled, or none cached yet),
    // nothing lands in `changed`, so the redraw gate above stays false and the
    // tray would keep whatever it last rendered — including the initial
    // "Loading…" built at setup. Force one redraw in that case so `update_tray`
    // can paint the `NO_PROVIDERS_STATUS` placeholder. `update_tray` is cheap
    // and converges (it sets the same placeholder each time), so re-running it
    // on subsequent empty polls is harmless.
    let needs_empty_redraw = active.is_none();

    if usage_redraw || incidents_changed || needs_empty_redraw {
        update_tray(app, settings).await;
    }

    // Notify any open UI windows that data refreshed.
    let _ = app.emit("usage-updated", ());
}

/// Re-render the tray icon, title, tooltip, and menu for the active provider.
async fn update_tray<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    let state = app.state::<AppState>();
    let snapshot = {
        let agg = state.aggregator.lock().await;
        let active = selector::resolve_active(settings, agg.all_cached());
        active.and_then(|id| agg.cached(id).cloned())
    };

    let appearance = tray::theme::detect();

    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    // No active snapshot resolves when every provider is disabled (or none has
    // fetched yet). Don't bail out here — that would leave the *previous*
    // provider's icon/title/menu frozen on screen even though the UI says no
    // providers are enabled. Instead reset the tray to a neutral placeholder so
    // it visibly reflects the empty state.
    let Some(snapshot) = snapshot else {
        let _ = tray.set_icon(app.default_window_icon().cloned());
        #[cfg(target_os = "macos")]
        {
            let _ = tray.set_icon_as_template(true);
            let _ = tray.set_title(None::<String>);
        }
        let _ = tray.set_tooltip(Some(NO_PROVIDERS_STATUS.to_string()));
        if let Ok(menu) = tray::menu::build_menu(app, settings, NO_PROVIDERS_STATUS, None) {
            let _ = tray.set_menu(Some(menu));
        }
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
        let _ = tray.set_title(Some(snapshot.mode.tray_title(settings.show_remaining)));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let rendered = tray::render_icon(&snapshot, settings, appearance);
        let image = Image::new_owned(rendered.rgba, rendered.width, rendered.height);
        let _ = tray.set_icon(Some(image));
    }

    let _ = tray.set_tooltip(Some(status_line(&snapshot, settings)));

    // Format the worst active service incident, if any, as a second header row.
    let incident_line = {
        let incidents = state.incidents.lock().await;
        status::worst(&incidents).map(|i| format!("⚠ {}: {}", i.provider.label(), i.description))
    };

    // Rebuild the menu so checkmarks + status header stay current.
    if let Ok(menu) = tray::menu::build_menu(
        app,
        settings,
        &status_line(&snapshot, settings),
        incident_line.as_deref(),
    ) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Header/tooltip shown when no provider is enabled, so the tray doesn't keep
/// displaying a stale provider after the user disables everything.
const NO_PROVIDERS_STATUS: &str = "No providers enabled";

/// Full one-line status (provider · plan · body), used in the tooltip and menu
/// header. The mode-specific body lives on `DisplayMode::status_summary`; this
/// only wraps it with the provider/plan prefix and stale marker.
fn status_line(snapshot: &UsageSnapshot, settings: &Settings) -> String {
    let provider = snapshot.provider.label();
    let plan = if snapshot.plan_label.is_empty() {
        String::new()
    } else {
        format!(" · {}", snapshot.plan_label)
    };
    let stale = if snapshot.stale { " (stale)" } else { "" };
    let body = snapshot.mode.status_summary(settings.show_remaining);
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
                // Route through the canonical chain so a manual refresh fires
                // quota notifications and refreshes incidents too — not just the
                // tray redraw the old hand-rolled copy did. Bust the cost cache
                // so the next cost read rescans, matching `refresh_now`.
                let settings = app.state::<AppState>().settings.lock().await.clone();
                crate::cost::invalidate();
                refresh_and_render(&app, &settings).await;
            }
            ids::OPEN_TERMINAL => {
                let terminal = app
                    .state::<AppState>()
                    .settings
                    .lock()
                    .await
                    .default_terminal;
                launch_terminal(terminal);
            }
            ids::SETTINGS => show_settings_window(&app),
            ids::PROVIDER_AUTO => set_active(&app, ActiveProvider::Auto).await,
            ids::PROVIDER_CLAUDE => set_active(&app, ActiveProvider::Claude).await,
            ids::PROVIDER_CODEX => set_active(&app, ActiveProvider::Codex).await,
            ids::STYLE_NUMBERS => set_style(&app, DisplayStyle::Numbers).await,
            ids::STYLE_BARS => set_style(&app, DisplayStyle::Bars).await,
            ids::TOGGLE_REMAINING => {
                mutate_settings(&app, |s| s.show_remaining = !s.show_remaining).await;
            }
            ids::LAUNCH_AT_LOGIN => {
                mutate_settings(&app, |s| s.launch_at_login = !s.launch_at_login).await;
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

/// Mutate settings in place via a closure, then run the canonical apply chain.
/// Used by the tray menu's checkbox/submenu handlers.
async fn mutate_settings<R, F>(app: &AppHandle<R>, f: F)
where
    R: Runtime,
    F: FnOnce(&mut Settings),
{
    let next = {
        let state = app.state::<AppState>();
        let mut s = state.settings.lock().await;
        f(&mut s);
        s.clone()
    };
    apply_settings(app, next).await;
}

/// The single canonical settings-mutation path. Every settings change — from the
/// tray menu *and* from the Settings window's `set_settings` IPC command — flows
/// through here so the side effects never diverge: sync autostart when
/// `launch_at_login` changed, persist to disk, update in-memory state, re-render
/// the tray, and notify open windows.
pub(crate) async fn apply_settings<R: Runtime>(app: &AppHandle<R>, next: Settings) {
    let state = app.state::<AppState>();

    // Detect a launch-at-login transition against the previous value so we only
    // touch the autostart plugin when it actually changed.
    let prev_launch = {
        let mut guard = state.settings.lock().await;
        let prev = guard.launch_at_login;
        *guard = next.clone();
        prev
    };
    if next.launch_at_login != prev_launch {
        let mgr = app.autolaunch();
        let _ = if next.launch_at_login {
            mgr.enable()
        } else {
            mgr.disable()
        };
    }

    let _ = settings::save(&next);
    update_tray(app, &next).await;
    let _ = app.emit("usage-updated", ());
}

/// Bring the (normally hidden) settings window to the foreground.
fn show_settings_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Handle left-click on the tray icon. Toggles the floating detail panel,
/// except on Windows where `windows_float_panel == false` opens Settings
/// instead (Windows users who prefer the classic tray-app behavior).
fn on_tray_icon_event<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, event: TrayIconEvent) {
    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        let app = tray.app_handle().clone();
        // The setting only exists on Windows; elsewhere left-click always
        // toggles the panel. Spawn the settings lookup off the read so we
        // don't block the tray callback on the settings lock.
        #[cfg(target_os = "windows")]
        {
            tauri::async_runtime::spawn(async move {
                let float_panel = {
                    let state = app.state::<AppState>();
                    let settings = state.settings.lock().await;
                    settings.windows_float_panel
                };
                if float_panel {
                    let _ = windows::float_panel::toggle(&app);
                } else {
                    show_settings_window(&app);
                }
            });
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = windows::float_panel::toggle(&app);
        }
    }
}
