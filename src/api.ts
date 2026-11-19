/**
 * Passerelle typée vers toutes les commandes IPC du backend Rust (Tauri v2).
 */
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  AppSettings,
  AudioStateSnapshot,
  Bookmark,
  Chapter,
  DiscoveryPodcast,
  DownloadTaskInfo,
  Episode,
  EpisodeQuery,
  Feed,
  FeedPreview,
  FeedSettingsUpdate,
  ListeningStatsSummary,
  NextcloudLoginInit,
  NextcloudLoginResult,
  SidebarCounts,
  StorageHealth,
  SyncReport,
  Tag,
  TranscriptCue,
} from "./types";

/** Convertit un chemin local de pochette en URL chargeable par la WebView, ou conserve l'URL HTTPS. */
export function resolveCoverUrl(raw: string | null | undefined): string {
  if (!raw) return "";
  if (raw.startsWith("http://") || raw.startsWith("https://") || raw.startsWith("data:")) {
    return raw;
  }
  try {
    return convertFileSrc(raw);
  } catch {
    return "";
  }
}

export const api = {
  // --- Données & Vues ---
  getSidebarCounts: () => invoke<SidebarCounts>("cmd_get_sidebar_counts"),
  listFeeds: () => invoke<Feed[]>("cmd_list_feeds"),
  listTags: () => invoke<Tag[]>("cmd_list_tags"),
  queryEpisodes: (query: EpisodeQuery) => invoke<Episode[]>("cmd_query_episodes", { query }),
  getEpisode: (episodeId: number) => invoke<Episode | null>("cmd_get_episode", { episodeId }),
  getChapters: (episodeId: number) => invoke<Chapter[]>("cmd_get_chapters", { episodeId }),
  listBookmarks: (episodeId: number) => invoke<Bookmark[]>("cmd_list_bookmarks", { episodeId }),
  addBookmark: (episodeId: number, positionMs: number, note: string) =>
    invoke<Bookmark>("cmd_add_bookmark", { episodeId, positionMs, note }),
  deleteBookmark: (bookmarkId: number) => invoke<void>("cmd_delete_bookmark", { bookmarkId }),
  loadTranscript: (episodeId: number) =>
    invoke<TranscriptCue[]>("cmd_load_transcript", { episodeId }),
  getStats: () => invoke<ListeningStatsSummary>("cmd_get_stats"),
  getStorageHealth: () => invoke<StorageHealth>("cmd_get_storage_health"),

  // --- Abonnements, Tags & Dossiers Locaux ---
  subscribeFeed: (feedUrl: string, username?: string, password?: string) =>
    invoke<number>("cmd_subscribe_feed", {
      feedUrl,
      username: username || null,
      password: password || null,
    }),
  addLocalFolder: (folderPath: string, customTitle?: string) =>
    invoke<number>("cmd_add_local_folder", {
      folderPath,
      customTitle: customTitle || null,
    }),
  refreshFeed: (feedId: number) => invoke<number>("cmd_refresh_feed", { feedId }),
  refreshAllFeeds: () => invoke<number>("cmd_refresh_all_feeds"),
  deleteFeed: (feedId: number) => invoke<void>("cmd_delete_feed", { feedId }),
  updateFeedSettings: (update: FeedSettingsUpdate) =>
    invoke<void>("cmd_update_feed_settings", { update }),
  upsertTag: (name: string, color?: string) =>
    invoke<number>("cmd_upsert_tag", { name, color: color || null }),
  updateTag: (tagId: number, name: string, color: string) =>
    invoke<void>("cmd_update_tag", { tagId, name, color }),
  deleteTag: (tagId: number) => invoke<void>("cmd_delete_tag", { tagId }),
  setFeedTags: (feedId: number, tagIds: number[]) =>
    invoke<void>("cmd_set_feed_tags", { feedId, tagIds }),

  // --- Découverte ---
  searchDirectory: (query: string) => invoke<DiscoveryPodcast[]>("cmd_search_directory", { query }),
  previewFeed: (feedUrl: string, username?: string, password?: string) =>
    invoke<FeedPreview>("cmd_preview_feed", {
      feedUrl,
      username: username || null,
      password: password || null,
    }),

  // --- Actions Épisodes & File ---
  markEpisodesPlayed: (episodeIds: number[], played: boolean) =>
    invoke<{ affected_ids: number[]; files_to_delete: string[] }>("cmd_mark_episodes_played", {
      episodeIds,
      played,
    }),
  markFeedAllPlayed: (feedId: number) =>
    invoke<{ affected_ids: number[] }>("cmd_mark_feed_all_played", { feedId }),
  markTagAllPlayed: (tagId: number) =>
    invoke<{ affected_ids: number[] }>("cmd_mark_tag_all_played", { tagId }),
  setEpisodesFavorite: (episodeIds: number[], favorite: boolean) =>
    invoke<void>("cmd_set_episodes_favorite", { episodeIds, favorite }),
  setInboxState: (episodeIds: number[], inboxState: number) =>
    invoke<void>("cmd_set_inbox_state", { episodeIds, inboxState }),
  dismissFeedFromInbox: (feedId: number) =>
    invoke<number>("cmd_dismiss_feed_from_inbox", { feedId }),
  dismissAllInbox: () => invoke<number>("cmd_dismiss_all_inbox"),
  enqueueEpisodes: (episodeIds: number[], strategy?: string) =>
    invoke<void>("cmd_enqueue_episodes", { episodeIds, strategy: strategy || null }),
  removeFromQueue: (episodeIds: number[]) =>
    invoke<void>("cmd_remove_from_queue", { episodeIds }),
  reorderQueue: (episodeId: number, newIndex: number) =>
    invoke<void>("cmd_reorder_queue", { episodeId, newIndex }),
  clearQueue: () => invoke<void>("cmd_clear_queue"),

  // --- Lecteur Audio Natif ---
  getAudioState: () => invoke<AudioStateSnapshot>("cmd_get_audio_state"),
  playEpisode: (episodeId: number, startMs?: number) =>
    invoke<void>("cmd_play_episode", { episodeId, startMs: startMs ?? null }),
  togglePlayPause: () => invoke<void>("cmd_toggle_play_pause"),
  pauseAudio: () => invoke<void>("cmd_pause_audio"),
  seekAudio: (positionMs: number) => invoke<void>("cmd_seek_audio", { positionMs }),
  setSpeed: (speed: number, rememberForFeed = true) =>
    invoke<void>("cmd_set_speed", { speed, rememberForFeed }),
  setVolume: (volume: number) => invoke<void>("cmd_set_volume", { volume }),
  setAudioFx: (skipSilence: boolean, voiceBoost: boolean, normalizeLoudness: boolean) =>
    invoke<void>("cmd_set_audio_fx", { skipSilence, voiceBoost, normalizeLoudness }),
  setSleepTimer: (minutes: number | null, endOfEpisode: boolean) =>
    invoke<void>("cmd_set_sleep_timer", { minutes, endOfEpisode }),
  playNextInQueue: () => invoke<void>("cmd_play_next_in_queue"),

  // --- Téléchargements & Stockage ---
  downloadEpisode: (episodeId: number) => invoke<string>("cmd_download_episode", { episodeId }),
  pauseDownload: (episodeId: number) => invoke<void>("cmd_pause_download", { episodeId }),
  cancelDownload: (episodeId: number) => invoke<void>("cmd_cancel_download", { episodeId }),
  listDownloads: () => invoke<DownloadTaskInfo[]>("cmd_list_downloads"),
  deleteLocalFile: (episodeId: number) => invoke<void>("cmd_delete_local_file", { episodeId }),
  purgePlayedDownloads: () => invoke<[number, number]>("cmd_purge_played_downloads"),
  purgeQueueDownloads: () => invoke<[number, number]>("cmd_purge_queue_downloads"),
  purgeNonQueueDownloads: () => invoke<[number, number]>("cmd_purge_non_queue_downloads"),
  clearWebviewCache: () => invoke<number>("cmd_clear_webview_cache"),
  openEpisodeFolder: (episodeId: number) => invoke<void>("cmd_open_episode_folder", { episodeId }),

  // --- Synchronisation AntennaPod ---
  startNextcloudLogin: (serverUrl: string) =>
    invoke<NextcloudLoginInit>("cmd_start_nextcloud_login", { serverUrl }),
  pollNextcloudLogin: (pollEndpoint: string, pollToken: string) =>
    invoke<NextcloudLoginResult | null>("cmd_poll_nextcloud_login", { pollEndpoint, pollToken }),
  saveSyncSecret: (secretPlain: string) => invoke<void>("cmd_save_sync_secret", { secretPlain }),
  syncNow: () => invoke<SyncReport>("cmd_sync_now"),
  resetAndSyncNow: () => invoke<SyncReport>("cmd_reset_and_sync_now"),
  wipeAndSyncFromServer: () => invoke<SyncReport>("cmd_wipe_and_sync_from_server"),

  // --- Paramètres, Import/Export & Maintenance ---
  getSettings: () => invoke<AppSettings>("cmd_get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("cmd_save_settings", { settings }),
  updateStorageDirs: (args: {
    dirData?: string;
    dirMedia?: string;
    dirCovers?: string;
    dirExports?: string;
    preferRelative?: boolean;
  }) => invoke<AppSettings>("cmd_update_storage_dirs", args),
  savePodcastIndexSecret: (secretPlain: string) =>
    invoke<void>("cmd_save_podcast_index_secret", { secretPlain }),
  importOpml: (filePath: string) => invoke<number>("cmd_import_opml", { filePath }),
  exportOpml: (destPath?: string) => invoke<string>("cmd_export_opml", { destPath: destPath || null }),
  exportFavorites: (format: string, destPath?: string) =>
    invoke<string>("cmd_export_favorites", { format, destPath: destPath || null }),
  exportSettingsJson: (destPath?: string) =>
    invoke<string>("cmd_export_settings_json", { destPath: destPath || null }),
  importSettingsJson: (filePath: string) => invoke<number>("cmd_import_settings_json", { filePath }),
  backupDatabase: (destPath?: string) =>
    invoke<string>("cmd_backup_database", { destPath: destPath || null }),
  restoreDatabase: (srcPath: string) => invoke<void>("cmd_restore_database", { srcPath }),
  vacuumDatabase: () => invoke<void>("cmd_vacuum_database"),

  // --- Dialogues natifs & Navigateur externe ---
  pickFolder: async (title?: string): Promise<string | null> => {
    const res = await openDialog({ directory: true, multiple: false, title });
    return typeof res === "string" ? res : null;
  },
  pickFile: async (
    title: string,
    extensions: string[]
  ): Promise<string | null> => {
    const res = await openDialog({
      directory: false,
      multiple: false,
      title,
      filters: [{ name: title, extensions }],
    });
    return typeof res === "string" ? res : null;
  },
  saveFile: async (title: string, defaultName: string, extensions: string[]): Promise<string | null> => {
    const res = await saveDialog({
      title,
      defaultPath: defaultName,
      filters: [{ name: title, extensions }],
    });
    return typeof res === "string" ? res : null;
  },
  openExternalUrl: async (url: string): Promise<void> => {
    if (!url) return;
    try {
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  },
};
