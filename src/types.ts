/**
 * Définitions TypeScript strictes alignées 1:1 sur `src-tauri/src/models.rs`.
 */

export type ViewKind =
  | "queue"
  | "inbox"
  | "all"
  | "downloads"
  | "favorites"
  | "history"
  | "discover"
  | "stats"
  | "settings"
  | "feed"
  | "tag";

export interface Feed {
  id: number;
  title: string;
  feed_url: string;
  image_url: string | null;
  local_cover_path: string | null;
  description: string | null;
  author: string | null;
  language: string | null;
  is_local_folder: boolean;
  local_folder_path: string | null;
  last_update: number | null;
  etag: string | null;
  last_modified: string | null;
  playback_speed: number | null;
  auto_download: boolean;
  auto_delete: boolean;
  keep_updated: boolean;
  max_kept_episodes: number | null;
  include_keywords: string | null;
  exclude_keywords: string | null;
  funding_url: string | null;
  username: string | null;
  has_password: boolean;
  unread_count: number;
  tag_ids: number[];
}

export interface Tag {
  id: number;
  name: string;
  color: string;
  unread_count: number;
  feed_count: number;
}

export interface Episode {
  id: number;
  feed_id: number;
  feed_title: string;
  feed_image_url: string | null;
  feed_funding_url: string | null;
  guid: string;
  title: string;
  pub_date: number;
  description: string | null;
  audio_url: string;
  mime_type: string | null;
  local_file_path: string | null;
  file_size: number;
  duration_ms: number;
  position_ms: number;
  is_played: boolean;
  is_favorite: boolean;
  inbox_state: number;
  played_at: number | null;
  transcript_url: string | null;
  transcript_type: string | null;
  transcript_data: string | null;
  queue_position: number | null;
  has_chapters: boolean;
}

export interface Chapter {
  id: number;
  episode_id: number;
  start_time_ms: number;
  title: string;
  url: string | null;
  image_url: string | null;
}

export interface Bookmark {
  id: number;
  episode_id: number;
  position_ms: number;
  note: string;
  created_at: number;
}

export interface TranscriptCue {
  start_ms: number;
  end_ms: number;
  speaker: string | null;
  text: string;
}

export interface SidebarCounts {
  queue: number;
  inbox: number;
  all_unplayed: number;
  downloads: number;
  favorites: number;
  history: number;
}

export interface EpisodeQuery {
  view: string;
  feed_id: number | null;
  tag_id: number | null;
  search: string | null;
  only_unplayed: boolean;
  only_in_progress: boolean;
  only_downloaded: boolean;
  only_favorites: boolean;
  only_with_chapters: boolean;
  sort_by: string | null;
  sort_asc: boolean;
  limit: number | null;
  offset: number | null;
}

export interface AudioStateSnapshot {
  episode_id: number | null;
  title: string;
  feed_title: string;
  cover_url: string | null;
  is_playing: boolean;
  is_loading: boolean;
  position_ms: number;
  duration_ms: number;
  speed: number;
  volume: number;
  skip_silence: boolean;
  voice_boost: boolean;
  normalize_loudness: boolean;
  sleep_timer_remaining_sec: number | null;
  sleep_at_end_of_episode: boolean;
  error: string | null;
}

export interface DownloadTaskInfo {
  episode_id: number;
  title: string;
  feed_title: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  status: string;
  error: string | null;
}

export interface StorageHealth {
  total_media_bytes: number;
  total_covers_bytes: number;
  total_runtime_bytes: number;
  db_size_bytes: number;
  downloaded_count: number;
  played_downloaded_count: number;
  by_feed: {
    feed_id: number;
    feed_title: string;
    bytes: number;
    episode_count: number;
  }[];
}

export interface ListeningStatsSummary {
  total_listened_ms: number;
  total_saved_ms: number;
  completed_episodes: number;
  by_month: { month: string; listened_ms: number }[];
  by_feed: {
    feed_id: number;
    feed_title: string;
    listened_ms: number;
    completed_count: number;
  }[];
}

export interface DiscoveryPodcast {
  title: string;
  author: string;
  feed_url: string;
  image_url: string | null;
  description: string | null;
  categories: string[];
  source: string;
}

export interface FeedPreview {
  title: string;
  feed_url: string;
  author: string | null;
  description: string | null;
  image_url: string | null;
  funding_url: string | null;
  episodes: {
    title: string;
    pub_date: number;
    duration_ms: number;
    audio_url: string;
    description: string | null;
  }[];
}

export interface AppSettings {
  language: string;
  dir_data: string;
  dir_media: string;
  dir_covers: string;
  dir_exports: string;
  prefer_relative_paths: boolean;
  stream_without_downloading: boolean;
  smart_rewind_enabled: boolean;
  smart_rewind_sec: number;
  default_speed: number;
  skip_silence_enabled: boolean;
  skip_silence_threshold_db: number;
  normalize_loudness: boolean;
  voice_boost: boolean;
  end_of_episode: string;
  queue_insert_manual: string;
  queue_insert_auto: string;
  queue_locked: boolean;
  auto_download_enabled: boolean;
  auto_download_on_queue: boolean;
  global_cache_limit_mb: number;
  default_max_kept_episodes: number;
  auto_delete_played: boolean;
  protect_favorites_from_delete: boolean;
  theme: string;
  density: string;
  minimize_to_tray: boolean;
  close_to_tray: boolean;
  taskbar_show_play_pause: boolean;
  taskbar_show_next: boolean;
  taskbar_show_mark_played: boolean;
  shortcuts: Record<string, string>;
  sync_provider: string;
  sync_server_url: string;
  sync_username: string;
  sync_has_secret: boolean;
  sync_device_name: string;
  sync_device_id: string;
  sync_episode_actions: boolean;
  sync_subscriptions: boolean;
  sync_download_delete: boolean;
  sync_interval_min: number;
  sync_last_timestamp: number;
  podcast_index_key: string;
  podcast_index_has_secret: boolean;
}

export interface SyncReport {
  success: boolean;
  message: string;
  pushed_actions: number;
  pulled_actions: number;
  raw_remote_actions?: number;
  unmatched_actions?: number;
  added_feeds: number;
  removed_feeds: number;
  failed_feeds?: number;
  devices?: string[];
  synced_at: number;
}

export interface NextcloudLoginInit {
  login_url: string;
  poll_endpoint: string;
  poll_token: string;
}

export interface NextcloudLoginResult {
  server: string;
  login_name: string;
  app_password: string;
}

export interface FeedSettingsUpdate {
  feed_id: number;
  playback_speed: number | null;
  auto_download: boolean;
  auto_delete: boolean;
  keep_updated: boolean;
  max_kept_episodes: number | null;
  include_keywords: string | null;
  exclude_keywords: string | null;
  username: string | null;
  password_enc: string | null;
}
