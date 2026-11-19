/**
 * Panneau Paramètres unifié à défilement continu couplé à un menu latéral d'ancrage rapide (6 sections).
 *
 * Design moderne Desktop :
 * - En-têtes de sections avec badges d'icônes SVG dégradés
 * - Interrupteurs modernes (Toggle Switches) et sélecteurs segmentés (Thème, Densité)
 * - Sélecteur visuel par cartes pour la synchronisation AntennaPod (Nextcloud / gpodder v2)
 * - Grille 2 colonnes pour les raccourcis clavier personnalisables
 * - Sauvegarde instantanée des réglages locaux + bouton dédié Tester & Synchroniser pour le réseau
 */
import { api } from "../api";
import { SUPPORTED_LANGUAGES, formatBytes, setLanguage, t } from "../i18n";
import { applyVisualPreferences, escapeHtml, showToast, store } from "../state";
import type { AppSettings } from "../types";
import { confirmDialog } from "./dialogs";
import { flagIcon, icons } from "./icons";
import { eventToShortcutCombo, setShortcutCaptureMode } from "./shortcuts";

export interface SettingsCallbacks {
  onSettingsApplied: () => void;
  onDataChanged: () => void;
}

/** Génère un interrupteur Toggle Switch accessible et moderne. */
function toggleSwitch(attrs: string, checked: boolean): string {
  return `
    <label class="toggle-switch">
      <input type="checkbox" ${attrs} ${checked ? "checked" : ""} />
      <span class="toggle-slider"></span>
    </label>
  `;
}

export async function renderSettingsView(cb: SettingsCallbacks): Promise<void> {
  const center = document.getElementById("center-pane");
  if (!center) return;

  const s = (store.settings = await api.getSettings());

  const stratOptions = (selected: string) =>
    [
      ["bottom", t("settings.stratBottom")],
      ["top", t("settings.stratTop")],
      ["after_current", t("settings.stratAfterCurrent")],
      ["smart_date_asc", t("settings.stratSmartAsc")],
      ["smart_date_desc", t("settings.stratSmartDesc")],
    ]
      .map(
        ([v, lbl]) =>
          `<option value="${v}" ${selected === v ? "selected" : ""}>${escapeHtml(lbl)}</option>`
      )
      .join("");

  const shortcutsList = Object.entries(s.shortcuts)
    .map(
      ([k, v]) => `
      <div class="shortcut-item">
        <span style="color:var(--text-secondary);">${escapeHtml(k.replace(/_/g, " "))}</span>
        <button class="btn btn-sm" data-edit-shortcut="${escapeHtml(k)}" style="font-family:var(--font-mono);min-width:96px;">
          ${escapeHtml(v)}
        </button>
      </div>`
    )
    .join("");

  center.innerHTML = `
    <div class="view-header">
      <div class="view-header-top">
        <span style="display:inline-flex;color:var(--accent-hover);">${icons.settings(20)}</span>
        <div class="view-title">${escapeHtml(t("settings.title"))}</div>
        <span style="font-size:12px;color:var(--success);margin-left:auto;display:inline-flex;align-items:center;gap:6px;padding:3px 10px;border-radius:99px;background:rgba(16,185,129,0.12);">
          <span style="width:7px;height:7px;border-radius:50%;background:var(--success);"></span>
          ${escapeHtml(t("settings.autoSaved"))}
        </span>
      </div>
    </div>

    <div class="settings-layout">
      <nav class="settings-anchors" id="settings-anchors">
        <a class="settings-anchor-item active" href="#sec-storage">${icons.folder(16)}<span>${escapeHtml(t("settings.secStorage"))}</span></a>
        <a class="settings-anchor-item" href="#sec-audio">${icons.sliders(16)}<span>${escapeHtml(t("settings.secAudio"))}</span></a>
        <a class="settings-anchor-item" href="#sec-queue">${icons.queue(16)}<span>${escapeHtml(t("settings.secQueue"))}</span></a>
        <a class="settings-anchor-item" href="#sec-interface">${icons.palette(16)}<span>${escapeHtml(t("settings.secInterface"))}</span></a>
        <a class="settings-anchor-item" href="#sec-sync">${icons.sync(16)}<span>${escapeHtml(t("settings.secSync"))}</span></a>
        <a class="settings-anchor-item" href="#sec-io">${icons.database(16)}<span>${escapeHtml(t("settings.secImportExport"))}</span></a>
      </nav>

      <div class="settings-content" id="settings-scroll">
        <!-- SECTION 1 : Stockage & Dossiers -->
        <section class="settings-section" id="sec-storage">
          <div class="settings-section-head">
            <div class="settings-badge b-storage">${icons.folder(18)}</div>
            <div>
              <div class="settings-section-title">${escapeHtml(t("settings.secStorage"))}</div>
              <div class="settings-section-sub">bootstrap.json · ./data/</div>
            </div>
          </div>

          <div class="setting-row">
            <div class="setting-label">
              <span class="setting-label-main">${escapeHtml(t("settings.preferRelative"))}</span>
            </div>
            ${toggleSwitch('id="st-rel"', s.prefer_relative_paths)}
          </div>
          ${(["dir_data", "dir_media", "dir_covers", "dir_exports"] as const)
            .map((key) => {
              const labelKey =
                key === "dir_data"
                  ? "settings.dirData"
                  : key === "dir_media"
                    ? "settings.dirMedia"
                    : key === "dir_covers"
                      ? "settings.dirCovers"
                      : "settings.dirExports";
              return `
            <div class="setting-row">
              <span class="setting-label-main">${escapeHtml(t(labelKey))}</span>
              <div style="display:flex;gap:8px;align-items:center;">
                <input type="text" id="st-${key}" value="${escapeHtml(s[key])}" readonly style="font-family:var(--font-mono);font-size:12px;" />
                <button class="btn" data-browse-dir="${key}">${icons.folder(14)} ${escapeHtml(t("app.browse"))}</button>
              </div>
            </div>`;
            })
            .join("")}
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("downloads.title"))}</span>
            <div style="display:flex;gap:8px;">
              <button class="btn" id="st-clear-cache">${escapeHtml(t("downloads.clearCacheBtn"))}</button>
              <button class="btn" id="st-purge-played">${escapeHtml(
                t("downloads.purgePlayedBtn", { count: "✓" })
              )}</button>
            </div>
          </div>
        </section>

        <!-- SECTION 2 : Lecture & Audio -->
        <section class="settings-section" id="sec-audio">
          <div class="settings-section-head">
            <div class="settings-badge b-audio">${icons.sliders(18)}</div>
            <div>
              <div class="settings-section-title">${escapeHtml(t("settings.secAudio"))}</div>
              <div class="settings-section-sub">Symphonia · WSOLA · EBU R128 · Voice Boost</div>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.streamWithoutDownloading"))}</span>
            ${toggleSwitch('data-bind-bool="stream_without_downloading"', s.stream_without_downloading)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.smartRewind"))}</span>
            ${toggleSwitch('data-bind-bool="smart_rewind_enabled"', s.smart_rewind_enabled)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.smartRewindSec"))}</span>
            <input type="number" min="0" max="60" data-bind-num="smart_rewind_sec" value="${s.smart_rewind_sec}" style="min-width:110px;width:110px;" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.defaultSpeed"))}</span>
            <input type="number" step="0.05" min="0.5" max="3.0" data-bind-num="default_speed" value="${s.default_speed}" style="min-width:110px;width:110px;" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.skipSilence"))}</span>
            ${toggleSwitch('data-bind-bool="skip_silence_enabled"', s.skip_silence_enabled)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.skipSilenceDb"))}</span>
            <input type="number" min="-55" max="-15" step="1" data-bind-num="skip_silence_threshold_db" value="${s.skip_silence_threshold_db}" style="min-width:110px;width:110px;" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.normalizeLoudness"))}</span>
            ${toggleSwitch('data-bind-bool="normalize_loudness"', s.normalize_loudness)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.voiceBoost"))}</span>
            ${toggleSwitch('data-bind-bool="voice_boost"', s.voice_boost)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.endOfEpisode"))}</span>
            <select data-bind-str="end_of_episode">
              <option value="play_next" ${s.end_of_episode === "play_next" ? "selected" : ""}>${escapeHtml(t("settings.endPlayNext"))}</option>
              <option value="pause" ${s.end_of_episode === "pause" ? "selected" : ""}>${escapeHtml(t("settings.endPause"))}</option>
              <option value="repeat" ${s.end_of_episode === "repeat" ? "selected" : ""}>${escapeHtml(t("settings.endRepeat"))}</option>
            </select>
          </div>
        </section>

        <!-- SECTION 3 : File & Automatisation -->
        <section class="settings-section" id="sec-queue">
          <div class="settings-section-head">
            <div class="settings-badge b-queue">${icons.queue(18)}</div>
            <div>
              <div class="settings-section-title">${escapeHtml(t("settings.secQueue"))}</div>
              <div class="settings-section-sub">Queue · Auto-Download · Auto-Delete</div>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.queueManual"))}</span>
            <select data-bind-str="queue_insert_manual">${stratOptions(s.queue_insert_manual)}</select>
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.queueAuto"))}</span>
            <select data-bind-str="queue_insert_auto">${stratOptions(s.queue_insert_auto)}</select>
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.autoDownload"))}</span>
            ${toggleSwitch('data-bind-bool="auto_download_enabled"', s.auto_download_enabled)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.autoDownloadQueue"))}</span>
            ${toggleSwitch('data-bind-bool="auto_download_on_queue"', s.auto_download_on_queue)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.maxKeptEpisodes"))}</span>
            <input type="number" min="0" data-bind-num="default_max_kept_episodes" value="${s.default_max_kept_episodes}" style="min-width:110px;width:110px;" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.globalCacheMb"))}</span>
            <input type="number" min="0" data-bind-num="global_cache_limit_mb" value="${s.global_cache_limit_mb}" style="min-width:110px;width:110px;" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.autoDeletePlayed"))}</span>
            ${toggleSwitch('data-bind-bool="auto_delete_played"', s.auto_delete_played)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.protectFavorites"))}</span>
            ${toggleSwitch('data-bind-bool="protect_favorites_from_delete"', s.protect_favorites_from_delete)}
          </div>
        </section>

        <!-- SECTION 4 : Interface, Langue, Barre des Tâches & Raccourcis -->
        <section class="settings-section" id="sec-interface">
          <div class="settings-section-head">
            <div class="settings-badge b-ui">${icons.palette(18)}</div>
            <div>
              <div class="settings-section-title">${escapeHtml(t("settings.secInterface"))}</div>
              <div class="settings-section-sub">FR / EN / DE / ES · Theme · Windows Taskbar · Shortcuts</div>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-label-main" style="display:inline-flex;align-items:center;gap:7px;">
              ${icons.globe(15)} ${escapeHtml(t("settings.language"))}
            </span>
            <div class="lang-pills" id="st-language-pills">
              <button type="button" class="lang-pill-btn ${s.language === "auto" ? "active" : ""}" data-lang-choice="auto">
                ${icons.globe(13)} ${escapeHtml(t("settings.languageAuto"))}
              </button>
              ${SUPPORTED_LANGUAGES.map(
                (l) => `
                <button type="button" class="lang-pill-btn ${s.language === l.code ? "active" : ""}" data-lang-choice="${l.code}">
                  ${flagIcon(l.code, 18, 13)} ${escapeHtml(l.label)}
                </button>`
              ).join("")}
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.theme"))}</span>
            <div class="segmented" data-seg="theme">
              <button class="segmented-btn ${s.theme === "dark" ? "active" : ""}" data-val="dark">${escapeHtml(t("settings.themeDark"))}</button>
              <button class="segmented-btn ${s.theme === "light" ? "active" : ""}" data-val="light">${escapeHtml(t("settings.themeLight"))}</button>
              <button class="segmented-btn ${s.theme === "system" ? "active" : ""}" data-val="system">${escapeHtml(t("settings.themeSystem"))}</button>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.density"))}</span>
            <div class="segmented" data-seg="density">
              <button class="segmented-btn ${s.density === "compact" ? "active" : ""}" data-val="compact">${escapeHtml(t("settings.densityCompact"))}</button>
              <button class="segmented-btn ${s.density === "comfortable" ? "active" : ""}" data-val="comfortable">${escapeHtml(t("settings.densityComfortable"))}</button>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.minimizeToTray"))}</span>
            ${toggleSwitch('data-bind-bool="minimize_to_tray"', s.minimize_to_tray)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.closeToTray"))}</span>
            ${toggleSwitch('data-bind-bool="close_to_tray"', s.close_to_tray)}
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.taskbarButtons"))}</span>
            <div style="display:flex;gap:14px;flex-wrap:wrap;">
              <label style="display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
                <input type="checkbox" data-bind-bool="taskbar_show_play_pause" ${s.taskbar_show_play_pause ? "checked" : ""} />
                ${escapeHtml(t("settings.tbPlayPause"))}
              </label>
              <label style="display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
                <input type="checkbox" data-bind-bool="taskbar_show_next" ${s.taskbar_show_next ? "checked" : ""} />
                ${escapeHtml(t("settings.tbNext"))}
              </label>
              <label style="display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
                <input type="checkbox" data-bind-bool="taskbar_show_mark_played" ${s.taskbar_show_mark_played ? "checked" : ""} />
                ${escapeHtml(t("settings.tbMarkPlayed"))}
              </label>
            </div>
          </div>

          <div style="margin-top:16px;display:flex;align-items:center;gap:8px;font-weight:600;color:var(--text-primary);">
            ${icons.keyboard(15)} ${escapeHtml(t("settings.shortcutsTitle"))}
          </div>
          <div class="shortcut-grid">${shortcutsList}</div>
        </section>

        <!-- SECTION 5 : Synchronisation Mobile (AntennaPod / Nextcloud / gpodder) -->
        <section class="settings-section" id="sec-sync">
          <div class="settings-section-head">
            <div class="settings-badge b-sync">${icons.sync(18)}</div>
            <div>
              <div class="settings-section-title">${escapeHtml(t("settings.secSync"))}</div>
              <div class="settings-section-sub">AntennaPod Android ↔ PodDesktop · AES-256-GCM</div>
            </div>
          </div>

          <!-- Sélecteur visuel par cartes -->
          <input type="hidden" id="sync-provider" value="${escapeHtml(s.sync_provider)}" />
          <div class="provider-grid" id="sync-provider-cards">
            <button type="button" class="provider-card ${s.sync_provider === "none" ? "active" : ""}" data-prov="none">
              <span class="provider-card-title">${escapeHtml(t("settings.syncNone"))}</span>
              <span class="provider-card-sub">100 % local</span>
            </button>
            <button type="button" class="provider-card ${s.sync_provider === "nextcloud" ? "active" : ""}" data-prov="nextcloud">
              <span class="provider-card-title">Nextcloud GPodder ★</span>
              <span class="provider-card-sub">Login Flow v2 · AntennaPod</span>
            </button>
            <button type="button" class="provider-card ${s.sync_provider === "gpodder" ? "active" : ""}" data-prov="gpodder">
              <span class="provider-card-title">gpodder API v2</span>
              <span class="provider-card-sub">gpodder.net · oPodSync · PodFetch</span>
            </button>
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncServerUrl"))}</span>
            <input type="text" id="sync-url" value="${escapeHtml(s.sync_server_url)}" placeholder="https://gpodder.net" />
          </div>
          <div class="setting-row ${s.sync_provider === "nextcloud" ? "" : "hidden"}" id="sync-nc-row">
            <span class="setting-label-main">Nextcloud Login Flow v2</span>
            <button class="btn" id="sync-nc-oauth">${icons.cloud(15)} ${escapeHtml(t("settings.syncNextcloudOAuth"))}</button>
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncUsername"))}</span>
            <input type="text" id="sync-user" value="${escapeHtml(s.sync_username)}" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main" style="display:inline-flex;align-items:center;gap:6px;">
              ${icons.shield(14)} ${escapeHtml(t("settings.syncSecret"))}
            </span>
            <input type="password" id="sync-secret" placeholder="${escapeHtml(
              s.sync_has_secret ? t("settings.syncSecretPlaceholder") : "••••••••"
            )}" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncDeviceName"))}</span>
            <input type="text" id="sync-dev-name" value="${escapeHtml(s.sync_device_name)}" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncDeviceId"))}</span>
            <input type="text" id="sync-dev-id" value="${escapeHtml(s.sync_device_id)}" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncEpisodeActions"))}</span>
            ${toggleSwitch('id="sync-ep-act"', s.sync_episode_actions)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncSubscriptions"))}</span>
            ${toggleSwitch('id="sync-subs"', s.sync_subscriptions)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncDownloadDelete"))}</span>
            ${toggleSwitch('id="sync-dldel"', s.sync_download_delete)}
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.syncInterval"))}</span>
            <input type="number" min="5" max="1440" id="sync-interval" value="${s.sync_interval_min}" style="min-width:110px;width:110px;" />
          </div>
          <div style="display:flex;justify-content:flex-end;gap:10px;flex-wrap:wrap;margin-top:14px;">
            <button class="btn btn-danger" id="sync-wipe-pull" style="height:36px;padding:0 14px;">
              🗑 ${escapeHtml(t("settings.syncWipeAndPull"))}
            </button>
            <button class="btn" id="sync-full-reset" style="height:36px;padding:0 14px;">
              ${icons.refresh(14)} ${escapeHtml(t("settings.syncFullReset"))}
            </button>
            <button class="btn btn-primary" id="sync-test-run" style="height:36px;padding:0 16px;">
              ${icons.sync(15)} ${escapeHtml(t("settings.syncTestAndRun"))}
            </button>
          </div>
          <div style="margin-top:10px;padding:9px 12px;border-radius:8px;background:var(--bg-elevated);border:1px solid var(--border-subtle);font-size:11.5px;color:var(--text-secondary);line-height:1.5;">
            ℹ ${escapeHtml(t("settings.syncProtocolNote"))}
          </div>
          <div id="sync-status-box" class="hidden" style="margin-top:12px;padding:11px 14px;border-radius:9px;border:1px solid var(--border-strong);background:var(--bg-elevated);font-size:12.5px;line-height:1.5;user-select:text;"></div>

          <hr style="border:none;border-top:1px solid var(--border-subtle);margin:18px 0;" />
          <div style="font-weight:600;margin-bottom:6px;">${escapeHtml(t("settings.podcastIndexTitle"))}</div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.podcastIndexKey"))}</span>
            <input type="text" id="pi-key" value="${escapeHtml(s.podcast_index_key)}" />
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.podcastIndexSecret"))}</span>
            <input type="password" id="pi-secret" placeholder="${s.podcast_index_has_secret ? "••••••••" : ""}" />
          </div>
        </section>

        <!-- SECTION 6 : Import / Export & Base de Données -->
        <section class="settings-section" id="sec-io">
          <div class="settings-section-head">
            <div class="settings-badge b-db">${icons.database(18)}</div>
            <div>
              <div class="settings-section-title">${escapeHtml(t("settings.secImportExport"))}</div>
              <div class="settings-section-sub">OPML 2.0 · JSON · SQLite WAL + FTS5</div>
            </div>
          </div>

          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.opmlTitle"))}</span>
            <div style="display:flex;gap:6px;flex-wrap:wrap;justify-content:flex-end;">
              <button class="btn" id="io-import-opml">${escapeHtml(t("settings.importOpml"))}</button>
              <button class="btn" id="io-export-opml">${escapeHtml(t("settings.exportOpml"))}</button>
              <button class="btn" id="io-fav-html">${escapeHtml(t("settings.exportFavHtml"))}</button>
              <button class="btn" id="io-fav-md">${escapeHtml(t("settings.exportFavMd"))}</button>
              <button class="btn" id="io-fav-json">${escapeHtml(t("settings.exportFavJson"))}</button>
            </div>
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.prefsTitle"))}</span>
            <div style="display:flex;gap:6px;">
              <button class="btn" id="io-export-prefs">${escapeHtml(t("settings.exportPrefs"))}</button>
              <button class="btn" id="io-import-prefs">${escapeHtml(t("settings.importPrefs"))}</button>
            </div>
          </div>
          <div class="setting-row">
            <span class="setting-label-main">${escapeHtml(t("settings.dbTitle"))}</span>
            <div style="display:flex;gap:6px;flex-wrap:wrap;justify-content:flex-end;">
              <button class="btn" id="io-backup-db">${escapeHtml(t("settings.backupDb"))}</button>
              <button class="btn" id="io-restore-db">${escapeHtml(t("settings.restoreDb"))}</button>
              <button class="btn btn-primary" id="io-vacuum-db">${escapeHtml(t("settings.vacuumDb"))}</button>
            </div>
          </div>
        </section>
      </div>
    </div>
  `;

  // Ancrage latéral fluide + scroll-spy automatique
  const scrollPane = center.querySelector("#settings-scroll") as HTMLElement;
  const anchorLinks = Array.from(center.querySelectorAll<HTMLAnchorElement>(".settings-anchor-item"));
  anchorLinks.forEach((a) => {
    a.addEventListener("click", (e) => {
      e.preventDefault();
      anchorLinks.forEach((x) => x.classList.remove("active"));
      a.classList.add("active");
      center.querySelector(a.getAttribute("href")!)?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  });

  scrollPane?.addEventListener(
    "scroll",
    () => {
      const sections = Array.from(scrollPane.querySelectorAll<HTMLElement>(".settings-section"));
      let activeId = sections[0]?.id;
      for (const sec of sections) {
        if (sec.offsetTop - scrollPane.offsetTop <= scrollPane.scrollTop + 90) {
          activeId = sec.id;
        }
      }
      anchorLinks.forEach((a) => a.classList.toggle("active", a.getAttribute("href") === `#${activeId}`));
    },
    { passive: true }
  );

  // Sauvegarde instantanée des réglages locaux
  const persistLocal = async () => {
    await api.saveSettings(s);
    applyVisualPreferences(s.theme, s.density);
    cb.onSettingsApplied();
  };

  center.querySelectorAll<HTMLInputElement>("[data-bind-bool]").forEach((el) => {
    el.addEventListener("change", () => {
      const k = el.dataset.bindBool as keyof AppSettings;
      (s as unknown as Record<string, unknown>)[k] = el.checked;
      void persistLocal();
    });
  });

  center.querySelectorAll<HTMLInputElement>("[data-bind-num]").forEach((el) => {
    el.addEventListener("change", () => {
      const k = el.dataset.bindNum as keyof AppSettings;
      (s as unknown as Record<string, unknown>)[k] = Number(el.value);
      void persistLocal();
    });
  });

  center.querySelectorAll<HTMLSelectElement>("[data-bind-str]").forEach((el) => {
    el.addEventListener("change", () => {
      const k = el.dataset.bindStr as keyof AppSettings;
      (s as unknown as Record<string, unknown>)[k] = el.value;
      void persistLocal();
    });
  });

  // Contrôles segmentés (Thème & Densité)
  center.querySelectorAll<HTMLElement>("[data-seg]").forEach((seg) => {
    const key = seg.dataset.seg as "theme" | "density";
    seg.querySelectorAll<HTMLButtonElement>(".segmented-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        seg.querySelectorAll(".segmented-btn").forEach((b) => b.classList.remove("active"));
        btn.classList.add("active");
        s[key] = btn.dataset.val!;
        void persistLocal();
      });
    });
  });

  // Changement de langue à chaud par drapeau (UI + Backend System Tray / Taskbar)
  center.querySelectorAll<HTMLButtonElement>("[data-lang-choice]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      s.language = btn.dataset.langChoice!;
      setLanguage(s.language);
      await api.saveSettings(s);
      cb.onSettingsApplied();
      void renderSettingsView(cb);
    });
  });

  // Personnalisation interactive des raccourcis clavier
  center.querySelectorAll<HTMLButtonElement>("[data-edit-shortcut]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const actionKey = btn.dataset.editShortcut!;
      const prev = btn.textContent || "";
      btn.textContent = "…";
      btn.classList.add("btn-primary");
      setShortcutCaptureMode(true);

      const onKey = async (ev: KeyboardEvent) => {
        ev.preventDefault();
        ev.stopPropagation();
        if (["Control", "Shift", "Alt", "Meta"].includes(ev.key)) return;
        window.removeEventListener("keydown", onKey, true);
        setShortcutCaptureMode(false);
        btn.classList.remove("btn-primary");
        if (ev.key === "Escape") {
          btn.textContent = prev;
          return;
        }
        const combo = eventToShortcutCombo(ev);
        s.shortcuts[actionKey] = combo;
        btn.textContent = combo;
        await persistLocal();
      };
      window.addEventListener("keydown", onKey, true);
    });
  });

  // Stockage & Dossiers
  center.querySelector("#st-rel")?.addEventListener("change", async (e) => {
    const rel = (e.target as HTMLInputElement).checked;
    store.settings = await api.updateStorageDirs({ preferRelative: rel });
    void renderSettingsView(cb);
  });

  center.querySelectorAll<HTMLElement>("[data-browse-dir]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const key = btn.dataset.browseDir!;
      const picked = await api.pickFolder(key);
      if (!picked) return;
      const arg =
        key === "dir_data"
          ? { dirData: picked }
          : key === "dir_media"
            ? { dirMedia: picked }
            : key === "dir_covers"
              ? { dirCovers: picked }
              : { dirExports: picked };
      store.settings = await api.updateStorageDirs(arg);
      void renderSettingsView(cb);
    });
  });

  center.querySelector("#st-clear-cache")?.addEventListener("click", async () => {
    const freed = await api.clearWebviewCache();
    showToast(t("toasts.cacheCleared", { size: formatBytes(freed) }), "success");
  });
  center.querySelector("#st-purge-played")?.addEventListener("click", async () => {
    const [count, freed] = await api.purgePlayedDownloads();
    showToast(t("downloads.purgeDone", { count, size: formatBytes(freed) }), "success");
    cb.onDataChanged();
  });

  // Cartes fournisseurs de synchro
  const syncProviderHidden = center.querySelector("#sync-provider") as HTMLInputElement;
  const syncUrlEl = center.querySelector("#sync-url") as HTMLInputElement | null;
  const syncNcRow = center.querySelector("#sync-nc-row") as HTMLElement | null;
  center.querySelectorAll<HTMLButtonElement>("#sync-provider-cards .provider-card").forEach((card) => {
    card.addEventListener("click", () => {
      const prov = card.dataset.prov!;
      syncProviderHidden.value = prov;
      center.querySelectorAll("#sync-provider-cards .provider-card").forEach((c) => c.classList.remove("active"));
      card.classList.add("active");
      syncNcRow?.classList.toggle("hidden", prov !== "nextcloud");
      if (prov === "gpodder" && syncUrlEl && !syncUrlEl.value.trim()) {
        syncUrlEl.value = "https://gpodder.net";
      }
    });
  });

  // Nextcloud Login Flow v2
  center.querySelector("#sync-nc-oauth")?.addEventListener("click", async () => {
    const url = (center.querySelector("#sync-url") as HTMLInputElement).value.trim();
    if (!url) {
      showToast(t("settings.syncServerUrl"), "error");
      return;
    }
    try {
      const init = await api.startNextcloudLogin(url);
      await api.openExternalUrl(init.login_url);
      showToast(t("settings.syncNextcloudWaiting"), "info");
      for (let i = 0; i < 40; i++) {
        await new Promise((r) => setTimeout(r, 3000));
        const res = await api.pollNextcloudLogin(init.poll_endpoint, init.poll_token);
        if (res) {
          showToast(`Nextcloud: ${res.login_name}`, "success");
          void renderSettingsView(cb);
          return;
        }
      }
    } catch (e) {
      showToast(String(e), "error");
    }
  });

  // Validation, sauvegarde chiffrée et test immédiat de synchronisation (normale, reset curseur ou wipe complet)
  const runSyncFromSettings = async (mode: "normal" | "reset" | "wipe") => {
    const statusBox = center.querySelector("#sync-status-box") as HTMLElement | null;
    const btnRun = center.querySelector("#sync-test-run") as HTMLButtonElement | null;
    const btnReset = center.querySelector("#sync-full-reset") as HTMLButtonElement | null;
    const btnWipe = center.querySelector("#sync-wipe-pull") as HTMLButtonElement | null;

    if (mode === "wipe") {
      const ok = await confirmDialog(
        t("settings.syncWipeAndPull"),
        t("settings.syncWipeConfirm"),
        true
      );
      if (!ok) return;
    }

    const setStatus = (msg: string, kind: "info" | "ok" | "err") => {
      if (!statusBox) return;
      statusBox.classList.remove("hidden");
      statusBox.style.borderColor =
        kind === "ok" ? "var(--success)" : kind === "err" ? "var(--danger)" : "var(--accent)";
      statusBox.style.color = kind === "err" ? "var(--danger)" : "var(--text-primary)";
      statusBox.textContent = msg;
    };

    s.sync_provider = syncProviderHidden.value;
    s.sync_server_url = (center.querySelector("#sync-url") as HTMLInputElement).value.trim();
    if (s.sync_provider === "gpodder" && !s.sync_server_url) {
      s.sync_server_url = "https://gpodder.net";
      if (syncUrlEl) syncUrlEl.value = s.sync_server_url;
    }
    s.sync_username = (center.querySelector("#sync-user") as HTMLInputElement).value.trim();
    s.sync_device_name = (center.querySelector("#sync-dev-name") as HTMLInputElement).value.trim();
    s.sync_device_id = (center.querySelector("#sync-dev-id") as HTMLInputElement).value.trim();
    s.sync_episode_actions = (center.querySelector("#sync-ep-act") as HTMLInputElement).checked;
    s.sync_subscriptions = (center.querySelector("#sync-subs") as HTMLInputElement).checked;
    s.sync_download_delete = (center.querySelector("#sync-dldel") as HTMLInputElement).checked;
    s.sync_interval_min = Number((center.querySelector("#sync-interval") as HTMLInputElement).value) || 15;
    s.podcast_index_key = (center.querySelector("#pi-key") as HTMLInputElement).value.trim();

    if (btnRun) btnRun.disabled = true;
    if (btnReset) btnReset.disabled = true;
    if (btnWipe) btnWipe.disabled = true;
    setStatus(t("app.loading"), "info");

    try {
      const secretVal = (center.querySelector("#sync-secret") as HTMLInputElement).value;
      if (secretVal) await api.saveSyncSecret(secretVal);
      const piSecret = (center.querySelector("#pi-secret") as HTMLInputElement).value;
      if (piSecret) await api.savePodcastIndexSecret(piSecret);
      await api.saveSettings(s);

      if (s.sync_provider !== "none") {
        const rep =
          mode === "wipe"
            ? await api.wipeAndSyncFromServer()
            : mode === "reset"
              ? await api.resetAndSyncNow()
              : await api.syncNow();
        const msg = t("toasts.syncSuccess", {
          pushed: rep.pushed_actions,
          pulled: rep.pulled_actions,
        });
        const rawInfo =
          rep.raw_remote_actions !== undefined
            ? ` (${rep.pulled_actions}/${rep.raw_remote_actions} actions serveur)`
            : "";
        const devInfo = rep.devices && rep.devices.length > 0 ? ` · Appareils : ${rep.devices.join(", ")}` : "";
        const failInfo = rep.failed_feeds && rep.failed_feeds > 0 ? ` · ⚠ ${rep.failed_feeds} RSS err` : "";
        setStatus(
          `✓ ${msg}${rawInfo} · +${rep.added_feeds} / -${rep.removed_feeds} podcasts${devInfo}${failInfo}`,
          "ok"
        );
        showToast(msg, "success");
        cb.onDataChanged();
      } else {
        setStatus(`✓ ${t("settings.autoSaved")}`, "ok");
        showToast(t("settings.autoSaved"), "success");
      }
    } catch (e) {
      const errText = String(e);
      setStatus(`✕ ${errText}`, "err");
      showToast(errText, "error");
    } finally {
      if (btnRun) btnRun.disabled = false;
      if (btnReset) btnReset.disabled = false;
      if (btnWipe) btnWipe.disabled = false;
    }
  };

  center.querySelector("#sync-test-run")?.addEventListener("click", () => void runSyncFromSettings("normal"));
  center.querySelector("#sync-full-reset")?.addEventListener("click", () => void runSyncFromSettings("reset"));
  center.querySelector("#sync-wipe-pull")?.addEventListener("click", () => void runSyncFromSettings("wipe"));

  // Section 6 : Import / Export & Maintenance SQLite
  center.querySelector("#io-import-opml")?.addEventListener("click", async () => {
    const file = await api.pickFile("OPML", ["opml", "xml"]);
    if (!file) return;
    try {
      const count = await api.importOpml(file);
      showToast(t("toasts.importDone", { count }), "success");
      cb.onDataChanged();
    } catch (e) {
      showToast(String(e), "error");
    }
  });
  center.querySelector("#io-export-opml")?.addEventListener("click", async () => {
    const path = await api.exportOpml();
    showToast(t("toasts.exportDone", { path }), "success");
  });
  center.querySelector("#io-fav-html")?.addEventListener("click", async () => {
    const path = await api.exportFavorites("html");
    showToast(t("toasts.exportDone", { path }), "success");
  });
  center.querySelector("#io-fav-md")?.addEventListener("click", async () => {
    const path = await api.exportFavorites("markdown");
    showToast(t("toasts.exportDone", { path }), "success");
  });
  center.querySelector("#io-fav-json")?.addEventListener("click", async () => {
    const path = await api.exportFavorites("json");
    showToast(t("toasts.exportDone", { path }), "success");
  });
  center.querySelector("#io-export-prefs")?.addEventListener("click", async () => {
    const path = await api.exportSettingsJson();
    showToast(t("toasts.exportDone", { path }), "success");
  });
  center.querySelector("#io-import-prefs")?.addEventListener("click", async () => {
    const file = await api.pickFile("JSON", ["json"]);
    if (!file) return;
    const count = await api.importSettingsJson(file);
    showToast(t("toasts.importDone", { count }), "success");
    void renderSettingsView(cb);
  });
  center.querySelector("#io-backup-db")?.addEventListener("click", async () => {
    const dest = await api.saveFile("Backup SQLite", "poddesktop-backup.db", ["db"]);
    const path = await api.backupDatabase(dest || undefined);
    showToast(t("toasts.backupDone", { path }), "success");
  });
  center.querySelector("#io-restore-db")?.addEventListener("click", async () => {
    const src = await api.pickFile("SQLite DB", ["db"]);
    if (!src) return;
    try {
      await api.restoreDatabase(src);
      showToast(t("toasts.restoreDone"), "success");
      cb.onDataChanged();
    } catch (e) {
      showToast(String(e), "error");
    }
  });
  center.querySelector("#io-vacuum-db")?.addEventListener("click", async () => {
    await api.vacuumDatabase();
    showToast(t("toasts.vacuumDone"), "success");
  });
}
