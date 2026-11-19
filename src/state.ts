/**
 * Store réactif central ultra-léger de PodDesktop (sans framework externe).
 * Gère la navigation, les filtres combinables, la multi-sélection desktop,
 * l'état audio temps réel et les notifications toast.
 */
import type {
  AppSettings,
  AudioStateSnapshot,
  DownloadTaskInfo,
  Episode,
  Feed,
  SidebarCounts,
  Tag,
  ViewKind,
} from "./types";

export interface AppStore {
  view: ViewKind;
  activeFeedId: number | null;
  activeTagId: number | null;

  // Données chargées
  counts: SidebarCounts;
  feeds: Feed[];
  tags: Tag[];
  episodes: Episode[];
  downloads: Map<number, DownloadTaskInfo>;
  settings: AppSettings | null;

  // État liste & filtres combinables
  searchQuery: string;
  filterUnplayed: boolean;
  filterInProgress: boolean;
  filterDownloaded: boolean;
  filterFavorites: boolean;
  filterChapters: boolean;
  filterFeedId: number | null;
  filterTagId: number | null;
  sortBy: string;
  sortAsc: boolean;

  // Multi-sélection Desktop (Clic, Ctrl+Clic, Shift+Clic, Ctrl+A)
  selectedIds: Set<number>;
  lastSelectedIndex: number;
  inspectedEpisodeId: number | null;
  inspectorTab: "notes" | "chapters" | "bookmarks" | "transcript";

  // Lecteur audio & Mini-Lecteur
  audio: AudioStateSnapshot;
  isMiniPlayer: boolean;
  miniDock: "top" | "bottom";
  isRefreshing: boolean;
  isSyncing: boolean;
}

export const store: AppStore = {
  view: "queue",
  activeFeedId: null,
  activeTagId: null,
  counts: { queue: 0, inbox: 0, all_unplayed: 0, downloads: 0, favorites: 0, history: 0 },
  feeds: [],
  tags: [],
  episodes: [],
  downloads: new Map(),
  settings: null,
  searchQuery: "",
  filterUnplayed: false,
  filterInProgress: false,
  filterDownloaded: false,
  filterFavorites: false,
  filterChapters: false,
  filterFeedId: null,
  filterTagId: null,
  sortBy: "pub_date",
  sortAsc: false,
  selectedIds: new Set(),
  lastSelectedIndex: -1,
  inspectedEpisodeId: null,
  inspectorTab: "notes",
  audio: {
    episode_id: null,
    title: "",
    feed_title: "",
    cover_url: null,
    is_playing: false,
    is_loading: false,
    position_ms: 0,
    duration_ms: 0,
    speed: 1.0,
    volume: 1.0,
    skip_silence: false,
    voice_boost: false,
    normalize_loudness: true,
    sleep_timer_remaining_sec: null,
    sleep_at_end_of_episode: false,
    error: null,
  },
  isMiniPlayer: false,
  miniDock: "bottom",
  isRefreshing: false,
  isSyncing: false,
};

/** Affiche une notification toast discrète et non bloquante en bas à droite. */
export function showToast(message: string, kind: "info" | "success" | "error" = "info"): void {
  const root = document.getElementById("toast-root");
  if (!root) return;
  const el = document.createElement("div");
  el.className = `toast toast-${kind}`;
  el.textContent = message;
  root.appendChild(el);
  requestAnimationFrame(() => el.classList.add("visible"));
  setTimeout(() => {
    el.classList.remove("visible");
    setTimeout(() => el.remove(), 220);
  }, 3400);
}

/** Applique le thème (dark / light / system) et la densité sur `<html>`. */
export function applyVisualPreferences(theme: string, density: string): void {
  const root = document.documentElement;
  let effective = theme;
  if (theme === "system") {
    effective = window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  root.dataset.theme = effective === "light" ? "light" : "dark";
  root.dataset.density = density === "comfortable" ? "comfortable" : "compact";
}

/** Échappe du texte brut avant injection HTML. */
export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
