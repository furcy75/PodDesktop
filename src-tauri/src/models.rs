//! Modèles de données partagés entre le backend Rust (SQLite, Audio, Sync, RSS)
//! et le frontend TypeScript via sérialisation JSON (Serde).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Abonnement RSS ou dossier local virtuel (`feeds`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    pub id: i64,
    pub title: String,
    pub feed_url: String,
    pub image_url: Option<String>,
    pub local_cover_path: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub is_local_folder: bool,
    pub local_folder_path: Option<String>,
    pub last_update: Option<i64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub playback_speed: Option<f32>,
    pub auto_download: bool,
    pub auto_delete: bool,
    pub keep_updated: bool,
    pub max_kept_episodes: Option<i64>,
    pub include_keywords: Option<String>,
    pub exclude_keywords: Option<String>,
    pub funding_url: Option<String>,
    pub username: Option<String>,
    /// Indique à l'UI si un mot de passe chiffré existe (le secret n'est jamais renvoyé en clair).
    pub has_password: bool,
    /// Compteur calculé d'épisodes non lus.
    #[serde(default)]
    pub unread_count: i64,
    /// Identifiants des tags associés.
    #[serde(default)]
    pub tag_ids: Vec<i64>,
}

/// Tag / Dossier personnalisé pour organiser les abonnements (`tags`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub unread_count: i64,
    #[serde(default)]
    pub feed_count: i64,
}

/// Épisode de podcast ou fichier audio local (`episodes`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: String,
    pub feed_image_url: Option<String>,
    pub feed_funding_url: Option<String>,
    pub guid: String,
    pub title: String,
    pub pub_date: i64,
    pub description: Option<String>,
    pub audio_url: String,
    pub mime_type: Option<String>,
    pub local_file_path: Option<String>,
    pub file_size: i64,
    pub duration_ms: i64,
    pub position_ms: i64,
    pub is_played: bool,
    pub is_favorite: bool,
    /// 0 = normal, 1 = dans l'Inbox, 2 = ignoré
    pub inbox_state: i64,
    pub played_at: Option<i64>,
    pub transcript_url: Option<String>,
    pub transcript_type: Option<String>,
    pub transcript_data: Option<String>,
    /// Position dans la file d'attente si présent (`None` sinon).
    pub queue_position: Option<i64>,
    /// Indique si l'épisode possède au moins un chapitre.
    pub has_chapters: bool,
}

/// Chapitre ID3 / Vorbis / Podcasting 2.0 (`chapters`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: i64,
    pub episode_id: i64,
    pub start_time_ms: i64,
    pub title: String,
    pub url: Option<String>,
    pub image_url: Option<String>,
}

/// Signet personnel horodaté (`bookmarks`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: i64,
    pub episode_id: i64,
    pub position_ms: i64,
    pub note: String,
    pub created_at: i64,
}

/// Entrée de la file d'attente de synchronisation vers AntennaPod (`sync_queue`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncQueueItem {
    pub id: i64,
    pub episode_url: String,
    pub feed_url: String,
    pub guid: Option<String>,
    /// "PLAY", "NEW", "DOWNLOAD", "DELETE", "SUBSCRIBE", "UNSUBSCRIBE"
    pub action: String,
    /// Horodatage UTC ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`)
    pub timestamp: String,
    pub started_sec: Option<i64>,
    pub position_sec: Option<i64>,
    pub total_sec: Option<i64>,
    pub synced: bool,
}

/// Statistiques agrégées d'écoute pour la vue Statistiques.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListeningStatsSummary {
    pub total_listened_ms: i64,
    pub total_saved_ms: i64,
    pub completed_episodes: i64,
    pub by_month: Vec<MonthlyStat>,
    pub by_feed: Vec<FeedStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyStat {
    pub month: String, // "YYYY-MM"
    pub listened_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedStat {
    pub feed_id: i64,
    pub feed_title: String,
    pub listened_ms: i64,
    pub completed_count: i64,
}

/// Santé du stockage hors-ligne (vue Téléchargements).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealth {
    pub total_media_bytes: u64,
    pub total_covers_bytes: u64,
    pub total_runtime_bytes: u64,
    pub db_size_bytes: u64,
    pub downloaded_count: i64,
    pub played_downloaded_count: i64,
    pub by_feed: Vec<FeedStorageUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedStorageUsage {
    pub feed_id: i64,
    pub feed_title: String,
    pub bytes: u64,
    pub episode_count: i64,
}

/// État d'un téléchargement actif (émis en temps réel vers le frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTaskInfo {
    pub episode_id: i64,
    pub title: String,
    pub feed_title: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    /// "queued", "downloading", "paused", "completed", "failed", "cancelled"
    pub status: String,
    pub error: Option<String>,
}

/// Instantané temps réel de l'état du lecteur audio natif (émis toutes les 250 ms).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStateSnapshot {
    pub episode_id: Option<i64>,
    pub title: String,
    pub feed_title: String,
    pub cover_url: Option<String>,
    pub is_playing: bool,
    pub is_loading: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub speed: f32,
    pub volume: f32,
    pub skip_silence: bool,
    pub voice_boost: bool,
    pub normalize_loudness: bool,
    /// Temps restant de la minuterie de sommeil en secondes (`None` si inactive).
    pub sleep_timer_remaining_sec: Option<u64>,
    /// Indique si la minuterie est réglée sur "Fin de l'épisode".
    pub sleep_at_end_of_episode: bool,
    pub error: Option<String>,
}

impl Default for AudioStateSnapshot {
    fn default() -> Self {
        Self {
            episode_id: None,
            title: String::new(),
            feed_title: String::new(),
            cover_url: None,
            is_playing: false,
            is_loading: false,
            position_ms: 0,
            duration_ms: 0,
            speed: 1.0,
            volume: 1.0,
            skip_silence: false,
            voice_boost: false,
            normalize_loudness: true,
            sleep_timer_remaining_sec: None,
            sleep_at_end_of_episode: false,
            error: None,
        }
    }
}

/// Stratégies d'insertion dans la file d'attente (manuelle et automatique).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueInsertStrategy {
    Bottom,
    Top,
    AfterCurrent,
    SmartDateAsc,
    SmartDateDesc,
}

impl QueueInsertStrategy {
    pub fn from_str_or_default(s: &str) -> Self {
        match s {
            "top" => Self::Top,
            "after_current" => Self::AfterCurrent,
            "smart_date_asc" => Self::SmartDateAsc,
            "smart_date_desc" => Self::SmartDateDesc,
            _ => Self::Bottom,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::AfterCurrent => "after_current",
            Self::SmartDateAsc => "smart_date_asc",
            Self::SmartDateDesc => "smart_date_desc",
        }
    }
}

/// Comportement à la fin d'un épisode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndOfEpisodeBehavior {
    PlayNext,
    Pause,
    Repeat,
}

/// Requête de filtrage / tri pour la liste centrale virtualisée.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EpisodeQuery {
    /// "queue", "inbox", "all", "downloads", "favorites", "history", "feed", "tag"
    pub view: String,
    pub feed_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub search: Option<String>,
    pub only_unplayed: bool,
    pub only_in_progress: bool,
    pub only_downloaded: bool,
    pub only_favorites: bool,
    pub only_with_chapters: bool,
    /// "pub_date", "duration_ms", "title", "feed_title", "file_size", "played_at", "queue_position"
    pub sort_by: Option<String>,
    pub sort_asc: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Compteurs globaux de la barre latérale.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SidebarCounts {
    pub queue: i64,
    pub inbox: i64,
    pub all_unplayed: i64,
    pub downloads: i64,
    pub favorites: i64,
    pub history: i64,
}

/// Résultat d'une recherche d'annuaire (iTunes Search / Podcast Index).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryPodcast {
    pub title: String,
    pub author: String,
    pub feed_url: String,
    pub image_url: Option<String>,
    pub description: Option<String>,
    pub categories: Vec<String>,
    pub source: String, // "itunes" | "podcastindex"
}

/// Aperçu d'un flux RSS avant abonnement (mode Preview).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedPreview {
    pub title: String,
    pub feed_url: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub funding_url: Option<String>,
    pub episodes: Vec<PreviewEpisode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewEpisode {
    pub title: String,
    pub pub_date: i64,
    pub duration_ms: i64,
    pub audio_url: String,
    pub description: Option<String>,
}

/// Préférences globales regroupant les 6 sections du panneau Paramètres.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    // Langue de l'application ("auto", "fr", "en", "de", "es")
    pub language: String,

    // Section 1 : Stockage & Dossiers
    pub dir_data: String,
    pub dir_media: String,
    pub dir_covers: String,
    pub dir_exports: String,
    pub prefer_relative_paths: bool,

    // Section 2 : Lecture & Audio
    pub stream_without_downloading: bool,
    pub smart_rewind_enabled: bool,
    pub smart_rewind_sec: u32,
    pub default_speed: f32,
    pub skip_silence_enabled: bool,
    pub skip_silence_threshold_db: f32,
    pub normalize_loudness: bool,
    pub voice_boost: bool,
    pub end_of_episode: String, // "play_next" | "pause" | "repeat"

    // Section 3 : File & Automatisation
    pub queue_insert_manual: String,
    pub queue_insert_auto: String,
    pub queue_locked: bool,
    pub auto_download_enabled: bool,
    pub auto_download_on_queue: bool,
    pub global_cache_limit_mb: u64,
    pub default_max_kept_episodes: i64,
    pub auto_delete_played: bool,
    pub protect_favorites_from_delete: bool,

    // Section 4 : Interface, Barre des Tâches & Raccourcis
    pub theme: String,   // "system" | "dark" | "light"
    pub density: String, // "compact" | "comfortable"
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub taskbar_show_play_pause: bool,
    pub taskbar_show_next: bool,
    pub taskbar_show_mark_played: bool,
    pub shortcuts: HashMap<String, String>,

    // Section 5 : Synchronisation Mobile (AntennaPod / Nextcloud / gpodder)
    pub sync_provider: String, // "none" | "nextcloud" | "gpodder"
    pub sync_server_url: String,
    pub sync_username: String,
    /// Jamais renvoyé en clair : indique seulement s'il est configuré.
    pub sync_has_secret: bool,
    pub sync_device_name: String,
    pub sync_device_id: String,
    pub sync_episode_actions: bool,
    pub sync_subscriptions: bool,
    pub sync_download_delete: bool,
    pub sync_interval_min: u32,
    pub sync_last_timestamp: i64,

    // Clés API optionnelles Podcast Index (si l'utilisateur souhaite enrichir la découverte)
    pub podcast_index_key: String,
    pub podcast_index_has_secret: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        let mut shortcuts = HashMap::new();
        shortcuts.insert("play_pause".into(), "Space".into());
        shortcuts.insert("speed_down".into(), "[".into());
        shortcuts.insert("speed_up".into(), "]".into());
        shortcuts.insert("mark_played".into(), "KeyM".into());
        shortcuts.insert("mark_feed_played".into(), "Shift+KeyM".into());
        shortcuts.insert("add_bookmark".into(), "KeyB".into());
        shortcuts.insert("toggle_favorite".into(), "KeyF".into());
        shortcuts.insert("remove_from_queue".into(), "Delete".into());
        shortcuts.insert("command_palette".into(), "Ctrl+KeyK".into());
        shortcuts.insert("focus_search".into(), "Ctrl+KeyF".into());
        shortcuts.insert("refresh_feeds".into(), "F5".into());

        Self {
            language: "auto".to_string(),
            dir_data: "./data".to_string(),
            dir_media: "./data/media".to_string(),
            dir_covers: "./data/covers".to_string(),
            dir_exports: "./data/exports".to_string(),
            prefer_relative_paths: true,
            stream_without_downloading: true,
            smart_rewind_enabled: true,
            smart_rewind_sec: 3,
            default_speed: 1.0,
            skip_silence_enabled: false,
            skip_silence_threshold_db: -34.0,
            normalize_loudness: true,
            voice_boost: false,
            end_of_episode: "play_next".to_string(),
            queue_insert_manual: "bottom".to_string(),
            queue_insert_auto: "bottom".to_string(),
            queue_locked: false,
            auto_download_enabled: false,
            auto_download_on_queue: false,
            global_cache_limit_mb: 2048,
            default_max_kept_episodes: 10,
            auto_delete_played: false,
            protect_favorites_from_delete: true,
            theme: "dark".to_string(),
            density: "compact".to_string(),
            minimize_to_tray: false,
            close_to_tray: false,
            taskbar_show_play_pause: true,
            taskbar_show_next: true,
            taskbar_show_mark_played: true,
            shortcuts,
            sync_provider: "none".to_string(),
            sync_server_url: String::new(),
            sync_username: String::new(),
            sync_has_secret: false,
            sync_device_name: "PodDesktop PC".to_string(),
            sync_device_id: "poddesktop-pc".to_string(),
            sync_episode_actions: true,
            sync_subscriptions: true,
            sync_download_delete: false,
            sync_interval_min: 15,
            sync_last_timestamp: 0,
            podcast_index_key: String::new(),
            podcast_index_has_secret: false,
        }
    }
}

/// Rapport d'une opération de synchronisation (Test ou Sync complète).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub success: bool,
    pub message: String,
    pub pushed_actions: usize,
    pub pulled_actions: usize,
    #[serde(default)]
    pub raw_remote_actions: usize,
    #[serde(default)]
    pub unmatched_actions: usize,
    pub added_feeds: usize,
    pub removed_feeds: usize,
    #[serde(default)]
    pub failed_feeds: usize,
    #[serde(default)]
    pub devices: Vec<String>,
    pub synced_at: i64,
}
