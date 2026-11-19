/**
 * Point d'entrée frontend de **PodDesktop** (Vanilla TypeScript ES2022).
 *
 * Orchestre le cycle de vie de l'interface desktop :
 * - Initialisation rapide (< 300 ms) : préférences, langue i18n, thème, écouteurs temps réel Tauri
 * - Coordination des 3 panneaux (Sidebar, Panneau Central, Inspecteur), Barre de Lecture,
 *   Palette Ctrl+K et raccourcis clavier
 */
import "./styles/tokens.css";
import "./styles/layout.css";
import "./styles/components.css";

import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { SUPPORTED_LANGUAGES, getActiveLang, onLanguageChange, setLanguage, t } from "./i18n";
import { applyVisualPreferences, escapeHtml, showToast, store } from "./state";
import type { AudioStateSnapshot, DownloadTaskInfo, SyncReport, ViewKind } from "./types";
import { openCommandPalette } from "./ui/commandPalette";
import { renderDiscoverView } from "./ui/discoverView";
import { renderDownloadsView } from "./ui/downloadsView";
import { refreshVirtualListRows, renderEpisodeListView } from "./ui/episodeList";
import {
  invalidateInspectorCache,
  renderInspector,
  syncTranscriptHighlight,
} from "./ui/inspector";
import {
  patchPlayerProgressOnly,
  renderPlayerBar,
  toggleMiniPlayerMode,
} from "./ui/playerBar";
import { renderSettingsView } from "./ui/settingsView";
import { registerKeyboardShortcuts } from "./ui/shortcuts";
import { renderSidebar } from "./ui/sidebar";
import { renderStatsView } from "./ui/statsView";
import { flagIcon, icons } from "./ui/icons";

let searchDebounce: number | null = null;
let lastAudioTrackKey = "";
let langOutsideClickBound = false;

async function bootstrap(): Promise<void> {
  // 1. Charge les réglages et applique immédiatement langue + thème/densité
  try {
    const settings = await api.getSettings();
    store.settings = settings;
    setLanguage(settings.language);
    applyVisualPreferences(settings.theme, settings.density);
  } catch (e) {
    console.error("Erreur init settings :", e);
  }

  // 2. Re-rendu instantané sur changement de langue à chaud
  onLanguageChange(() => {
    renderTopbar();
    renderAllPanels();
  });

  // 3. Raccourcis clavier globaux
  registerKeyboardShortcuts({
    onOpenPalette: () => triggerPalette(),
    onFocusSearch: () => {
      const inp = document.getElementById("global-search") as HTMLInputElement | null;
      inp?.focus();
      inp?.select();
    },
    onRefreshAll: () => void handleRefreshAll(),
    onSelectAll: () => {
      if (isEpisodeListView(store.view)) {
        store.selectedIds = new Set(store.episodes.map((e) => e.id));
        renderCenterPanel();
      }
    },
    onDataChanged: () => void reloadAllData(),
    onBookmarkSaved: () => {
      invalidateInspectorCache();
      void renderInspector(store.inspectedEpisodeId);
    },
  });

  // 4. Événements temps réel émis par le backend Rust
  await listen<AudioStateSnapshot>("audio-state", (event) => {
    const snap = event.payload;
    store.audio = snap;
    if (snap.error) showToast(snap.error, "error");

    const trackKey = `${snap.episode_id}|${snap.is_playing}|${snap.is_loading}|${snap.speed}|${snap.skip_silence}|${snap.voice_boost}|${snap.sleep_at_end_of_episode}|${snap.sleep_timer_remaining_sec !== null}`;
    if (trackKey !== lastAudioTrackKey) {
      lastAudioTrackKey = trackKey;
      renderPlayerBar(playerCallbacks);
      refreshVirtualListRows();
    } else {
      patchPlayerProgressOnly();
    }
    syncTranscriptHighlight(snap.episode_id, snap.position_ms);
  });

  await listen<DownloadTaskInfo>("download-progress", (event) => {
    store.downloads.set(event.payload.episode_id, event.payload);
    if (store.view === "downloads") {
      void renderDownloadsView({ onDataChanged: () => void reloadAllData() });
    }
  });

  await listen<string>("data-changed", () => {
    void reloadAllData();
  });

  await listen<SyncReport>("sync-completed", (event) => {
    const r = event.payload;
    if (r.pulled_actions > 0 || r.added_feeds > 0) {
      showToast(
        t("toasts.syncSuccess", { pushed: r.pushed_actions, pulled: r.pulled_actions }),
        "success"
      );
    }
  });

  await listen("os-toggle-mini-player", () => {
    void toggleMiniPlayerMode(playerCallbacks);
  });

  // 5. Hydratation initiale des données + premier rendu
  renderTopbar();
  store.audio = await api.getAudioState().catch(() => store.audio);
  renderPlayerBar(playerCallbacks);
  await reloadAllData();
}

const playerCallbacks = {
  onOpenQueue: () => navigateTo("queue", null, null),
  onDataChanged: () => void reloadAllData(),
};

function isEpisodeListView(v: ViewKind): boolean {
  return (
    v === "queue" ||
    v === "inbox" ||
    v === "all" ||
    v === "favorites" ||
    v === "history" ||
    v === "feed" ||
    v === "tag"
  );
}

function renderTopbar(): void {
  const bar = document.getElementById("topbar");
  if (!bar) return;

  const activeLang = getActiveLang();

  bar.innerHTML = `
    <div class="brand">
      <div class="brand-badge">${icons.podcast(15)}</div>
      <span>PodDesktop</span>
    </div>

    <div class="search-box">
      <span class="search-icon">${icons.search(14)}</span>
      <input type="text" id="global-search" value="${escapeHtml(store.searchQuery)}"
             placeholder="${escapeHtml(t("app.searchPlaceholder"))}" />
      <button class="kbd-hint" id="btn-open-palette" title="${escapeHtml(
        t("app.commandPaletteHint")
      )}">Ctrl+K</button>
    </div>

    <div class="topbar-actions">
      <button class="topbar-btn topbar-btn-icon-only" id="btn-refresh-all"
              title="${escapeHtml(t("app.refreshAll"))}">
        <span class="${store.isRefreshing ? "spin" : ""}" style="display:inline-flex;">${icons.refresh(17)}</span>
      </button>
      <button class="topbar-btn" id="btn-sync-now"
              title="${escapeHtml(t("app.syncNow"))}">
        <span class="${store.isSyncing ? "spin" : ""}" style="display:inline-flex;color:var(--accent-cyan);">${icons.sync(16)}</span>
        <span>Sync</span>
      </button>
      <div class="lang-switcher" id="lang-switcher">
        <button class="topbar-btn" id="btn-lang-toggle" title="${escapeHtml(t("settings.language"))}">
          ${flagIcon(activeLang, 18, 13)}
          <span style="font-size:11.5px;font-weight:700;letter-spacing:0.03em;">${activeLang.toUpperCase()}</span>
        </button>
        <div class="lang-dropdown hidden" id="lang-dropdown">
          ${SUPPORTED_LANGUAGES.map(
            (l) => `
            <button class="lang-option ${activeLang === l.code ? "active" : ""}" data-pick-lang="${l.code}">
              ${flagIcon(l.code, 18, 13)}
              <span>${escapeHtml(l.label)}</span>
              ${activeLang === l.code ? '<span style="margin-left:auto;">✓</span>' : ""}
            </button>`
          ).join("")}
        </div>
      </div>
      <button class="topbar-btn topbar-btn-icon-only" id="btn-mini-top"
              title="${escapeHtml(t("app.miniPlayer"))}">
        ${icons.miniPlayer(17)}
      </button>
      <button class="topbar-btn ${store.view === "settings" ? "active" : ""}" id="btn-settings"
              title="${escapeHtml(t("app.settings"))}">
        ${icons.settings(16)}
        <span>${escapeHtml(t("app.settings"))}</span>
      </button>
    </div>
  `;

  // Menu déroulant rapide des langues par drapeau (FR / EN / DE / ES)
  const langBtn = bar.querySelector("#btn-lang-toggle");
  const langMenu = bar.querySelector("#lang-dropdown");
  langBtn?.addEventListener("click", (e) => {
    e.stopPropagation();
    langMenu?.classList.toggle("hidden");
  });
  if (!langOutsideClickBound) {
    langOutsideClickBound = true;
    document.addEventListener("click", () => {
      document.getElementById("lang-dropdown")?.classList.add("hidden");
    });
  }
  bar.querySelectorAll<HTMLElement>("[data-pick-lang]").forEach((opt) => {
    opt.addEventListener("click", async (e) => {
      e.stopPropagation();
      const code = opt.dataset.pickLang!;
      langMenu?.classList.add("hidden");
      setLanguage(code);
      if (store.settings) {
        store.settings.language = code;
        await api.saveSettings(store.settings).catch(() => {});
      }
    });
  });

  const searchInp = bar.querySelector("#global-search") as HTMLInputElement;
  searchInp?.addEventListener("input", () => {
    store.searchQuery = searchInp.value;
    if (searchDebounce) window.clearTimeout(searchDebounce);
    searchDebounce = window.setTimeout(() => {
      if (!isEpisodeListView(store.view) && store.searchQuery.trim()) {
        store.view = "all";
      }
      void loadCurrentViewEpisodes();
    }, 140);
  });

  bar.querySelector("#btn-open-palette")?.addEventListener("click", triggerPalette);
  bar.querySelector("#btn-refresh-all")?.addEventListener("click", () => void handleRefreshAll());
  bar.querySelector("#btn-sync-now")?.addEventListener("click", () => void handleSyncNow());
  bar.querySelector("#btn-mini-top")?.addEventListener("click", () => {
    void toggleMiniPlayerMode(playerCallbacks);
  });
  bar.querySelector("#btn-settings")?.addEventListener("click", () => {
    navigateTo("settings", null, null);
  });
}

function navigateTo(view: ViewKind, feedId: number | null = null, tagId: number | null = null): void {
  store.view = view;
  store.activeFeedId = feedId;
  store.activeTagId = tagId;
  store.selectedIds.clear();
  store.lastSelectedIndex = -1;
  store.filterFeedId = null;
  store.filterTagId = null;

  // Masque l'inspecteur droit sur les vues pleine largeur (Paramètres, Statistiques, Découvrir)
  const ws = document.querySelector(".workspace");
  ws?.classList.toggle("no-inspector", !isEpisodeListView(view));

  renderTopbar();
  renderSidebar({ onNavigate: navigateTo, onDataChanged: () => void reloadAllData() });
  void loadCurrentViewEpisodes();
}

async function reloadAllData(): Promise<void> {
  try {
    const [counts, feeds, tags] = await Promise.all([
      api.getSidebarCounts(),
      api.listFeeds(),
      api.listTags(),
    ]);
    store.counts = counts;
    store.feeds = feeds;
    store.tags = tags;
  } catch (e) {
    console.error("Erreur chargement données globales :", e);
  }

  renderSidebar({ onNavigate: navigateTo, onDataChanged: () => void reloadAllData() });
  await loadCurrentViewEpisodes();
}

async function loadCurrentViewEpisodes(): Promise<void> {
  if (isEpisodeListView(store.view) || store.view === "downloads") {
    try {
      store.episodes = await api.queryEpisodes({
        view: store.view,
        feed_id: store.view === "feed" ? store.activeFeedId : store.filterFeedId,
        tag_id: store.view === "tag" ? store.activeTagId : store.filterTagId,
        search: store.searchQuery.trim() || null,
        only_unplayed: store.filterUnplayed,
        only_in_progress: store.filterInProgress,
        only_downloaded: store.filterDownloaded,
        only_favorites: store.filterFavorites,
        only_with_chapters: store.filterChapters,
        sort_by: store.view === "queue" ? null : store.sortBy,
        sort_asc: store.sortAsc,
        limit: null,
        offset: null,
      });
    } catch (e) {
      showToast(String(e), "error");
    }
  }
  renderCenterPanel();
  if (isEpisodeListView(store.view)) {
    void renderInspector(store.inspectedEpisodeId);
  }
}

function renderAllPanels(): void {
  renderSidebar({ onNavigate: navigateTo, onDataChanged: () => void reloadAllData() });
  renderCenterPanel();
  if (isEpisodeListView(store.view)) {
    void renderInspector(store.inspectedEpisodeId);
  }
  renderPlayerBar(playerCallbacks);
}

function renderCenterPanel(): void {
  if (store.view === "discover") {
    renderDiscoverView({
      onSubscribed: (feedId) => {
        void reloadAllData().then(() => navigateTo("feed", feedId, null));
      },
    });
    return;
  }
  if (store.view === "downloads") {
    void renderDownloadsView({ onDataChanged: () => void reloadAllData() });
    return;
  }
  if (store.view === "stats") {
    void renderStatsView();
    return;
  }
  if (store.view === "settings") {
    void renderSettingsView({
      onSettingsApplied: () => {
        renderTopbar();
        renderPlayerBar(playerCallbacks);
      },
      onDataChanged: () => void reloadAllData(),
    });
    return;
  }

  renderEpisodeListView({
    onSelectInspect: (epId) => void renderInspector(epId),
    onRefreshView: () => void loadCurrentViewEpisodes(),
    onDataChanged: () => void reloadAllData(),
  });
}

function triggerPalette(): void {
  openCommandPalette({
    onNavigate: navigateTo,
    onRefreshAll: () => void handleRefreshAll(),
    onSyncNow: () => void handleSyncNow(),
    onToggleMiniPlayer: () => void toggleMiniPlayerMode(playerCallbacks),
    onDataChanged: () => void reloadAllData(),
  });
}

async function handleRefreshAll(): Promise<void> {
  if (store.isRefreshing) return;
  store.isRefreshing = true;
  renderTopbar();
  try {
    const count = await api.refreshAllFeeds();
    showToast(t("toasts.refreshed", { count }), "success");
    await reloadAllData();
  } catch (e) {
    showToast(String(e), "error");
  } finally {
    store.isRefreshing = false;
    renderTopbar();
  }
}

async function handleSyncNow(): Promise<void> {
  if (store.isSyncing) return;
  store.isSyncing = true;
  renderTopbar();
  try {
    const rep = await api.syncNow();
    showToast(
      t("toasts.syncSuccess", { pushed: rep.pushed_actions, pulled: rep.pulled_actions }),
      "success"
    );
    await reloadAllData();
  } catch (e) {
    showToast(String(e), "error");
  } finally {
    store.isSyncing = false;
    renderTopbar();
  }
}

void bootstrap();
