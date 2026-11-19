/**
 * Palette de Commandes (`Ctrl + K`) :
 * Accès instantané au clavier à n'importe quel podcast, épisode (FTS5) ou action globale
 * ("Tout marquer comme lu", "Passer en 1.5x", "Importer OPML", "Synchroniser", etc.).
 */
import { api } from "../api";
import { t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";
import type { ViewKind } from "../types";

export interface PaletteCallbacks {
  onNavigate: (view: ViewKind, feedId?: number | null, tagId?: number | null) => void;
  onRefreshAll: () => void;
  onSyncNow: () => void;
  onToggleMiniPlayer: () => void;
  onDataChanged: () => void;
}

interface PaletteEntry {
  group: string;
  label: string;
  hint?: string;
  run: () => void | Promise<void>;
}

let isOpen = false;
let activeIndex = 0;

export function openCommandPalette(cb: PaletteCallbacks): void {
  const root = document.getElementById("command-palette");
  if (!root) return;
  isOpen = true;
  activeIndex = 0;

  root.innerHTML = `
    <div class="modal-card palette-card">
      <input type="text" class="palette-input" id="palette-input"
             placeholder="${escapeHtml(t("palette.placeholder"))}" />
      <div class="palette-results" id="palette-results"></div>
    </div>
  `;
  root.classList.remove("hidden");

  const input = root.querySelector("#palette-input") as HTMLInputElement;
  const resultsEl = root.querySelector("#palette-results") as HTMLElement;
  input.focus();

  let currentEntries: PaletteEntry[] = [];

  const renderResults = async () => {
    const q = input.value.trim().toLowerCase();
    const actions: PaletteEntry[] = [
      { group: t("palette.groupActions"), label: t("palette.cmdRefreshAll"), hint: "F5", run: cb.onRefreshAll },
      { group: t("palette.groupActions"), label: t("palette.cmdSyncNow"), run: cb.onSyncNow },
      {
        group: t("palette.groupActions"),
        label: t("palette.cmdImportOpml"),
        run: async () => {
          const path = await api.pickFile("OPML", ["opml", "xml"]);
          if (!path) return;
          const count = await api.importOpml(path);
          showToast(t("toasts.importDone", { count }), "success");
          cb.onDataChanged();
        },
      },
      {
        group: t("palette.groupActions"),
        label: t("palette.cmdExportOpml"),
        run: async () => {
          const path = await api.exportOpml();
          showToast(t("toasts.exportDone", { path }), "success");
        },
      },
      {
        group: t("palette.groupActions"),
        label: t("palette.cmdAddLocalFolder"),
        run: async () => {
          const dir = await api.pickFolder(t("nav.addLocalFolder"));
          if (!dir) return;
          const id = await api.addLocalFolder(dir);
          showToast(t("toasts.localFolderAdded"), "success");
          cb.onDataChanged();
          cb.onNavigate("feed", id, null);
        },
      },
      { group: t("palette.groupActions"), label: t("palette.cmdSpeed100"), run: () => api.setSpeed(1.0) },
      { group: t("palette.groupActions"), label: t("palette.cmdSpeed125"), run: () => api.setSpeed(1.25) },
      { group: t("palette.groupActions"), label: t("palette.cmdSpeed150"), run: () => api.setSpeed(1.5) },
      { group: t("palette.groupActions"), label: t("palette.cmdSpeed200"), run: () => api.setSpeed(2.0) },
      { group: t("palette.groupActions"), label: t("palette.cmdMiniPlayer"), run: cb.onToggleMiniPlayer },
      {
        group: t("palette.groupActions"),
        label: t("palette.cmdMarkCurrentPlayed"),
        hint: "M",
        run: async () => {
          if (store.audio.episode_id) {
            await api.markEpisodesPlayed([store.audio.episode_id], true);
            cb.onDataChanged();
          }
        },
      },
      {
        group: t("palette.groupActions"),
        label: t("palette.cmdOpenSettings"),
        run: () => cb.onNavigate("settings"),
      },
    ];

    const feedEntries: PaletteEntry[] = store.feeds.map((f) => ({
      group: t("palette.groupFeeds"),
      label: f.title,
      hint: f.unread_count > 0 ? `${f.unread_count}` : undefined,
      run: () => cb.onNavigate("feed", f.id, null),
    }));

    let epEntries: PaletteEntry[] = [];
    if (q.length >= 2) {
      const found = await api
        .queryEpisodes({
          view: "all",
          feed_id: null,
          tag_id: null,
          search: q,
          only_unplayed: false,
          only_in_progress: false,
          only_downloaded: false,
          only_favorites: false,
          only_with_chapters: false,
          sort_by: "pub_date",
          sort_asc: false,
          limit: 10,
          offset: 0,
        })
        .catch(() => []);
      epEntries = found.map((ep) => ({
        group: t("palette.groupEpisodes"),
        label: ep.title,
        hint: ep.feed_title,
        run: () => void api.playEpisode(ep.id),
      }));
    }

    const filtered = [
      ...actions.filter((a) => !q || a.label.toLowerCase().includes(q)),
      ...feedEntries.filter((f) => !q || f.label.toLowerCase().includes(q)).slice(0, 8),
      ...epEntries,
    ];
    currentEntries = filtered;
    activeIndex = Math.min(activeIndex, Math.max(0, filtered.length - 1));

    if (filtered.length === 0) {
      resultsEl.innerHTML = `<div style="padding:18px;text-align:center;color:var(--text-muted);">${escapeHtml(
        t("palette.noMatch")
      )}</div>`;
      return;
    }

    resultsEl.innerHTML = filtered
      .map(
        (item, idx) => `
      <div class="palette-item ${idx === activeIndex ? "active" : ""}" data-idx="${idx}">
        <div style="min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
          <span style="font-size:10.5px;color:var(--text-muted);margin-right:8px;">${escapeHtml(
            item.group
          )}</span>
          <span>${escapeHtml(item.label)}</span>
        </div>
        ${
          item.hint
            ? `<span style="font-size:11px;color:var(--text-muted);margin-left:8px;">${escapeHtml(
                item.hint
              )}</span>`
            : ""
        }
      </div>
    `
      )
      .join("");

    resultsEl.querySelectorAll<HTMLElement>(".palette-item").forEach((el) => {
      el.addEventListener("click", () => {
        const entry = currentEntries[Number(el.dataset.idx)];
        closeCommandPalette();
        void entry?.run();
      });
    });
  };

  void renderResults();

  input.addEventListener("input", () => {
    activeIndex = 0;
    void renderResults();
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closeCommandPalette();
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      activeIndex = Math.min(currentEntries.length - 1, activeIndex + 1);
      void renderResults();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      activeIndex = Math.max(0, activeIndex - 1);
      void renderResults();
    } else if (e.key === "Enter" && currentEntries[activeIndex]) {
      e.preventDefault();
      const entry = currentEntries[activeIndex];
      closeCommandPalette();
      void entry.run();
    }
  });

  root.onclick = (e) => {
    if (e.target === root) closeCommandPalette();
  };
}

export function closeCommandPalette(): void {
  isOpen = false;
  const root = document.getElementById("command-palette");
  if (root) {
    root.classList.add("hidden");
    root.innerHTML = "";
  }
}

export function isCommandPaletteOpen(): boolean {
  return isOpen;
}
