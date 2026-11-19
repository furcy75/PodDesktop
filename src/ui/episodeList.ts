/**
 * Panneau central : Liste dense virtualisée (> 50 000 épisodes), bandeau contextuel d'abonnement
 * avec "Tout marquer comme lu", filtres combinables, multi-sélection desktop, actions rapides au
 * survol, clic droit complet et réorganisation Drag & Drop précise de la file d'attente.
 */
import { api, resolveCoverUrl } from "../api";
import { formatBytes, formatDate, formatDurationMs, formatHumanTime, t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";
import type { Episode } from "../types";
import { showContextMenu } from "./contextMenu";
import { confirmDialog, openAssignFeedTagsDialog, openFeedSettingsDialog } from "./dialogs";
import { VirtualList } from "./virtualList";

export interface EpisodeListCallbacks {
  onSelectInspect: (episodeId: number) => void;
  onRefreshView: () => void;
  onDataChanged: () => void;
}

let activeVList: VirtualList<Episode> | null = null;
let dragEpisodeId: number | null = null;

export function renderEpisodeListView(cb: EpisodeListCallbacks): void {
  const center = document.getElementById("center-pane");
  if (!center) return;

  const isQueue = store.view === "queue";
  const isInbox = store.view === "inbox";
  const activeFeed =
    store.view === "feed" ? store.feeds.find((f) => f.id === store.activeFeedId) ?? null : null;
  const activeTag =
    store.view === "tag" ? store.tags.find((tg) => tg.id === store.activeTagId) ?? null : null;

  // Calcul durée totale restante ajustée à la vitesse (pour la vue File d'attente)
  const speed = store.audio.speed || 1.0;
  const remainingMs = store.episodes.reduce(
    (acc, e) => acc + Math.max(0, (e.duration_ms - e.position_ms) / speed),
    0
  );
  const totalBytes = store.episodes.reduce((acc, e) => acc + (e.local_file_path ? e.file_size : 0), 0);

  // En-tête contextuel (spécial Abonnement, Tag, File d'attente ou vue standard)
  let headerTopHtml = "";
  if (activeFeed) {
    const cover = resolveCoverUrl(activeFeed.local_cover_path || activeFeed.image_url);
    headerTopHtml = `
      <div class="feed-banner" style="flex:1;">
        ${
          cover
            ? `<img class="feed-banner-cover" src="${escapeHtml(cover)}" alt="" />`
            : `<div class="feed-banner-cover" style="display:grid;place-items:center;font-size:22px;">${
                activeFeed.is_local_folder ? "📁" : "🎙"
              }</div>`
        }
        <div style="min-width:0;flex:1;">
          <div class="view-title">${escapeHtml(activeFeed.title)}</div>
          <div class="view-meta">${escapeHtml(activeFeed.author || "")} · ${escapeHtml(
      t("list.episodesCount", { count: store.episodes.length })
    )}</div>
        </div>
        <div style="display:flex;gap:6px;flex-wrap:wrap;">
          <button class="btn btn-primary" id="btn-feed-mark-all">✓ ${escapeHtml(
            t("list.markAllPlayed")
          )}</button>
          <button class="btn" id="btn-feed-refresh">↻ ${escapeHtml(t("list.refreshFeed"))}</button>
          <button class="btn" id="btn-feed-tags">🏷</button>
          <button class="btn" id="btn-feed-settings">⚙ ${escapeHtml(t("list.feedSettings"))}</button>
        </div>
      </div>
    `;
  } else if (activeTag) {
    headerTopHtml = `
      <div style="display:flex;align-items:center;gap:10px;flex:1;">
        <span class="tag-dot" style="width:14px;height:14px;background:${escapeHtml(
          activeTag.color
        )}"></span>
        <div class="view-title">${escapeHtml(activeTag.name)}</div>
        <div class="view-meta">${escapeHtml(
          t("list.episodesCount", { count: store.episodes.length })
        )}</div>
        <div style="margin-left:auto;display:flex;gap:6px;">
          <button class="btn btn-primary" id="btn-tag-mark-all">✓ ${escapeHtml(
            t("list.markAllPlayed")
          )}</button>
        </div>
      </div>
    `;
  } else {
    const titleKey = `nav.${store.view}`;
    const metaExtra = isQueue
      ? ` · ${t("list.remainingTime", { time: formatHumanTime(remainingMs) })} · ${formatBytes(
          totalBytes
        )}`
      : "";
    headerTopHtml = `
      <div style="display:flex;align-items:center;gap:12px;flex:1;">
        <div class="view-title">${escapeHtml(t(titleKey))}</div>
        <div class="view-meta">${escapeHtml(
          t("list.episodesCount", { count: store.episodes.length })
        )}${escapeHtml(metaExtra)}</div>
        ${
          isQueue
            ? `<div style="margin-left:auto;display:flex;gap:6px;flex-wrap:wrap;">
                 <button class="btn btn-sm" id="btn-toggle-lock">${
                   store.settings?.queue_locked ? "🔒 " + t("list.unlockQueue") : "🔓 " + t("list.lockQueue")
                 }</button>
                 ${
                   totalBytes > 0
                     ? `<button class="btn btn-sm" id="btn-purge-queue-files">🗑 ${escapeHtml(
                         t("list.deleteQueueFiles")
                       )}</button>`
                     : ""
                 }
                 <button class="btn btn-sm btn-danger" id="btn-clear-queue">${escapeHtml(
                   t("list.clearQueue")
                 )}</button>
               </div>`
            : isInbox && store.episodes.length > 0
              ? `<div style="margin-left:auto;display:flex;gap:6px;">
                   <button class="btn btn-sm" id="btn-clear-inbox">✕ ${escapeHtml(
                     t("list.clearInbox")
                   )}</button>
                 </div>`
              : ""
        }
      </div>
    `;
  }

  // Filtres combinables (tous les épisodes, favoris, etc.)
  const showFilters = !isQueue;
  const filterHtml = showFilters
    ? `
    <div class="filter-bar">
      <button class="chip ${store.filterUnplayed ? "active" : ""}" data-flt="unplayed">${escapeHtml(
      t("list.filterUnplayed")
    )}</button>
      <button class="chip ${store.filterInProgress ? "active" : ""}" data-flt="inProgress">${escapeHtml(
      t("list.filterInProgress")
    )}</button>
      <button class="chip ${store.filterDownloaded ? "active" : ""}" data-flt="downloaded">${escapeHtml(
      t("list.filterDownloaded")
    )}</button>
      <button class="chip ${store.filterFavorites ? "active" : ""}" data-flt="favorites">★ ${escapeHtml(
      t("list.filterFavorites")
    )}</button>
      <button class="chip ${store.filterChapters ? "active" : ""}" data-flt="chapters">${escapeHtml(
      t("list.filterChapters")
    )}</button>
      ${
        store.view === "favorites" || store.view === "all" || isInbox
          ? `
        <select id="flt-feed-select" class="chip" style="padding:0 6px;">
          <option value="">${escapeHtml(t("list.filterAllFeeds"))}</option>
          ${store.feeds
            .map(
              (f) =>
                `<option value="${f.id}" ${store.filterFeedId === f.id ? "selected" : ""}>${escapeHtml(
                  f.title
                )}</option>`
            )
            .join("")}
        </select>
        <select id="flt-tag-select" class="chip" style="padding:0 6px;">
          <option value="">${escapeHtml(t("list.filterAllTags"))}</option>
          ${store.tags
            .map(
              (tg) =>
                `<option value="${tg.id}" ${store.filterTagId === tg.id ? "selected" : ""}>${escapeHtml(
                  tg.name
                )}</option>`
            )
            .join("")}
        </select>`
          : ""
      }
    </div>`
    : "";

  // Barre d'actions groupées si sélection multiple (> 1 élément)
  const bulkHtml =
    store.selectedIds.size > 1
      ? `
    <div class="bulk-bar">
      <strong>${escapeHtml(t("list.selectedCount", { count: store.selectedIds.size }))}</strong>
      <button class="btn btn-sm" id="bulk-queue">+ ${escapeHtml(t("actions.addToQueue"))}</button>
      <button class="btn btn-sm" id="bulk-dl">↓ ${escapeHtml(t("actions.download"))}</button>
      <button class="btn btn-sm" id="bulk-played">✓ ${escapeHtml(t("actions.markPlayed"))}</button>
      <button class="btn btn-sm" id="bulk-fav">★ ${escapeHtml(t("actions.addFavorite"))}</button>
      ${
        isQueue
          ? `<button class="btn btn-sm btn-danger" id="bulk-rm-queue">${escapeHtml(
              t("actions.removeFromQueue")
            )}</button>`
          : ""
      }
      ${
        isInbox
          ? `<button class="btn btn-sm btn-danger" id="bulk-rm-inbox">✕ ${escapeHtml(
              t("actions.ignoreInbox")
            )}</button>`
          : ""
      }
    </div>`
      : "";

  const sortArrow = (col: string) =>
    store.sortBy === col ? (store.sortAsc ? " ↑" : " ↓") : "";

  center.innerHTML = `
    <div class="view-header">
      <div class="view-header-top">${headerTopHtml}</div>
      ${filterHtml}
    </div>
    ${bulkHtml}
    <div class="ep-table-header">
      <span></span>
      <span class="ep-col-sort" data-sort="title">${escapeHtml(t("list.colTitle"))}${sortArrow("title")}</span>
      <span class="ep-col-sort" data-sort="feed_title">${escapeHtml(t("list.colPodcast"))}${sortArrow("feed_title")}</span>
      <span class="ep-col-sort" data-sort="pub_date">${escapeHtml(t("list.colDate"))}${sortArrow("pub_date")}</span>
      <span class="ep-col-sort" data-sort="duration_ms">${escapeHtml(t("list.colDuration"))}${sortArrow("duration_ms")}</span>
      <span class="ep-col-sort" data-sort="file_size">${escapeHtml(t("list.colSize"))}${sortArrow("file_size")}</span>
      <span>${escapeHtml(t("list.colStatus"))}</span>
    </div>
    <div class="virtual-viewport" id="ep-virtual-viewport"></div>
  `;

  // Actions d'en-tête Abonnement / Tag / File
  center.querySelector("#btn-feed-mark-all")?.addEventListener("click", async () => {
    if (!activeFeed) return;
    if (activeFeed.unread_count > 10) {
      const ok = await confirmDialog(
        t("list.markAllPlayed"),
        t("list.markAllPlayedConfirm", { count: activeFeed.unread_count })
      );
      if (!ok) return;
    }
    await api.markFeedAllPlayed(activeFeed.id);
    cb.onDataChanged();
  });

  center.querySelector("#btn-feed-refresh")?.addEventListener("click", async () => {
    if (!activeFeed) return;
    try {
      const count = await api.refreshFeed(activeFeed.id);
      showToast(t("toasts.refreshed", { count }), "success");
      cb.onDataChanged();
    } catch (e) {
      showToast(String(e), "error");
    }
  });

  center.querySelector("#btn-feed-tags")?.addEventListener("click", () => {
    if (activeFeed) openAssignFeedTagsDialog(activeFeed, cb.onDataChanged);
  });

  center.querySelector("#btn-feed-settings")?.addEventListener("click", () => {
    if (activeFeed) openFeedSettingsDialog(activeFeed, cb.onDataChanged);
  });

  center.querySelector("#btn-tag-mark-all")?.addEventListener("click", async () => {
    if (!activeTag) return;
    if (activeTag.unread_count > 10) {
      const ok = await confirmDialog(
        t("list.markAllPlayed"),
        t("list.markAllPlayedConfirm", { count: activeTag.unread_count })
      );
      if (!ok) return;
    }
    await api.markTagAllPlayed(activeTag.id);
    cb.onDataChanged();
  });

  center.querySelector("#btn-toggle-lock")?.addEventListener("click", async () => {
    if (!store.settings) return;
    store.settings.queue_locked = !store.settings.queue_locked;
    await api.saveSettings(store.settings);
    cb.onRefreshView();
  });

  center.querySelector("#btn-clear-queue")?.addEventListener("click", async () => {
    await api.clearQueue();
    cb.onDataChanged();
  });

  center.querySelector("#btn-purge-queue-files")?.addEventListener("click", async () => {
    const [count, freed] = await api.purgeQueueDownloads();
    showToast(t("downloads.purgeDone", { count, size: formatBytes(freed) }), "success");
    cb.onDataChanged();
  });

  center.querySelector("#btn-clear-inbox")?.addEventListener("click", async () => {
    const ok = await confirmDialog(
      t("list.clearInbox"),
      t("list.clearInboxConfirm", { count: store.episodes.length })
    );
    if (!ok) return;
    await api.dismissAllInbox();
    store.selectedIds.clear();
    cb.onDataChanged();
  });

  // Filtres chips
  center.querySelectorAll<HTMLElement>("[data-flt]").forEach((chip) => {
    chip.addEventListener("click", () => {
      const f = chip.dataset.flt;
      if (f === "unplayed") store.filterUnplayed = !store.filterUnplayed;
      if (f === "inProgress") store.filterInProgress = !store.filterInProgress;
      if (f === "downloaded") store.filterDownloaded = !store.filterDownloaded;
      if (f === "favorites") store.filterFavorites = !store.filterFavorites;
      if (f === "chapters") store.filterChapters = !store.filterChapters;
      cb.onRefreshView();
    });
  });

  center.querySelector("#flt-feed-select")?.addEventListener("change", (e) => {
    const v = (e.target as HTMLSelectElement).value;
    store.filterFeedId = v ? Number(v) : null;
    cb.onRefreshView();
  });
  center.querySelector("#flt-tag-select")?.addEventListener("change", (e) => {
    const v = (e.target as HTMLSelectElement).value;
    store.filterTagId = v ? Number(v) : null;
    cb.onRefreshView();
  });

  // Tri multi-colonnes
  center.querySelectorAll<HTMLElement>("[data-sort]").forEach((col) => {
    col.addEventListener("click", () => {
      const key = col.dataset.sort!;
      if (store.sortBy === key) {
        store.sortAsc = !store.sortAsc;
      } else {
        store.sortBy = key;
        store.sortAsc = key === "title" || key === "feed_title";
      }
      cb.onRefreshView();
    });
  });

  // Actions groupées
  const selectedArray = () => Array.from(store.selectedIds);
  center.querySelector("#bulk-queue")?.addEventListener("click", async () => {
    await api.enqueueEpisodes(selectedArray());
    cb.onDataChanged();
  });
  center.querySelector("#bulk-dl")?.addEventListener("click", async () => {
    for (const id of selectedArray()) {
      void api.downloadEpisode(id);
    }
  });
  center.querySelector("#bulk-played")?.addEventListener("click", async () => {
    await api.markEpisodesPlayed(selectedArray(), true);
    store.selectedIds.clear();
    cb.onDataChanged();
  });
  center.querySelector("#bulk-fav")?.addEventListener("click", async () => {
    await api.setEpisodesFavorite(selectedArray(), true);
    cb.onDataChanged();
  });
  center.querySelector("#bulk-rm-queue")?.addEventListener("click", async () => {
    await api.removeFromQueue(selectedArray());
    store.selectedIds.clear();
    cb.onDataChanged();
  });
  center.querySelector("#bulk-rm-inbox")?.addEventListener("click", async () => {
    await api.setInboxState(selectedArray(), 2);
    store.selectedIds.clear();
    cb.onDataChanged();
  });

  // Virtual Scrolling de la liste d'épisodes
  const viewport = center.querySelector("#ep-virtual-viewport") as HTMLElement;
  if (store.episodes.length === 0) {
    viewport.innerHTML = `
      <div class="empty-state">
        <div style="font-size:32px;opacity:.45;">🎧</div>
        <strong>${escapeHtml(t("list.emptyTitle"))}</strong>
        <p style="max-width:420px;">${escapeHtml(t("list.emptyHint"))}</p>
      </div>
    `;
    activeVList = null;
    return;
  }

  const rowHeight = store.settings?.density === "comfortable" ? 56 : 46;
  activeVList = new VirtualList<Episode>(viewport, rowHeight, (ep, index) =>
    buildEpisodeRow(ep, index, isQueue, cb)
  );
  activeVList.setItems(store.episodes);
}

/** Met à jour uniquement le rendu visible du VirtualList (ex: changement sélection ou lecture). */
export function refreshVirtualListRows(): void {
  activeVList?.forceRender();
}

function buildEpisodeRow(
  ep: Episode,
  index: number,
  isQueue: boolean,
  cb: EpisodeListCallbacks
): HTMLElement {
  const row = document.createElement("div");
  const isPlaying = store.audio.episode_id === ep.id;
  const isSelected = store.selectedIds.has(ep.id);
  row.className = `ep-row${isSelected ? " selected" : ""}${isPlaying ? " playing" : ""}${
    ep.is_played ? " played" : ""
  }`;
  row.dataset.id = String(ep.id);

  // Drag & Drop dans la file d'attente
  if (isQueue && !store.settings?.queue_locked) {
    row.draggable = true;
    row.addEventListener("dragstart", (e) => {
      dragEpisodeId = ep.id;
      e.dataTransfer?.setData("text/plain", String(ep.id));
    });
    row.addEventListener("dragover", (e) => {
      e.preventDefault();
      const rect = row.getBoundingClientRect();
      const topHalf = e.clientY < rect.top + rect.height / 2;
      row.classList.toggle("drag-over-top", topHalf);
      row.classList.toggle("drag-over-bottom", !topHalf);
    });
    row.addEventListener("dragleave", () => {
      row.classList.remove("drag-over-top", "drag-over-bottom");
    });
    row.addEventListener("drop", async (e) => {
      e.preventDefault();
      row.classList.remove("drag-over-top", "drag-over-bottom");
      if (dragEpisodeId !== null && dragEpisodeId !== ep.id) {
        try {
          await api.reorderQueue(dragEpisodeId, index);
          cb.onDataChanged();
        } catch (err) {
          showToast(String(err), "error");
        }
      }
      dragEpisodeId = null;
    });
  }

  const cover = resolveCoverUrl(ep.feed_image_url);
  const progressPct =
    ep.duration_ms > 0 && ep.position_ms > 0
      ? Math.min(100, Math.round((ep.position_ms / ep.duration_ms) * 100))
      : 0;

  let statusBadge = `<span class="status-pill">${escapeHtml(t("list.statusNew"))}</span>`;
  if (ep.is_played) {
    statusBadge = `<span class="status-pill">✓ ${escapeHtml(t("list.statusPlayed"))}</span>`;
  } else if (ep.local_file_path) {
    statusBadge = `<span class="status-pill offline">↓ ${escapeHtml(t("list.statusOffline"))}</span>`;
  } else if (progressPct > 0) {
    statusBadge = `<span class="status-pill in-progress">${progressPct}%</span>`;
  }

  row.innerHTML = `
    <div class="ep-cover-cell">
      ${isQueue ? `<span class="drag-handle" title="Drag">⋮⋮</span>` : ""}
      ${
        cover
          ? `<img class="ep-cover" src="${escapeHtml(cover)}" alt="" loading="lazy" />`
          : `<div class="ep-cover" style="display:grid;place-items:center;">🎙</div>`
      }
    </div>
    <div class="ep-title-cell">
      <span class="ep-title-text">${ep.is_favorite ? "★ " : ""}${escapeHtml(ep.title)}</span>
      ${
        progressPct > 0 && !ep.is_played
          ? `<div class="ep-mini-progress"><div class="ep-mini-progress-fill" style="width:${progressPct}%"></div></div>`
          : ""
      }
      <div class="ep-hover-actions">
        <button class="icon-btn" data-act="play" title="${escapeHtml(t("actions.playNow"))}" style="width:24px;height:24px;">▶</button>
        <button class="icon-btn" data-act="queue" title="${escapeHtml(t("actions.addToQueue"))}" style="width:24px;height:24px;">+</button>
        <button class="icon-btn" data-act="dl" title="${escapeHtml(t("actions.download"))}" style="width:24px;height:24px;">↓</button>
        <button class="icon-btn" data-act="played" title="${escapeHtml(
          ep.is_played ? t("actions.markUnplayed") : t("actions.markPlayed")
        )}" style="width:24px;height:24px;">✓</button>
        <button class="icon-btn" data-act="fav" title="${escapeHtml(
          ep.is_favorite ? t("actions.removeFavorite") : t("actions.addFavorite")
        )}" style="width:24px;height:24px;color:${ep.is_favorite ? "var(--star)" : ""};">★</button>
        ${
          ep.local_file_path
            ? `<button class="icon-btn" data-act="del-file" title="${escapeHtml(
                t("actions.deleteLocalFile")
              )}" style="width:24px;height:24px;color:var(--danger);">🗑</button>`
            : ""
        }
        ${
          isQueue
            ? `<button class="icon-btn" data-act="rm-queue" title="${escapeHtml(
                t("actions.removeFromQueue")
              )}" style="width:24px;height:24px;color:var(--danger);">✕</button>`
            : ""
        }
        ${
          store.view === "inbox"
            ? `<button class="icon-btn" data-act="dismiss-inbox" title="${escapeHtml(
                t("actions.ignoreInbox")
              )}" style="width:24px;height:24px;color:var(--danger);">✕</button>`
            : ""
        }
      </div>
    </div>
    <div class="ep-cell-muted">${escapeHtml(ep.feed_title)}</div>
    <div class="ep-cell-muted tabular">${escapeHtml(formatDate(ep.pub_date))}</div>
    <div class="ep-cell-muted tabular">${escapeHtml(formatDurationMs(ep.duration_ms))}</div>
    <div class="ep-cell-muted tabular">${escapeHtml(formatBytes(ep.file_size))}</div>
    <div>${statusBadge}</div>
  `;

  // Actions au survol (:hover)
  row.querySelectorAll<HTMLElement>("[data-act]").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      const act = btn.dataset.act;
      try {
        if (act === "play") await api.playEpisode(ep.id);
        if (act === "queue") {
          await api.enqueueEpisodes([ep.id]);
          cb.onDataChanged();
        }
        if (act === "dl") void api.downloadEpisode(ep.id);
        if (act === "played") {
          await api.markEpisodesPlayed([ep.id], !ep.is_played);
          cb.onDataChanged();
        }
        if (act === "fav") {
          await api.setEpisodesFavorite([ep.id], !ep.is_favorite);
          cb.onDataChanged();
        }
        if (act === "dismiss-inbox") {
          await api.setInboxState([ep.id], 2);
          cb.onDataChanged();
        }
        if (act === "rm-queue") {
          await api.removeFromQueue([ep.id]);
          cb.onDataChanged();
        }
        if (act === "del-file") {
          await api.deleteLocalFile(ep.id);
          cb.onDataChanged();
        }
      } catch (err) {
        showToast(String(err), "error");
      }
    });
  });

  // Clic simple (inspecteur + multi-sélection Ctrl/Shift) & double-clic (lire)
  row.addEventListener("click", (e) => {
    if (e.shiftKey && store.lastSelectedIndex >= 0) {
      const lo = Math.min(store.lastSelectedIndex, index);
      const hi = Math.max(store.lastSelectedIndex, index);
      for (let i = lo; i <= hi; i++) {
        store.selectedIds.add(store.episodes[i].id);
      }
    } else if (e.ctrlKey || e.metaKey) {
      if (store.selectedIds.has(ep.id)) store.selectedIds.delete(ep.id);
      else store.selectedIds.add(ep.id);
      store.lastSelectedIndex = index;
    } else {
      store.selectedIds.clear();
      store.selectedIds.add(ep.id);
      store.lastSelectedIndex = index;
    }
    store.inspectedEpisodeId = ep.id;
    cb.onSelectInspect(ep.id);
    // Met à jour la barre d'actions groupées si nécessaire
    if (store.selectedIds.size > 1 || document.querySelector(".bulk-bar")) {
      renderEpisodeListView(cb);
    } else {
      refreshVirtualListRows();
    }
  });

  row.addEventListener("dblclick", async () => {
    try {
      await api.playEpisode(ep.id);
    } catch (err) {
      showToast(String(err), "error");
    }
  });

  // Menu contextuel complet au clic droit
  row.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    if (!store.selectedIds.has(ep.id)) {
      store.selectedIds.clear();
      store.selectedIds.add(ep.id);
      refreshVirtualListRows();
    }
    const targetIds = store.selectedIds.size > 1 ? Array.from(store.selectedIds) : [ep.id];

    showContextMenu(e.clientX, e.clientY, [
      { label: `▶  ${t("actions.playNow")}`, onClick: () => void api.playEpisode(ep.id) },
      {
        label: `⏭  ${t("actions.playNext")}`,
        onClick: async () => {
          await api.enqueueEpisodes(targetIds, "after_current");
          cb.onDataChanged();
        },
      },
      {
        label: `⤒  ${t("actions.addToQueueTop")}`,
        onClick: async () => {
          await api.enqueueEpisodes(targetIds, "top");
          cb.onDataChanged();
        },
      },
      {
        label: `⤓  ${t("actions.addToQueueBottom")}`,
        onClick: async () => {
          await api.enqueueEpisodes(targetIds, "bottom");
          cb.onDataChanged();
        },
      },
      ...(ep.queue_position !== null
        ? [
            {
              label: `✕  ${t("actions.removeFromQueue")}`,
              onClick: async () => {
                await api.removeFromQueue(targetIds);
                cb.onDataChanged();
              },
            },
          ]
        : []),
      { separator: true },
      {
        label: `↓  ${t("actions.download")}`,
        onClick: () => {
          for (const id of targetIds) void api.downloadEpisode(id);
        },
      },
      {
        label: ep.is_played ? `↺  ${t("actions.markUnplayed")}` : `✓  ${t("actions.markPlayed")}`,
        onClick: async () => {
          await api.markEpisodesPlayed(targetIds, !ep.is_played);
          cb.onDataChanged();
        },
      },
      {
        label: ep.is_favorite ? `☆  ${t("actions.removeFavorite")}` : `★  ${t("actions.addFavorite")}`,
        onClick: async () => {
          await api.setEpisodesFavorite(targetIds, !ep.is_favorite);
          cb.onDataChanged();
        },
      },
      ...(store.view === "inbox"
        ? [
            { separator: true },
            {
              label: `✕  ${t("actions.ignoreInbox")}`,
              onClick: async () => {
                await api.setInboxState(targetIds, 2);
                store.selectedIds.clear();
                cb.onDataChanged();
              },
            },
            {
              label: `⊘  ${t("actions.dismissFeedFromInbox", { feed: ep.feed_title })}`,
              onClick: async () => {
                await api.dismissFeedFromInbox(ep.feed_id);
                store.selectedIds.clear();
                cb.onDataChanged();
              },
            },
            {
              label: `🗑  ${t("actions.unsubscribeFeed", { feed: ep.feed_title })}`,
              danger: true,
              onClick: async () => {
                const ok = await confirmDialog(
                  t("list.unsubscribe"),
                  t("list.unsubscribeConfirm", { title: ep.feed_title }),
                  true
                );
                if (!ok) return;
                await api.deleteFeed(ep.feed_id);
                store.selectedIds.clear();
                cb.onDataChanged();
              },
            },
          ]
        : []),
      { separator: true },
      {
        label: `🔗 ${t("actions.copyTimestampLink")}`,
        onClick: async () => {
          const sec = Math.floor(ep.position_ms / 1000);
          await navigator.clipboard.writeText(`${ep.audio_url}#t=${sec}`);
          showToast(t("inspector.copyShareLink"), "success");
        },
      },
      ...(ep.local_file_path
        ? [
            {
              label: `📁 ${t("actions.openFolder")}`,
              onClick: () => void api.openEpisodeFolder(ep.id),
            },
            {
              label: `🗑 ${t("actions.deleteLocalFile")}`,
              danger: true,
              onClick: async () => {
                try {
                  await api.deleteLocalFile(ep.id);
                  cb.onDataChanged();
                } catch (err) {
                  showToast(String(err), "error");
                }
              },
            },
          ]
        : []),
    ]);
  });

  return row;
}
