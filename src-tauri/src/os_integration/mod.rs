//! Intégration native OS (Windows & Linux) :
//! - Contrôles multimédias natifs (`souvlaki`) : Windows SMTC + Linux MPRIS2 D-Bus
//! - Icône System Tray multilingue (`fr`, `en`, `de`, `es`) avec menu contextuel complet
//! - Barre des tâches Windows (`ITaskbarList3` : progression verte/jaune/cyan + boutons miniatures)

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

use crate::i18n_backend::{native_strings, BackendLang};
use crate::models::AudioStateSnapshot;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// État visuel de la barre de progression sur l'icône de la barre des tâches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarProgressMode {
    None,
    Playing,     // Vert (TBPF_NORMAL)
    Paused,      // Jaune (TBPF_PAUSED)
    Downloading, // Cyan / Indéterminé ou progression téléchargement
}

/// Action déclenchée depuis les contrôles OS (SMTC, MPRIS2, Thumbnail Toolbar, System Tray).
#[derive(Debug, Clone)]
pub enum OsMediaAction {
    TogglePlayPause,
    Play,
    Pause,
    NextEpisode,
    PreviousOrRewind,
    MarkPlayed,
    SeekToMs(i64),
    SetSpeed(f32),
    ToggleMiniPlayer,
}

/// Contrôleur central d'intégration OS (SMTC / MPRIS2 + Barre des tâches Windows).
pub struct OsIntegration {
    controls: Mutex<Option<MediaControls>>,
    #[cfg(target_os = "windows")]
    win_taskbar: Mutex<Option<windows::WinTaskbarController>>,
}

impl OsIntegration {
    /// Initialise SMTC (Windows) ou MPRIS2 (Linux) + `ITaskbarList3` sous Windows.
    pub fn new<R: Runtime, F>(app: &AppHandle<R>, lang: BackendLang, on_action: F) -> Arc<Self>
    where
        F: Fn(OsMediaAction) + Send + Sync + 'static,
    {
        let cb = Arc::new(on_action);

        // Récupère le handle natif HWND sous Windows (requis par souvlaki SMTC et ITaskbarList3)
        #[cfg(target_os = "windows")]
        let hwnd_ptr: Option<*mut std::ffi::c_void> = app
            .get_webview_window("main")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as *mut std::ffi::c_void);

        #[cfg(not(target_os = "windows"))]
        let hwnd_ptr: Option<*mut std::ffi::c_void> = None;

        let config = PlatformConfig {
            dbus_name: "poddesktop",
            display_name: "PodDesktop",
            hwnd: hwnd_ptr,
        };

        let controls = match MediaControls::new(config) {
            Ok(mut ctrl) => {
                let cb_clone = Arc::clone(&cb);
                let _ = ctrl.attach(move |event: MediaControlEvent| match event {
                    MediaControlEvent::Toggle => cb_clone(OsMediaAction::TogglePlayPause),
                    MediaControlEvent::Play => cb_clone(OsMediaAction::Play),
                    MediaControlEvent::Pause => cb_clone(OsMediaAction::Pause),
                    MediaControlEvent::Next => cb_clone(OsMediaAction::NextEpisode),
                    MediaControlEvent::Previous => cb_clone(OsMediaAction::PreviousOrRewind),
                    MediaControlEvent::SetPosition(pos) => {
                        cb_clone(OsMediaAction::SeekToMs(pos.0.as_millis() as i64));
                    }
                    _ => {}
                });
                Some(ctrl)
            }
            Err(e) => {
                log::warn!("Initialisation SMTC/MPRIS2 ignorée : {e:?}");
                None
            }
        };

        #[cfg(target_os = "windows")]
        let win_taskbar = hwnd_ptr.and_then(|raw| {
            let cb_win = Arc::clone(&cb);
            windows::WinTaskbarController::attach(raw as isize, lang, move |btn| match btn {
                windows::ThumbButton::PlayPause => cb_win(OsMediaAction::TogglePlayPause),
                windows::ThumbButton::Next => cb_win(OsMediaAction::NextEpisode),
                windows::ThumbButton::MarkPlayed => cb_win(OsMediaAction::MarkPlayed),
            })
            .ok()
        });

        #[cfg(target_os = "linux")]
        {
            linux::log_mpris_ready();
        }

        let _ = (app, lang);
        Arc::new(Self {
            controls: Mutex::new(controls),
            #[cfg(target_os = "windows")]
            win_taskbar: Mutex::new(win_taskbar),
        })
    }

    /// Met à jour les métadonnées SMTC/MPRIS2 et la barre de progression de la barre des tâches.
    pub fn update_playback(&self, snap: &AudioStateSnapshot, downloading: bool) {
        if let Ok(mut guard) = self.controls.lock() {
            if let Some(ref mut ctrl) = *guard {
                if snap.episode_id.is_some() {
                    let duration = if snap.duration_ms > 0 {
                        Some(Duration::from_millis(snap.duration_ms as u64))
                    } else {
                        None
                    };
                    let _ = ctrl.set_metadata(MediaMetadata {
                        title: Some(&snap.title),
                        artist: Some(&snap.feed_title),
                        album: Some("PodDesktop"),
                        cover_url: snap.cover_url.as_deref(),
                        duration,
                    });
                    let progress = Some(souvlaki::MediaPosition(Duration::from_millis(
                        snap.position_ms.max(0) as u64,
                    )));
                    let playback = if snap.is_playing {
                        MediaPlayback::Playing { progress }
                    } else {
                        MediaPlayback::Paused { progress }
                    };
                    let _ = ctrl.set_playback(playback);
                } else {
                    let _ = ctrl.set_playback(MediaPlayback::Stopped);
                }
            }
        }

        #[cfg(target_os = "windows")]
        if let Ok(mut guard) = self.win_taskbar.lock() {
            if let Some(ref mut tb) = *guard {
                let (mode, completed, total) = if downloading {
                    (TaskbarProgressMode::Downloading, 50, 100)
                } else if snap.episode_id.is_some() && snap.duration_ms > 0 {
                    let m = if snap.is_playing {
                        TaskbarProgressMode::Playing
                    } else {
                        TaskbarProgressMode::Paused
                    };
                    (m, snap.position_ms.max(0) as u64, snap.duration_ms as u64)
                } else {
                    (TaskbarProgressMode::None, 0, 100)
                };
                tb.set_progress(mode, completed, total);
            }
        }
    }

    /// Met à jour la visibilité et la langue des boutons de la miniature Windows.
    pub fn update_taskbar_buttons(
        &self,
        lang: BackendLang,
        show_play_pause: bool,
        show_next: bool,
        show_mark_played: bool,
    ) {
        #[cfg(target_os = "windows")]
        if let Ok(mut guard) = self.win_taskbar.lock() {
            if let Some(ref mut tb) = *guard {
                tb.configure_buttons(lang, show_play_pause, show_next, show_mark_played);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (lang, show_play_pause, show_next, show_mark_played);
        }
    }
}

/// Construit le menu contextuel multilingue de l'icône System Tray.
pub fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    lang: BackendLang,
) -> tauri::Result<Menu<R>> {
    let s = native_strings(lang);
    let show = MenuItem::with_id(app, "tray_show", s.show_window, true, None::<&str>)?;
    let play_pause = MenuItem::with_id(app, "tray_play_pause", s.play_pause, true, None::<&str>)?;
    let next = MenuItem::with_id(app, "tray_next", s.next_episode, true, None::<&str>)?;
    let mark_played =
        MenuItem::with_id(app, "tray_mark_played", s.mark_played, true, None::<&str>)?;
    let mini = MenuItem::with_id(app, "tray_mini_player", s.mini_player, true, None::<&str>)?;

    let sp_075 = MenuItem::with_id(app, "tray_speed_075", "0.75x", true, None::<&str>)?;
    let sp_100 = MenuItem::with_id(app, "tray_speed_100", "1.00x", true, None::<&str>)?;
    let sp_125 = MenuItem::with_id(app, "tray_speed_125", "1.25x", true, None::<&str>)?;
    let sp_150 = MenuItem::with_id(app, "tray_speed_150", "1.50x", true, None::<&str>)?;
    let sp_175 = MenuItem::with_id(app, "tray_speed_175", "1.75x", true, None::<&str>)?;
    let sp_200 = MenuItem::with_id(app, "tray_speed_200", "2.00x", true, None::<&str>)?;
    let speed_sub = Submenu::with_items(
        app,
        s.speed_menu,
        true,
        &[&sp_075, &sp_100, &sp_125, &sp_150, &sp_175, &sp_200],
    )?;

    let quit = MenuItem::with_id(app, "tray_quit", s.quit, true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &play_pause,
            &next,
            &mark_played,
            &speed_sub,
            &mini,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

/// Installe l'icône System Tray et connecte ses actions.
pub fn setup_system_tray<R: Runtime, F>(
    app: &AppHandle<R>,
    lang: BackendLang,
    on_action: F,
) -> tauri::Result<()>
where
    F: Fn(OsMediaAction) + Send + Sync + 'static,
{
    let menu = build_tray_menu(app, lang)?;
    let cb = Arc::new(on_action);

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("PodDesktop")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app_handle, event| match event.id.as_ref() {
            "tray_show" => {
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "tray_play_pause" => cb(OsMediaAction::TogglePlayPause),
            "tray_next" => cb(OsMediaAction::NextEpisode),
            "tray_mark_played" => cb(OsMediaAction::MarkPlayed),
            "tray_speed_075" => cb(OsMediaAction::SetSpeed(0.75)),
            "tray_speed_100" => cb(OsMediaAction::SetSpeed(1.0)),
            "tray_speed_125" => cb(OsMediaAction::SetSpeed(1.25)),
            "tray_speed_150" => cb(OsMediaAction::SetSpeed(1.5)),
            "tray_speed_175" => cb(OsMediaAction::SetSpeed(1.75)),
            "tray_speed_200" => cb(OsMediaAction::SetSpeed(2.0)),
            "tray_mini_player" => {
                cb(OsMediaAction::ToggleMiniPlayer);
                let _ = app_handle.emit("os-toggle-mini-player", ());
            }
            "tray_quit" => {
                let _ = app_handle.emit("app-before-quit", ());
                app_handle.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(w) = tray.app_handle().get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
