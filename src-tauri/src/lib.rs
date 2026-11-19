//! Noyau applicatif de **PodDesktop** (Tauri v2 + Moteur Audio Rust + SQLite FTS5 + Sync AntennaPod).
//!
//! Ce module assemble tous les sous-systèmes et expose des commandes IPC typées
//! et non bloquantes à l'interface desktop Vanilla TypeScript.

pub mod audio;
pub mod crypto;
pub mod db;
pub mod download;
pub mod i18n_backend;
pub mod models;
pub mod os_integration;
pub mod paths;
pub mod rss;
pub mod sync;

use crate::audio::{AudioEngine, AudioEvent, AudioSource, PlayRequest};
use crate::crypto::LocalCrypto;
use crate::db::episodes::MarkPlayedResult;
use crate::db::feeds::FeedSettingsUpdate;
use crate::db::Database;
use crate::download::DownloadManager;
use crate::i18n_backend::BackendLang;
use crate::models::{
    AppSettings, AudioStateSnapshot, Bookmark, Chapter, DiscoveryPodcast, DownloadTaskInfo,
    Episode, EpisodeQuery, Feed, FeedPreview, ListeningStatsSummary, QueueInsertStrategy,
    SidebarCounts, StorageHealth, SyncReport, Tag,
};
use crate::os_integration::{build_tray_menu, setup_system_tray, OsIntegration, OsMediaAction};
use crate::paths::PortablePaths;
use crate::rss::parser::{parse_transcript, TranscriptCue};
use crate::sync::nextcloud::{NextcloudLoginInit, NextcloudLoginResult};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

/// État global partagé de l'application (injecté dans toutes les commandes Tauri).
pub struct AppState {
    pub paths: Arc<Mutex<PortablePaths>>,
    pub db: Arc<Database>,
    pub crypto: Arc<LocalCrypto>,
    pub http: Client,
    pub audio: Arc<AudioEngine>,
    pub downloads: Arc<DownloadManager>,
    pub os_integration: Mutex<Option<Arc<OsIntegration>>>,
    /// Mémorise la position de départ de la session d'écoute courante (pour `started_sec` AntennaPod).
    pub session_started_sec: Mutex<Option<(i64, i64)>>,
}

impl AppState {
    fn current_paths(&self) -> PortablePaths {
        self.paths.lock().map(|p| p.clone()).unwrap()
    }

    fn current_settings(&self) -> AppSettings {
        let paths = self.current_paths();
        self.db.load_app_settings(&paths).unwrap_or_default()
    }

    fn active_lang(&self) -> BackendLang {
        let s = self.current_settings();
        if s.language == "auto" {
            BackendLang::detect_system()
        } else {
            BackendLang::from_code(&s.language)
        }
    }
}

/// Point d'entrée principal appelé depuis `main.rs` après isolation de la WebView.
pub fn run(portable_paths: PortablePaths) {
    let db = Arc::new(
        Database::open(&portable_paths.db_path)
            .expect("Impossible d'ouvrir la base SQLite poddesktop.db"),
    );
    let crypto = Arc::new(
        LocalCrypto::load_or_create(&portable_paths.dir_data)
            .expect("Impossible d'initialiser le chiffrement local AES-256-GCM"),
    );
    let http = rss::build_http_client();
    let downloads = Arc::new(DownloadManager::new(http.clone(), 3));
    let paths_arc = Arc::new(Mutex::new(portable_paths.clone()));

    // Canal différé vers AppHandle pour les événements émis par le thread audio natif
    let app_handle_slot: Arc<Mutex<Option<AppHandle>>> = Arc::new(Mutex::new(None));
    let app_slot_audio = Arc::clone(&app_handle_slot);
    let db_audio = Arc::clone(&db);
    let dl_audio = Arc::clone(&downloads);
    let last_db_persist = Arc::new(Mutex::new(std::time::Instant::now()));

    let audio = Arc::new(AudioEngine::spawn(move |event| match event {
        AudioEvent::StateChanged(snap) => {
            if let Some(app) = app_slot_audio.lock().ok().and_then(|g| g.clone()) {
                let _ = app.emit("audio-state", &snap);

                // Persistance régulière de la position (toutes les 2 s ou à la pause)
                let should_persist = !snap.is_playing
                    || last_db_persist
                        .lock()
                        .map(|t| t.elapsed() >= std::time::Duration::from_secs(2))
                        .unwrap_or(false);
                if should_persist {
                    if let Some(ep_id) = snap.episode_id {
                        let dur = if snap.duration_ms > 0 {
                            Some(snap.duration_ms)
                        } else {
                            None
                        };
                        let _ = db_audio.update_playback_position(ep_id, snap.position_ms, dur);
                    }
                    if let Ok(mut t) = last_db_persist.lock() {
                        *t = std::time::Instant::now();
                    }
                }

                // Mise à jour SMTC / MPRIS2 / Barre des tâches Windows
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    if let Ok(os_guard) = state.os_integration.lock() {
                        if let Some(ref os) = *os_guard {
                            os.update_playback(&snap, dl_audio.has_active_downloads());
                        }
                    }
                }
            }
        }
        AudioEvent::ListeningDelta {
            episode_id,
            feed_id,
            listened_ms,
            saved_ms,
        } => {
            let _ = db_audio.record_listening_delta(episode_id, feed_id, listened_ms, saved_ms);
        }
        AudioEvent::EpisodeEnded { episode_id } => {
            if let Some(app) = app_slot_audio.lock().ok().and_then(|g| g.clone()) {
                tauri::async_runtime::spawn(async move {
                    handle_episode_ended(&app, episode_id).await;
                });
            }
        }
    }));

    // Initialise les réglages DSP initiaux du moteur audio depuis SQLite
    let init_settings = db.load_app_settings(&portable_paths).unwrap_or_default();
    audio.update_dsp(|cfg| {
        cfg.speed = init_settings.default_speed;
        cfg.skip_silence = init_settings.skip_silence_enabled;
        cfg.skip_silence_threshold_db = init_settings.skip_silence_threshold_db;
        cfg.normalize_loudness = init_settings.normalize_loudness;
        cfg.voice_boost = init_settings.voice_boost;
    });

    let app_state = Arc::new(AppState {
        paths: paths_arc,
        db,
        crypto,
        http,
        audio,
        downloads,
        os_integration: Mutex::new(None),
        session_started_sec: Mutex::new(None),
    });

    let state_for_setup = Arc::clone(&app_state);
    let slot_for_setup = Arc::clone(&app_handle_slot);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(move |app| {
            let handle = app.handle().clone();
            if let Ok(mut slot) = slot_for_setup.lock() {
                *slot = Some(handle.clone());
            }

            let lang = state_for_setup.active_lang();
            let handle_os = handle.clone();
            let on_os_action = move |action: OsMediaAction| {
                let app_clone = handle_os.clone();
                tauri::async_runtime::spawn(async move {
                    dispatch_os_media_action(&app_clone, action).await;
                });
            };

            // 1. System Tray multilingue
            let _ = setup_system_tray(&handle, lang, on_os_action.clone());

            // 2. SMTC (Windows) / MPRIS2 (Linux) + ITaskbarList3 Windows
            let os_ctrl = OsIntegration::new(&handle, lang, on_os_action);
            let s = state_for_setup.current_settings();
            os_ctrl.update_taskbar_buttons(
                lang,
                s.taskbar_show_play_pause,
                s.taskbar_show_next,
                s.taskbar_show_mark_played,
            );
            if let Ok(mut guard) = state_for_setup.os_integration.lock() {
                *guard = Some(os_ctrl);
            }

            // 3. Synchronisation automatique au démarrage (si configurée)
            let handle_sync = handle.clone();
            tauri::async_runtime::spawn(async move {
                trigger_background_sync(&handle_sync).await;
            });

            // 4. Boucle périodique de synchronisation
            let handle_timer = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let mins = handle_timer
                        .try_state::<Arc<AppState>>()
                        .map(|st| st.current_settings().sync_interval_min.max(5))
                        .unwrap_or(15);
                    tokio::time::sleep(std::time::Duration::from_secs(mins as u64 * 60)).await;
                    trigger_background_sync(&handle_timer).await;
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                if let Some(state) = window.app_handle().try_state::<Arc<AppState>>() {
                    let s = state.current_settings();
                    if s.close_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                        return;
                    }
                    // Flush des actions de synchronisation avant fermeture définitive
                    let app_clone = window.app_handle().clone();
                    let state_clone = Arc::clone(&state);
                    record_current_play_sync_action(&state_clone);
                    if s.sync_provider != "none" && !s.sync_server_url.is_empty() {
                        api.prevent_close();
                        tauri::async_runtime::spawn(async move {
                            let _ = tokio::time::timeout(
                                std::time::Duration::from_secs(4),
                                trigger_background_sync(&app_clone),
                            )
                            .await;
                            app_clone.exit(0);
                        });
                    }
                }
            }
            WindowEvent::Resized(_) => {
                // Réduction dans le System Tray si minimisé et option activée
                if let Some(state) = window.app_handle().try_state::<Arc<AppState>>() {
                    if state.current_settings().minimize_to_tray {
                        if let Ok(true) = window.is_minimized() {
                            let _ = window.hide();
                        }
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            // --- Données & Vues ---
            cmd_get_sidebar_counts,
            cmd_list_feeds,
            cmd_list_tags,
            cmd_query_episodes,
            cmd_get_episode,
            cmd_get_chapters,
            cmd_list_bookmarks,
            cmd_add_bookmark,
            cmd_delete_bookmark,
            cmd_load_transcript,
            cmd_get_stats,
            cmd_get_storage_health,
            // --- Abonnements, Tags & Dossiers Locaux ---
            cmd_subscribe_feed,
            cmd_add_local_folder,
            cmd_refresh_feed,
            cmd_refresh_all_feeds,
            cmd_delete_feed,
            cmd_update_feed_settings,
            cmd_upsert_tag,
            cmd_update_tag,
            cmd_delete_tag,
            cmd_set_feed_tags,
            // --- Découverte ---
            cmd_search_directory,
            cmd_preview_feed,
            // --- Actions Épisodes & File ---
            cmd_mark_episodes_played,
            cmd_mark_feed_all_played,
            cmd_mark_tag_all_played,
            cmd_set_episodes_favorite,
            cmd_set_inbox_state,
            cmd_dismiss_feed_from_inbox,
            cmd_dismiss_all_inbox,
            cmd_enqueue_episodes,
            cmd_remove_from_queue,
            cmd_reorder_queue,
            cmd_clear_queue,
            // --- Lecteur Audio Natif ---
            cmd_get_audio_state,
            cmd_play_episode,
            cmd_toggle_play_pause,
            cmd_pause_audio,
            cmd_seek_audio,
            cmd_set_speed,
            cmd_set_volume,
            cmd_set_audio_fx,
            cmd_set_sleep_timer,
            cmd_play_next_in_queue,
            // --- Téléchargements & Stockage ---
            cmd_download_episode,
            cmd_pause_download,
            cmd_cancel_download,
            cmd_list_downloads,
            cmd_delete_local_file,
            cmd_purge_played_downloads,
            cmd_purge_queue_downloads,
            cmd_purge_non_queue_downloads,
            cmd_clear_webview_cache,
            cmd_open_episode_folder,
            // --- Synchronisation Mobile AntennaPod ---
            cmd_start_nextcloud_login,
            cmd_poll_nextcloud_login,
            cmd_save_sync_secret,
            cmd_sync_now,
            cmd_reset_and_sync_now,
            cmd_wipe_and_sync_from_server,
            // --- Paramètres, Import/Export & Maintenance ---
            cmd_get_settings,
            cmd_save_settings,
            cmd_update_storage_dirs,
            cmd_save_podcast_index_secret,
            cmd_import_opml,
            cmd_export_opml,
            cmd_export_favorites,
            cmd_export_settings_json,
            cmd_import_settings_json,
            cmd_backup_database,
            cmd_restore_database,
            cmd_vacuum_database,
        ])
        .run(tauri::generate_context!())
        .expect("Erreur fatale lors de l'exécution de PodDesktop");
}

// =============================================================================
// Logique interne partagée (fin d'épisode, actions OS, synchro silencieuse)
// =============================================================================

async fn handle_episode_ended(app: &AppHandle, episode_id: i64) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let settings = state.current_settings();

    match settings.end_of_episode.as_str() {
        "repeat" => {
            let _ = start_playing_episode_internal(app, &state, episode_id, Some(0)).await;
        }
        "pause" => {
            let res = state.db.mark_episodes_played(
                &[episode_id],
                true,
                true,
                settings.auto_delete_played,
                settings.protect_favorites_from_delete,
            );
            if let Ok(r) = res {
                cleanup_files(&r.files_to_delete);
            }
            let _ = app.emit("data-changed", "episode-ended");
            trigger_background_sync(app).await;
        }
        _ => {
            // Comportement par défaut : marquer lu puis passer automatiquement au suivant de la file
            let next_id = state.db.get_next_in_queue(Some(episode_id)).ok().flatten();
            if let Ok(r) = state.db.mark_episodes_played(
                &[episode_id],
                true,
                true,
                settings.auto_delete_played,
                settings.protect_favorites_from_delete,
            ) {
                cleanup_files(&r.files_to_delete);
            }
            let _ = app.emit("data-changed", "episode-ended");
            if let Some(nid) = next_id {
                let _ = start_playing_episode_internal(app, &state, nid, None).await;
            }
            trigger_background_sync(app).await;
        }
    }
}

async fn dispatch_os_media_action(app: &AppHandle, action: OsMediaAction) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    match action {
        OsMediaAction::TogglePlayPause => {
            let snap = state.audio.snapshot();
            if snap.is_playing {
                pause_with_sync(app, &state).await;
            } else if snap.episode_id.is_some() {
                let rewind = get_smart_rewind_ms(&state);
                state.audio.resume(rewind);
            } else if let Ok(Some(first_id)) = state.db.get_next_in_queue(None) {
                let _ = start_playing_episode_internal(app, &state, first_id, None).await;
            }
        }
        OsMediaAction::Play => {
            let snap = state.audio.snapshot();
            if snap.episode_id.is_some() {
                state.audio.resume(get_smart_rewind_ms(&state));
            } else if let Ok(Some(first_id)) = state.db.get_next_in_queue(None) {
                let _ = start_playing_episode_internal(app, &state, first_id, None).await;
            }
        }
        OsMediaAction::Pause => {
            pause_with_sync(app, &state).await;
        }
        OsMediaAction::NextEpisode => {
            let current = state.audio.snapshot().episode_id;
            if let Ok(Some(next_id)) = state.db.get_next_in_queue(current) {
                let _ = start_playing_episode_internal(app, &state, next_id, None).await;
            }
        }
        OsMediaAction::PreviousOrRewind => {
            let snap = state.audio.snapshot();
            state.audio.seek_ms((snap.position_ms - 15_000).max(0));
        }
        OsMediaAction::MarkPlayed => {
            if let Some(ep_id) = state.audio.snapshot().episode_id {
                let settings = state.current_settings();
                let next_id = state.db.get_next_in_queue(Some(ep_id)).ok().flatten();
                state.audio.stop();
                if let Ok(r) = state.db.mark_episodes_played(
                    &[ep_id],
                    true,
                    true,
                    settings.auto_delete_played,
                    settings.protect_favorites_from_delete,
                ) {
                    cleanup_files(&r.files_to_delete);
                }
                let _ = app.emit("data-changed", "mark-played");
                if let Some(nid) = next_id {
                    let _ = start_playing_episode_internal(app, &state, nid, None).await;
                }
                trigger_background_sync(app).await;
            }
        }
        OsMediaAction::SeekToMs(ms) => state.audio.seek_ms(ms),
        OsMediaAction::SetSpeed(sp) => {
            state.audio.update_dsp(|c| c.speed = sp);
        }
        OsMediaAction::ToggleMiniPlayer => {}
    }
}

async fn pause_with_sync(app: &AppHandle, state: &AppState) {
    state.audio.pause();
    record_current_play_sync_action(state);
    trigger_background_sync(app).await;
}

fn record_current_play_sync_action(state: &AppState) {
    let snap = state.audio.snapshot();
    let Some(ep_id) = snap.episode_id else { return };
    if let Ok(Some(ep)) = state.db.get_episode_by_id(ep_id) {
        let _ = state.db.update_playback_position(
            ep_id,
            snap.position_ms,
            if snap.duration_ms > 0 {
                Some(snap.duration_ms)
            } else {
                None
            },
        );
        if let Ok(Some(feed)) = state.db.get_feed_by_id(ep.feed_id) {
            if !feed.is_local_folder {
                let started_sec = state
                    .session_started_sec
                    .lock()
                    .ok()
                    .and_then(|g| g.filter(|(id, _)| *id == ep_id).map(|(_, s)| s))
                    .unwrap_or(snap.position_ms / 1000);
                let pos_sec = (snap.position_ms / 1000).max(0);
                let total_sec = (snap.duration_ms.max(ep.duration_ms) / 1000).max(pos_sec);
                let _ = state.db.enqueue_sync_action(
                    &ep.audio_url,
                    &feed.feed_url,
                    Some(&ep.guid),
                    "PLAY",
                    Some(started_sec),
                    Some(pos_sec),
                    Some(total_sec),
                );
            }
        }
    }
}

fn get_smart_rewind_ms(state: &AppState) -> i64 {
    let s = state.current_settings();
    if s.smart_rewind_enabled {
        (s.smart_rewind_sec as i64) * 1000
    } else {
        0
    }
}

async fn trigger_background_sync(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let settings = state.current_settings();
    if settings.sync_provider == "none" || settings.sync_server_url.trim().is_empty() {
        return;
    }
    let covers = state.current_paths().dir_covers;
    if let Ok(report) =
        sync::run_full_sync(&state.http, &state.db, &state.crypto, &covers, &settings).await
    {
        if report.pulled_actions > 0 || report.added_feeds > 0 || report.removed_feeds > 0 {
            let _ = app.emit("data-changed", "sync");
        }
        let _ = app.emit("sync-completed", &report);
    }
}

fn cleanup_files(paths: &[String]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}

/// Prépare la source audio (fichier local existant ou mise en cache streaming rapide) puis démarre la lecture native.
async fn start_playing_episode_internal(
    app: &AppHandle,
    state: &AppState,
    episode_id: i64,
    override_pos_ms: Option<i64>,
) -> Result<(), String> {
    // Enregistre la progression de l'épisode précédent avant de basculer
    record_current_play_sync_action(state);

    let ep = state
        .db
        .get_episode_by_id(episode_id)?
        .ok_or_else(|| format!("Épisode introuvable ({episode_id})"))?;
    let feed = state.db.get_feed_by_id(ep.feed_id)?;
    let settings = state.current_settings();

    // Détermine la source audio :
    // 1. Fichier local existant -> lecture directe sans réseau
    // 2. Option `stream_without_downloading` active -> streaming HTTP progressif (0 octet sur disque)
    // 3. Sinon -> téléchargement préalable dans `data/media/`
    let source: AudioSource = if let Some(ref local) = ep.local_file_path {
        let p = PathBuf::from(local);
        if p.exists() {
            AudioSource::LocalFile(p)
        } else if settings.stream_without_downloading {
            let (username, pwd_enc) = state.db.get_feed_credentials(ep.feed_id)?;
            let password = pwd_enc
                .as_deref()
                .filter(|s| !s.is_empty())
                .and_then(|enc| state.crypto.decrypt(enc).ok());
            AudioSource::HttpStream {
                url: ep.audio_url.clone(),
                username,
                password,
                http: state.http.clone(),
                runtime: tokio::runtime::Handle::current(),
            }
        } else {
            AudioSource::LocalFile(download_to_stream_cache(app, state, &ep).await?)
        }
    } else if settings.stream_without_downloading {
        let (username, pwd_enc) = state.db.get_feed_credentials(ep.feed_id)?;
        let password = pwd_enc
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|enc| state.crypto.decrypt(enc).ok());
        AudioSource::HttpStream {
            url: ep.audio_url.clone(),
            username,
            password,
            http: state.http.clone(),
            runtime: tokio::runtime::Handle::current(),
        }
    } else {
        AudioSource::LocalFile(download_to_stream_cache(app, state, &ep).await?)
    };

    // Place automatiquement l'épisode en tête de la file s'il n'y est pas encore
    if ep.queue_position.is_none() {
        let _ = state.db.enqueue_episode(episode_id, QueueInsertStrategy::Top);
    }

    let speed = feed
        .as_ref()
        .and_then(|f| f.playback_speed)
        .unwrap_or(settings.default_speed)
        .clamp(0.50, 3.00);

    let start_pos = override_pos_ms.unwrap_or_else(|| {
        if ep.is_played {
            0
        } else {
            ep.position_ms
        }
    });

    if let Ok(mut guard) = state.session_started_sec.lock() {
        *guard = Some((episode_id, start_pos / 1000));
    }

    state.audio.load_and_play(PlayRequest {
        episode_id,
        feed_id: ep.feed_id,
        title: ep.title.clone(),
        feed_title: ep.feed_title.clone(),
        cover_url: ep.feed_image_url.clone(),
        source,
        start_position_ms: start_pos,
        known_duration_ms: ep.duration_ms,
        speed,
    });

    let _ = app.emit("data-changed", "play-started");
    Ok(())
}

/// Télécharge rapidement un épisode distant vers le dossier média portable pour permettre
/// un décodage natif Symphonia fluide avec seek exact à la milliseconde (y compris MP3 VBR).
async fn download_to_stream_cache(
    app: &AppHandle,
    state: &AppState,
    ep: &Episode,
) -> Result<PathBuf, String> {
    let dir_media = state.current_paths().dir_media;
    let settings = state.current_settings();
    let app_clone = app.clone();
    state
        .downloads
        .start_or_resume(
            Arc::clone(&state.db),
            Arc::clone(&state.crypto),
            dir_media,
            settings,
            ep.id,
            move |info| {
                let _ = app_clone.emit("download-progress", &info);
            },
        )
        .await
}

// =============================================================================
// Commandes Tauri IPC (exposées au frontend TypeScript)
// =============================================================================

#[tauri::command]
fn cmd_get_sidebar_counts(state: State<'_, Arc<AppState>>) -> Result<SidebarCounts, String> {
    state.db.get_sidebar_counts()
}

#[tauri::command]
fn cmd_list_feeds(state: State<'_, Arc<AppState>>) -> Result<Vec<Feed>, String> {
    state.db.list_feeds()
}

#[tauri::command]
fn cmd_list_tags(state: State<'_, Arc<AppState>>) -> Result<Vec<Tag>, String> {
    state.db.list_tags()
}

#[tauri::command]
fn cmd_query_episodes(
    state: State<'_, Arc<AppState>>,
    query: EpisodeQuery,
) -> Result<Vec<Episode>, String> {
    state.db.query_episodes(&query)
}

#[tauri::command]
fn cmd_get_episode(
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
) -> Result<Option<Episode>, String> {
    state.db.get_episode_by_id(episode_id)
}

#[tauri::command]
fn cmd_get_chapters(
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
) -> Result<Vec<Chapter>, String> {
    state.db.list_chapters(episode_id)
}

#[tauri::command]
fn cmd_list_bookmarks(
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
) -> Result<Vec<Bookmark>, String> {
    state.db.list_bookmarks(episode_id)
}

#[tauri::command]
fn cmd_add_bookmark(
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
    position_ms: i64,
    note: String,
) -> Result<Bookmark, String> {
    state.db.add_bookmark(episode_id, position_ms, &note)
}

#[tauri::command]
fn cmd_delete_bookmark(state: State<'_, Arc<AppState>>, bookmark_id: i64) -> Result<(), String> {
    state.db.delete_bookmark(bookmark_id)
}

#[tauri::command]
async fn cmd_load_transcript(
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
) -> Result<Vec<TranscriptCue>, String> {
    let ep = state
        .db
        .get_episode_by_id(episode_id)?
        .ok_or_else(|| "Épisode introuvable".to_string())?;

    if let Some(ref cached) = ep.transcript_data {
        if !cached.trim().is_empty() {
            return Ok(parse_transcript(cached, ep.transcript_type.as_deref()));
        }
    }

    let Some(ref url) = ep.transcript_url.filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };

    let resp = state
        .http
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Erreur téléchargement transcription : {e}"))?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let raw = resp.text().await.unwrap_or_default();
    if !raw.trim().is_empty() {
        // Stocke en DB -> indexation automatique FTS5 pour recherche instantanée
        let _ = state.db.set_transcript_data(episode_id, &raw);
    }
    Ok(parse_transcript(&raw, ep.transcript_type.as_deref()))
}

#[tauri::command]
fn cmd_get_stats(state: State<'_, Arc<AppState>>) -> Result<ListeningStatsSummary, String> {
    state.db.get_listening_stats()
}

#[tauri::command]
fn cmd_get_storage_health(state: State<'_, Arc<AppState>>) -> Result<StorageHealth, String> {
    let p = state.current_paths();
    state
        .db
        .get_storage_health(&p.dir_media, &p.dir_covers, &p.dir_runtime)
}

#[tauri::command]
async fn cmd_subscribe_feed(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    feed_url: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<i64, String> {
    let covers = state.current_paths().dir_covers;
    let id = rss::subscribe_to_feed(
        &state.http,
        &state.db,
        &state.crypto,
        &covers,
        &feed_url,
        username,
        password,
        true,
    )
    .await?;
    let _ = app.emit("data-changed", "subscribe");
    let app_sync = app.clone();
    tauri::async_runtime::spawn(async move {
        trigger_background_sync(&app_sync).await;
    });
    Ok(id)
}

#[tauri::command]
fn cmd_add_local_folder(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    folder_path: String,
    custom_title: Option<String>,
) -> Result<i64, String> {
    let id = rss::local_folder::add_local_folder_feed(
        &state.db,
        Path::new(&folder_path),
        custom_title.as_deref(),
    )?;
    let _ = app.emit("data-changed", "local-folder");
    Ok(id)
}

#[tauri::command]
async fn cmd_refresh_feed(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    feed_id: i64,
) -> Result<usize, String> {
    let covers = state.current_paths().dir_covers;
    let settings = state.current_settings();
    let new_ids = rss::refresh_single_feed(
        &state.http,
        &state.db,
        &state.crypto,
        &covers,
        &settings,
        feed_id,
    )
    .await?;
    let _ = app.emit("data-changed", "refresh");
    Ok(new_ids.len())
}

#[tauri::command]
async fn cmd_refresh_all_feeds(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    let feeds = state.db.list_feeds()?;
    let covers = state.current_paths().dir_covers;
    let settings = state.current_settings();
    let mut total_new = 0usize;

    for feed in feeds.into_iter().filter(|f| f.keep_updated) {
        if let Ok(new_ids) = rss::refresh_single_feed(
            &state.http,
            &state.db,
            &state.crypto,
            &covers,
            &settings,
            feed.id,
        )
        .await
        {
            total_new += new_ids.len();
        }
    }

    let _ = app.emit("data-changed", "refresh-all");
    Ok(total_new)
}

#[tauri::command]
async fn cmd_delete_feed(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    feed_id: i64,
) -> Result<(), String> {
    if let Some(url) = state.db.delete_feed(feed_id)? {
        let _ = state
            .db
            .enqueue_sync_action("", &url, None, "UNSUBSCRIBE", None, None, None);
    }
    let _ = app.emit("data-changed", "delete-feed");
    Ok(())
}

#[tauri::command]
fn cmd_update_feed_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    mut update: FeedSettingsUpdate,
) -> Result<(), String> {
    if let Some(ref pwd) = update.password_enc {
        if !pwd.is_empty() {
            update.password_enc = Some(state.crypto.encrypt(pwd)?);
        }
    }
    state.db.update_feed_settings(&update)?;
    let _ = app.emit("data-changed", "feed-settings");
    Ok(())
}

#[tauri::command]
fn cmd_upsert_tag(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
    color: Option<String>,
) -> Result<i64, String> {
    let id = state.db.upsert_tag(&name, color.as_deref())?;
    let _ = app.emit("data-changed", "tags");
    Ok(id)
}

#[tauri::command]
fn cmd_update_tag(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    tag_id: i64,
    name: String,
    color: String,
) -> Result<(), String> {
    state.db.update_tag(tag_id, &name, &color)?;
    let _ = app.emit("data-changed", "tags");
    Ok(())
}

#[tauri::command]
fn cmd_delete_tag(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    tag_id: i64,
) -> Result<(), String> {
    state.db.delete_tag(tag_id)?;
    let _ = app.emit("data-changed", "tags");
    Ok(())
}

#[tauri::command]
fn cmd_set_feed_tags(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    feed_id: i64,
    tag_ids: Vec<i64>,
) -> Result<(), String> {
    state.db.set_feed_tags(feed_id, &tag_ids)?;
    let _ = app.emit("data-changed", "tags");
    Ok(())
}

#[tauri::command]
async fn cmd_search_directory(
    state: State<'_, Arc<AppState>>,
    query: String,
) -> Result<Vec<DiscoveryPodcast>, String> {
    let s = state.current_settings();
    let pi_secret = state
        .db
        .get_setting_raw("podcast_index_secret_enc")?
        .and_then(|enc| state.crypto.decrypt(&enc).ok());
    rss::discovery::search_directories(
        &state.http,
        &query,
        if s.podcast_index_key.is_empty() {
            None
        } else {
            Some(s.podcast_index_key.as_str())
        },
        pi_secret.as_deref(),
    )
    .await
}

#[tauri::command]
async fn cmd_preview_feed(
    state: State<'_, Arc<AppState>>,
    feed_url: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<FeedPreview, String> {
    rss::discovery::preview_feed(
        &state.http,
        &feed_url,
        username.as_deref(),
        password.as_deref(),
    )
    .await
}

#[tauri::command]
async fn cmd_mark_episodes_played(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_ids: Vec<i64>,
    played: bool,
) -> Result<MarkPlayedResult, String> {
    let s = state.current_settings();
    // Si l'épisode actuellement en lecture est marqué comme lu, passe au suivant
    let playing_id = state.audio.snapshot().episode_id;
    let next_if_playing = if played {
        playing_id
            .filter(|id| episode_ids.contains(id))
            .and_then(|id| state.db.get_next_in_queue(Some(id)).ok().flatten())
    } else {
        None
    };

    if played && playing_id.map(|id| episode_ids.contains(&id)).unwrap_or(false) {
        state.audio.stop();
    }

    let res = state.db.mark_episodes_played(
        &episode_ids,
        played,
        true,
        s.auto_delete_played,
        s.protect_favorites_from_delete,
    )?;
    cleanup_files(&res.files_to_delete);

    if let Some(nid) = next_if_playing {
        let _ = start_playing_episode_internal(&app, &state, nid, None).await;
    }

    let _ = app.emit("data-changed", "mark-played");
    let app_sync = app.clone();
    tauri::async_runtime::spawn(async move {
        trigger_background_sync(&app_sync).await;
    });
    Ok(res)
}

#[tauri::command]
async fn cmd_mark_feed_all_played(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    feed_id: i64,
) -> Result<MarkPlayedResult, String> {
    let ids = state.db.get_unplayed_ids_for_feed(feed_id)?;
    cmd_mark_episodes_played(app, state, ids, true).await
}

#[tauri::command]
async fn cmd_mark_tag_all_played(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    tag_id: i64,
) -> Result<MarkPlayedResult, String> {
    let ids = state.db.get_unplayed_ids_for_tag(tag_id)?;
    cmd_mark_episodes_played(app, state, ids, true).await
}

#[tauri::command]
fn cmd_set_episodes_favorite(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_ids: Vec<i64>,
    favorite: bool,
) -> Result<(), String> {
    state.db.set_episodes_favorite(&episode_ids, favorite)?;
    let _ = app.emit("data-changed", "favorite");
    Ok(())
}

#[tauri::command]
fn cmd_set_inbox_state(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_ids: Vec<i64>,
    inbox_state: i64,
) -> Result<(), String> {
    state.db.set_inbox_state(&episode_ids, inbox_state)?;
    let _ = app.emit("data-changed", "inbox");
    Ok(())
}

#[tauri::command]
fn cmd_dismiss_feed_from_inbox(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    feed_id: i64,
) -> Result<usize, String> {
    let count = state.db.dismiss_feed_from_inbox(feed_id)?;
    let _ = app.emit("data-changed", "inbox");
    Ok(count)
}

#[tauri::command]
fn cmd_dismiss_all_inbox(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    let count = state.db.dismiss_all_inbox()?;
    let _ = app.emit("data-changed", "inbox");
    Ok(count)
}

#[tauri::command]
async fn cmd_enqueue_episodes(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_ids: Vec<i64>,
    strategy: Option<String>,
) -> Result<(), String> {
    let settings = state.current_settings();
    let strat = QueueInsertStrategy::from_str_or_default(
        strategy
            .as_deref()
            .unwrap_or(&settings.queue_insert_manual),
    );
    for &id in &episode_ids {
        state.db.enqueue_episode(id, strat)?;
    }

    // Téléchargement automatique à l'ajout dans la file si configuré
    if settings.auto_download_on_queue {
        for ep_id in episode_ids {
            let app_c = app.clone();
            let st_c = Arc::clone(&state);
            let dir_media = st_c.current_paths().dir_media;
            let s_c = settings.clone();
            tauri::async_runtime::spawn(async move {
                let _ = st_c
                    .downloads
                    .start_or_resume(
                        Arc::clone(&st_c.db),
                        Arc::clone(&st_c.crypto),
                        dir_media,
                        s_c,
                        ep_id,
                        move |info| {
                            let _ = app_c.emit("download-progress", &info);
                        },
                    )
                    .await;
            });
        }
    }

    let _ = app.emit("data-changed", "queue");
    Ok(())
}

#[tauri::command]
fn cmd_remove_from_queue(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_ids: Vec<i64>,
) -> Result<(), String> {
    state.db.remove_from_queue(&episode_ids)?;
    let _ = app.emit("data-changed", "queue");
    Ok(())
}

#[tauri::command]
fn cmd_reorder_queue(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
    new_index: usize,
) -> Result<(), String> {
    if state.current_settings().queue_locked {
        return Err("La file d'attente est verrouillée".to_string());
    }
    state.db.reorder_queue(episode_id, new_index)?;
    let _ = app.emit("data-changed", "queue");
    Ok(())
}

#[tauri::command]
fn cmd_clear_queue(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.db.clear_queue()?;
    let _ = app.emit("data-changed", "queue");
    Ok(())
}

#[tauri::command]
fn cmd_get_audio_state(state: State<'_, Arc<AppState>>) -> AudioStateSnapshot {
    state.audio.snapshot()
}

#[tauri::command]
async fn cmd_play_episode(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
    start_ms: Option<i64>,
) -> Result<(), String> {
    start_playing_episode_internal(&app, &state, episode_id, start_ms).await
}

#[tauri::command]
async fn cmd_toggle_play_pause(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let snap = state.audio.snapshot();
    if snap.is_playing {
        pause_with_sync(&app, &state).await;
    } else if snap.episode_id.is_some() {
        state.audio.resume(get_smart_rewind_ms(&state));
    } else if let Some(first_id) = state.db.get_next_in_queue(None)? {
        start_playing_episode_internal(&app, &state, first_id, None).await?;
    }
    Ok(())
}

#[tauri::command]
async fn cmd_pause_audio(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    pause_with_sync(&app, &state).await;
    Ok(())
}

#[tauri::command]
fn cmd_seek_audio(state: State<'_, Arc<AppState>>, position_ms: i64) -> Result<(), String> {
    state.audio.seek_ms(position_ms);
    Ok(())
}

#[tauri::command]
fn cmd_set_speed(
    state: State<'_, Arc<AppState>>,
    speed: f32,
    remember_for_feed: Option<bool>,
) -> Result<(), String> {
    let clamped = (speed * 20.0).round() / 20.0; // Arrondi au palier exact de 0.05x
    let clamped = clamped.clamp(0.50, 3.00);
    state.audio.update_dsp(|c| c.speed = clamped);

    if remember_for_feed.unwrap_or(true) {
        if let Some(ep_id) = state.audio.snapshot().episode_id {
            if let Ok(Some(ep)) = state.db.get_episode_by_id(ep_id) {
                if let Ok(Some(feed)) = state.db.get_feed_by_id(ep.feed_id) {
                    let _ = state.db.update_feed_settings(&FeedSettingsUpdate {
                        feed_id: feed.id,
                        playback_speed: Some(clamped),
                        auto_download: feed.auto_download,
                        auto_delete: feed.auto_delete,
                        keep_updated: feed.keep_updated,
                        max_kept_episodes: feed.max_kept_episodes,
                        include_keywords: feed.include_keywords,
                        exclude_keywords: feed.exclude_keywords,
                        username: feed.username,
                        password_enc: None,
                    });
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn cmd_set_volume(state: State<'_, Arc<AppState>>, volume: f32) -> Result<(), String> {
    state.audio.update_dsp(|c| c.volume = volume.clamp(0.0, 2.0));
    Ok(())
}

#[tauri::command]
fn cmd_set_audio_fx(
    state: State<'_, Arc<AppState>>,
    skip_silence: bool,
    voice_boost: bool,
    normalize_loudness: bool,
) -> Result<(), String> {
    state.audio.update_dsp(|c| {
        c.skip_silence = skip_silence;
        c.voice_boost = voice_boost;
        c.normalize_loudness = normalize_loudness;
    });
    let _ = state
        .db
        .set_setting_raw("skip_silence_enabled", &skip_silence.to_string());
    let _ = state
        .db
        .set_setting_raw("voice_boost", &voice_boost.to_string());
    let _ = state
        .db
        .set_setting_raw("normalize_loudness", &normalize_loudness.to_string());
    Ok(())
}

#[tauri::command]
fn cmd_set_sleep_timer(
    state: State<'_, Arc<AppState>>,
    minutes: Option<u32>,
    end_of_episode: bool,
) -> Result<(), String> {
    state.audio.set_sleep_timer(minutes, end_of_episode);
    Ok(())
}

#[tauri::command]
async fn cmd_play_next_in_queue(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let curr = state.audio.snapshot().episode_id;
    if let Some(next_id) = state.db.get_next_in_queue(curr)? {
        start_playing_episode_internal(&app, &state, next_id, None).await?;
    }
    Ok(())
}

#[tauri::command]
async fn cmd_download_episode(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
) -> Result<String, String> {
    let dir_media = state.current_paths().dir_media;
    let settings = state.current_settings();
    let app_c = app.clone();
    let path = state
        .downloads
        .start_or_resume(
            Arc::clone(&state.db),
            Arc::clone(&state.crypto),
            dir_media,
            settings,
            episode_id,
            move |info| {
                let _ = app_c.emit("download-progress", &info);
            },
        )
        .await?;
    let _ = app.emit("data-changed", "download-complete");
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn cmd_pause_download(state: State<'_, Arc<AppState>>, episode_id: i64) {
    state.downloads.pause(episode_id);
}

#[tauri::command]
fn cmd_cancel_download(state: State<'_, Arc<AppState>>, episode_id: i64) {
    state.downloads.cancel(episode_id);
}

#[tauri::command]
fn cmd_list_downloads(state: State<'_, Arc<AppState>>) -> Vec<DownloadTaskInfo> {
    state.downloads.list_tasks()
}

#[tauri::command]
fn cmd_delete_local_file(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
) -> Result<(), String> {
    let Some(ep) = state.db.get_episode_by_id(episode_id)? else {
        return Ok(());
    };
    let feed = state.db.get_feed_by_id(ep.feed_id)?;
    if feed.as_ref().map(|f| f.is_local_folder).unwrap_or(false) {
        return Err("Impossible de supprimer un fichier appartenant à un dossier local virtuel".into());
    }
    if let Some(ref p) = ep.local_file_path {
        let _ = std::fs::remove_file(p);
    }
    state.db.set_local_file_path(episode_id, None, None)?;

    if state.current_settings().sync_download_delete {
        if let Some(f) = feed {
            let _ = state.db.enqueue_sync_action(
                &ep.audio_url,
                &f.feed_url,
                Some(&ep.guid),
                "DELETE",
                None,
                None,
                None,
            );
        }
    }
    let _ = app.emit("data-changed", "delete-local-file");
    Ok(())
}

#[tauri::command]
fn cmd_purge_played_downloads(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(usize, u64), String> {
    let protect = state.current_settings().protect_favorites_from_delete;
    let res = state.db.purge_played_downloads(protect)?;
    let _ = app.emit("data-changed", "purge");
    Ok(res)
}

#[tauri::command]
fn cmd_purge_queue_downloads(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(usize, u64), String> {
    let protect = state.current_settings().protect_favorites_from_delete;
    let res = state.db.purge_queue_downloads(protect)?;
    let _ = app.emit("data-changed", "purge");
    Ok(res)
}

#[tauri::command]
fn cmd_purge_non_queue_downloads(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(usize, u64), String> {
    let protect = state.current_settings().protect_favorites_from_delete;
    let res = state.db.purge_non_queue_downloads(protect)?;
    let _ = app.emit("data-changed", "purge");
    Ok(res)
}

#[tauri::command]
fn cmd_clear_webview_cache(state: State<'_, Arc<AppState>>) -> Result<u64, String> {
    state.current_paths().clear_webview_cache()
}

#[tauri::command]
fn cmd_open_episode_folder(
    state: State<'_, Arc<AppState>>,
    episode_id: i64,
) -> Result<(), String> {
    let ep = state
        .db
        .get_episode_by_id(episode_id)?
        .ok_or_else(|| "Épisode introuvable".to_string())?;
    let target_dir = ep
        .local_file_path
        .as_deref()
        .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| state.current_paths().dir_media);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&target_dir)
            .spawn()
            .map_err(|e| format!("Erreur ouverture Explorateur : {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&target_dir)
            .spawn()
            .map_err(|e| format!("Erreur ouverture dossier : {e}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn cmd_start_nextcloud_login(
    state: State<'_, Arc<AppState>>,
    server_url: String,
) -> Result<NextcloudLoginInit, String> {
    sync::nextcloud::start_login_flow_v2(&state.http, &server_url).await
}

#[tauri::command]
async fn cmd_poll_nextcloud_login(
    state: State<'_, Arc<AppState>>,
    poll_endpoint: String,
    poll_token: String,
) -> Result<Option<NextcloudLoginResult>, String> {
    let res = sync::nextcloud::poll_login_flow_v2(&state.http, &poll_endpoint, &poll_token).await?;
    if let Some(ref creds) = res {
        state.db.set_setting_raw("sync_provider", "nextcloud")?;
        state.db.set_setting_raw("sync_server_url", &creds.server)?;
        state.db.set_setting_raw("sync_username", &creds.login_name)?;
        let enc = state.crypto.encrypt(&creds.app_password)?;
        state.db.set_setting_raw("sync_secret_enc", &enc)?;
    }
    Ok(res)
}

#[tauri::command]
fn cmd_save_sync_secret(state: State<'_, Arc<AppState>>, secret_plain: String) -> Result<(), String> {
    let enc = if secret_plain.is_empty() {
        String::new()
    } else {
        state.crypto.encrypt(&secret_plain)?
    };
    state.db.set_setting_raw("sync_secret_enc", &enc)
}

#[tauri::command]
async fn cmd_sync_now(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncReport, String> {
    let settings = state.current_settings();
    let covers = state.current_paths().dir_covers;
    let report =
        sync::run_full_sync(&state.http, &state.db, &state.crypto, &covers, &settings).await?;
    let _ = app.emit("data-changed", "sync");
    Ok(report)
}

#[tauri::command]
async fn cmd_reset_and_sync_now(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncReport, String> {
    // Remet le curseur à 0 pour rapatrier tous les appareils, abonnements et historiques (ex: oPodSync)
    state.db.set_setting_raw("sync_last_timestamp", "0")?;
    let settings = state.current_settings();
    let covers = state.current_paths().dir_covers;
    let report =
        sync::run_full_sync(&state.http, &state.db, &state.crypto, &covers, &settings).await?;
    let _ = app.emit("data-changed", "sync");
    Ok(report)
}

#[tauri::command]
async fn cmd_wipe_and_sync_from_server(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncReport, String> {
    // 1. Stoppe la lecture active pour libérer tout fichier audio ouvert
    state.audio.stop();

    // 2. Supprime tout le catalogue distant local (flux RSS, épisodes, file, sync_queue)
    //    en conservant les réglages, identifiants chiffrés et dossiers locaux virtuels
    let (_deleted_feeds, files_to_delete) = state.db.wipe_remote_catalog_for_resync()?;
    cleanup_files(&files_to_delete);

    // 3. Reconstruit intégralement la base depuis le serveur (gpodder / oPodSync / Nextcloud)
    let settings = state.current_settings();
    let covers = state.current_paths().dir_covers;
    let report =
        sync::run_full_sync(&state.http, &state.db, &state.crypto, &covers, &settings).await?;
    let _ = app.emit("data-changed", "wipe-sync");
    Ok(report)
}

#[tauri::command]
fn cmd_get_settings(state: State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    let paths = state.current_paths();
    state.db.load_app_settings(&paths)
}

#[tauri::command]
fn cmd_save_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> Result<(), String> {
    state.db.save_app_settings(&settings)?;

    // Applique immédiatement les réglages audio DSP
    state.audio.update_dsp(|c| {
        c.skip_silence = settings.skip_silence_enabled;
        c.skip_silence_threshold_db = settings.skip_silence_threshold_db;
        c.normalize_loudness = settings.normalize_loudness;
        c.voice_boost = settings.voice_boost;
    });

    // Met à jour la langue du System Tray et des boutons miniatures barre des tâches Windows
    let lang = if settings.language == "auto" {
        BackendLang::detect_system()
    } else {
        BackendLang::from_code(&settings.language)
    };
    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Ok(menu) = build_tray_menu(&app, lang) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    if let Ok(os_guard) = state.os_integration.lock() {
        if let Some(ref os) = *os_guard {
            os.update_taskbar_buttons(
                lang,
                settings.taskbar_show_play_pause,
                settings.taskbar_show_next,
                settings.taskbar_show_mark_played,
            );
        }
    }

    Ok(())
}

#[tauri::command]
fn cmd_update_storage_dirs(
    state: State<'_, Arc<AppState>>,
    dir_data: Option<String>,
    dir_media: Option<String>,
    dir_covers: Option<String>,
    dir_exports: Option<String>,
    prefer_relative: Option<bool>,
) -> Result<AppSettings, String> {
    {
        let mut p = state
            .paths
            .lock()
            .map_err(|_| "Verrou chemins corrompu".to_string())?;
        p.update_directories(dir_data, dir_media, dir_covers, dir_exports, prefer_relative)?;
    }
    let paths = state.current_paths();
    state.db.load_app_settings(&paths)
}

#[tauri::command]
fn cmd_save_podcast_index_secret(
    state: State<'_, Arc<AppState>>,
    secret_plain: String,
) -> Result<(), String> {
    let enc = if secret_plain.is_empty() {
        String::new()
    } else {
        state.crypto.encrypt(&secret_plain)?
    };
    state.db.set_setting_raw("podcast_index_secret_enc", &enc)
}

#[tauri::command]
async fn cmd_import_opml(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    file_path: String,
) -> Result<usize, String> {
    let xml = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|e| format!("Impossible de lire le fichier OPML : {e}"))?;
    let entries = rss::opml::parse_opml(&xml)?;
    let covers = state.current_paths().dir_covers;
    let mut imported = 0usize;

    for entry in entries {
        if let Ok(feed_id) = rss::subscribe_to_feed(
            &state.http,
            &state.db,
            &state.crypto,
            &covers,
            &entry.feed_url,
            None,
            None,
            true,
        )
        .await
        {
            if !entry.tags.is_empty() {
                let mut tag_ids = Vec::new();
                for tag_name in &entry.tags {
                    if let Ok(tid) = state.db.upsert_tag(tag_name, None) {
                        tag_ids.push(tid);
                    }
                }
                let _ = state.db.set_feed_tags(feed_id, &tag_ids);
            }
            imported += 1;
        }
    }

    let _ = app.emit("data-changed", "import-opml");
    Ok(imported)
}

#[tauri::command]
fn cmd_export_opml(
    state: State<'_, Arc<AppState>>,
    dest_path: Option<String>,
) -> Result<String, String> {
    let feeds = state.db.list_feeds()?;
    let tags = state.db.list_tags()?;
    let xml = rss::opml::generate_opml(&feeds, &tags)?;
    let out = dest_path.map(PathBuf::from).unwrap_or_else(|| {
        state
            .current_paths()
            .dir_exports
            .join(format!(
                "poddesktop-subscriptions-{}.opml",
                chrono::Local::now().format("%Y%m%d")
            ))
    });
    std::fs::write(&out, xml).map_err(|e| format!("Erreur écriture OPML : {e}"))?;
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
fn cmd_export_favorites(
    state: State<'_, Arc<AppState>>,
    format: String,
    dest_path: Option<String>,
) -> Result<String, String> {
    let favs = state.db.query_episodes(&EpisodeQuery {
        view: "favorites".into(),
        ..Default::default()
    })?;
    let content = rss::opml::export_favorites(&favs, &format)?;
    let ext = match format.to_lowercase().as_str() {
        "json" => "json",
        "markdown" | "md" => "md",
        _ => "html",
    };
    let out = dest_path.map(PathBuf::from).unwrap_or_else(|| {
        state.current_paths().dir_exports.join(format!(
            "poddesktop-favorites-{}.{ext}",
            chrono::Local::now().format("%Y%m%d")
        ))
    });
    std::fs::write(&out, content).map_err(|e| format!("Erreur export Favoris : {e}"))?;
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
fn cmd_export_settings_json(
    state: State<'_, Arc<AppState>>,
    dest_path: Option<String>,
) -> Result<String, String> {
    // Exclut toujours les secrets par défaut (conformément au §6.3 et §7.6)
    let json = state.db.export_settings_json(false)?;
    let out = dest_path.map(PathBuf::from).unwrap_or_else(|| {
        state.current_paths().dir_exports.join(format!(
            "poddesktop-settings-{}.json",
            chrono::Local::now().format("%Y%m%d")
        ))
    });
    std::fs::write(&out, json).map_err(|e| format!("Erreur export JSON : {e}"))?;
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
fn cmd_import_settings_json(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    file_path: String,
) -> Result<usize, String> {
    let raw = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Impossible de lire le fichier de préférences : {e}"))?;
    let count = state.db.import_settings_json(&raw)?;
    let _ = app.emit("data-changed", "settings-imported");
    Ok(count)
}

#[tauri::command]
fn cmd_backup_database(
    state: State<'_, Arc<AppState>>,
    dest_path: Option<String>,
) -> Result<String, String> {
    let out = dest_path.map(PathBuf::from).unwrap_or_else(|| {
        state.current_paths().dir_exports.join(format!(
            "poddesktop-backup-{}.db",
            chrono::Local::now().format("%Y%m%d-%H%M")
        ))
    });
    state.db.backup_to(&out)?;
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command]
fn cmd_restore_database(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    src_path: String,
) -> Result<(), String> {
    state.audio.stop();
    state.db.restore_from(Path::new(&src_path))?;
    let _ = app.emit("data-changed", "db-restored");
    Ok(())
}

#[tauri::command]
fn cmd_vacuum_database(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.db.vacuum_and_rebuild_fts()
}
