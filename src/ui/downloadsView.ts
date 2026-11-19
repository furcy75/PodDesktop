/**
 * Vue Téléchargements & Santé du stockage :
 * - Espace disque (media, covers, cache WebView .runtime, base SQLite)
 * - Purge en 1 clic des fichiers locaux d'épisodes déjà lus
 * - File des téléchargements actifs avec progression temps réel, pause/reprise et annulation
 * - Répartition de l'espace disque par podcast + liste des épisodes hors-ligne
 */
import { api } from "../api";
import { formatBytes, formatDurationMs, t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";

export interface DownloadsCallbacks {
  onDataChanged: () => void;
}

export async function renderDownloadsView(cb: DownloadsCallbacks): Promise<void> {
  const center = document.getElementById("center-pane");
  if (!center) return;

  const [health, tasks] = await Promise.all([
    api.getStorageHealth(),
    api.listDownloads(),
  ]);
  for (const task of tasks) {
    store.downloads.set(task.episode_id, task);
  }
  const activeList = Array.from(store.downloads.values());
  const maxFeedBytes = Math.max(1, ...health.by_feed.map((f) => f.bytes));

  center.innerHTML = `
    <div class="view-header">
      <div class="view-header-top">
        <div class="view-title">↓ ${escapeHtml(t("downloads.title"))}</div>
        <div style="margin-left:auto;display:flex;gap:8px;flex-wrap:wrap;">
          <button class="btn btn-primary" id="dl-purge-played">🧹 ${escapeHtml(
            t("downloads.purgePlayedBtn", { count: health.played_downloaded_count })
          )}</button>
          <button class="btn btn-danger" id="dl-purge-non-queue">🗑 ${escapeHtml(
            t("downloads.purgeNonQueueBtn")
          )}</button>
          <button class="btn" id="dl-clear-cache">${escapeHtml(
            t("downloads.clearCacheBtn")
          )}</button>
        </div>
      </div>
    </div>

    <div class="page-scroll">
      <!-- Santé du stockage -->
      <div class="cards-grid" style="margin-top:0;margin-bottom:16px;">
        <div class="card">
          <div style="color:var(--text-secondary);font-size:12px;">${escapeHtml(
            t("downloads.mediaDisk")
          )}</div>
          <div class="stat-value tabular">${escapeHtml(formatBytes(health.total_media_bytes))}</div>
          <div style="font-size:11.5px;color:var(--text-muted);">${health.downloaded_count} ${escapeHtml(
    t("downloads.offlineEpisodes")
  )}</div>
        </div>
        <div class="card">
          <div style="color:var(--text-secondary);font-size:12px;">${escapeHtml(
            t("downloads.coversDisk")
          )}</div>
          <div class="stat-value tabular">${escapeHtml(formatBytes(health.total_covers_bytes))}</div>
        </div>
        <div class="card">
          <div style="color:var(--text-secondary);font-size:12px;">${escapeHtml(
            t("downloads.cacheDisk")
          )}</div>
          <div class="stat-value tabular">${escapeHtml(formatBytes(health.total_runtime_bytes))}</div>
        </div>
        <div class="card">
          <div style="color:var(--text-secondary);font-size:12px;">${escapeHtml(
            t("downloads.dbDisk")
          )}</div>
          <div class="stat-value tabular">${escapeHtml(formatBytes(health.db_size_bytes))}</div>
        </div>
      </div>

      <!-- Téléchargements en cours -->
      <div class="card" style="margin-bottom:16px;">
        <div style="font-weight:700;">${escapeHtml(t("downloads.activeDownloads"))}</div>
        ${
          activeList.length === 0
            ? `<p style="color:var(--text-muted);">${escapeHtml(t("downloads.noActiveDownloads"))}</p>`
            : activeList
                .map((task) => {
                  const pct =
                    task.total_bytes && task.total_bytes > 0
                      ? Math.min(100, Math.round((task.downloaded_bytes / task.total_bytes) * 100))
                      : 0;
                  return `
                <div style="padding:8px 0;border-bottom:1px solid var(--border-subtle);display:flex;flex-direction:column;gap:6px;">
                  <div style="display:flex;justify-content:space-between;align-items:center;gap:10px;">
                    <div style="min-width:0;">
                      <div style="font-weight:600;">${escapeHtml(task.title)}</div>
                      <div style="font-size:11.5px;color:var(--text-secondary);">${escapeHtml(
                        task.feed_title
                      )} · ${escapeHtml(formatBytes(task.downloaded_bytes))}${
                    task.total_bytes ? ` / ${formatBytes(task.total_bytes)}` : ""
                  } · ${escapeHtml(task.status)}</div>
                    </div>
                    <div style="display:flex;gap:5px;">
                      ${
                        task.status === "downloading"
                          ? `<button class="btn btn-sm" data-dl-pause="${task.episode_id}">${escapeHtml(
                              t("downloads.pause")
                            )}</button>`
                          : task.status === "paused" || task.status === "failed"
                            ? `<button class="btn btn-sm btn-primary" data-dl-resume="${task.episode_id}">${escapeHtml(
                                t("downloads.resume")
                              )}</button>`
                            : ""
                      }
                      ${
                        task.status === "downloading" || task.status === "paused"
                          ? `<button class="btn btn-sm btn-danger" data-dl-cancel="${task.episode_id}">${escapeHtml(
                              t("downloads.cancel")
                            )}</button>`
                          : ""
                      }
                    </div>
                  </div>
                  <div class="bar-track"><div class="bar-fill" style="width:${pct}%"></div></div>
                </div>`;
                })
                .join("")
        }
      </div>

      <!-- Espace disque par podcast -->
      <div class="card" style="margin-bottom:16px;">
        <div style="font-weight:700;">${escapeHtml(t("downloads.storageByPodcast"))}</div>
        ${
          health.by_feed.length === 0
            ? `<p style="color:var(--text-muted);">—</p>`
            : health.by_feed
                .map(
                  (f) => `
              <div class="bar-row">
                <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(
                  f.feed_title
                )} (${f.episode_count})</span>
                <div class="bar-track"><div class="bar-fill" style="width:${Math.round(
                  (f.bytes / maxFeedBytes) * 100
                )}%"></div></div>
                <span class="tabular" style="text-align:right;color:var(--text-secondary);">${escapeHtml(
                  formatBytes(f.bytes)
                )}</span>
              </div>`
                )
                .join("")
        }
      </div>

      <!-- Liste des épisodes hors-ligne -->
      <div class="card">
        <div style="font-weight:700;">${escapeHtml(t("downloads.offlineEpisodes"))} (${
    store.episodes.length
  })</div>
        ${store.episodes
          .map(
            (ep) => `
          <div style="display:flex;align-items:center;gap:10px;padding:7px 0;border-bottom:1px solid var(--border-subtle);">
            <button class="icon-btn" data-off-play="${ep.id}" style="width:26px;height:26px;">▶</button>
            <div style="flex:1;min-width:0;">
              <div style="font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(
                ep.title
              )}</div>
              <div style="font-size:11.5px;color:var(--text-secondary);">${escapeHtml(
                ep.feed_title
              )} · ${escapeHtml(formatDurationMs(ep.duration_ms))} · ${escapeHtml(
              formatBytes(ep.file_size)
            )}</div>
            </div>
            <button class="btn btn-sm" data-off-folder="${ep.id}">📁</button>
            <button class="btn btn-sm btn-danger" data-off-del="${ep.id}">🗑</button>
          </div>`
          )
          .join("")}
      </div>
    </div>
  `;

  center.querySelector("#dl-purge-played")?.addEventListener("click", async () => {
    const [count, freed] = await api.purgePlayedDownloads();
    showToast(t("downloads.purgeDone", { count, size: formatBytes(freed) }), "success");
    cb.onDataChanged();
  });

  center.querySelector("#dl-purge-non-queue")?.addEventListener("click", async () => {
    const [count, freed] = await api.purgeNonQueueDownloads();
    showToast(t("downloads.purgeDone", { count, size: formatBytes(freed) }), "success");
    cb.onDataChanged();
  });

  center.querySelector("#dl-clear-cache")?.addEventListener("click", async () => {
    const freed = await api.clearWebviewCache();
    showToast(t("toasts.cacheCleared", { size: formatBytes(freed) }), "success");
    void renderDownloadsView(cb);
  });

  center.querySelectorAll<HTMLElement>("[data-dl-pause]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      await api.pauseDownload(Number(btn.dataset.dlPause));
      void renderDownloadsView(cb);
    });
  });
  center.querySelectorAll<HTMLElement>("[data-dl-resume]").forEach((btn) => {
    btn.addEventListener("click", () => {
      void api.downloadEpisode(Number(btn.dataset.dlResume));
    });
  });
  center.querySelectorAll<HTMLElement>("[data-dl-cancel]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      await api.cancelDownload(Number(btn.dataset.dlCancel));
      void renderDownloadsView(cb);
    });
  });
  center.querySelectorAll<HTMLElement>("[data-off-play]").forEach((btn) => {
    btn.addEventListener("click", () => void api.playEpisode(Number(btn.dataset.offPlay)));
  });
  center.querySelectorAll<HTMLElement>("[data-off-folder]").forEach((btn) => {
    btn.addEventListener("click", () => void api.openEpisodeFolder(Number(btn.dataset.offFolder)));
  });
  center.querySelectorAll<HTMLElement>("[data-off-del]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      try {
        await api.deleteLocalFile(Number(btn.dataset.offDel));
        cb.onDataChanged();
      } catch (e) {
        showToast(String(e), "error");
      }
    });
  });
}
