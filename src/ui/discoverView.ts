/**
 * Vue Découvrir : Recherche d'annuaires (iTunes Search & Podcast Index),
 * ajout direct par URL RSS ou lien Apple Podcasts (avec support flux privé HTTP Basic),
 * mode Aperçu (Preview) avant abonnement, et ajout de dossiers locaux virtuels.
 */
import { api } from "../api";
import { formatDate, formatDurationMs, t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";
import type { DiscoveryPodcast, FeedPreview } from "../types";

export interface DiscoverCallbacks {
  onSubscribed: (feedId: number) => void;
}

let lastQuery = "";
let lastResults: DiscoveryPodcast[] = [];
let activePreview: FeedPreview | null = null;

export function renderDiscoverView(cb: DiscoverCallbacks): void {
  const center = document.getElementById("center-pane");
  if (!center) return;

  const subscribedUrls = new Set(store.feeds.map((f) => f.feed_url.toLowerCase()));

  center.innerHTML = `
    <div class="view-header">
      <div class="view-header-top">
        <div class="view-title">✦ ${escapeHtml(t("discover.title"))}</div>
      </div>
    </div>
    <div class="page-scroll">
      <!-- Recherche annuaires -->
      <div class="card" style="margin-bottom:14px;">
        <div style="display:flex;gap:8px;">
          <input type="text" id="disc-query" value="${escapeHtml(lastQuery)}"
                 placeholder="${escapeHtml(t("discover.searchPlaceholder"))}"
                 style="flex:1;height:34px;padding:0 12px;border-radius:6px;border:1px solid var(--border-strong);background:var(--bg-elevated);" />
          <button class="btn btn-primary" id="disc-search-btn" style="height:34px;">${escapeHtml(
            t("discover.searchBtn")
          )}</button>
        </div>
      </div>

      <!-- Ajout direct par URL RSS / Apple Podcasts + flux privé -->
      <div class="card" style="margin-bottom:14px;">
        <div style="font-weight:600;">${escapeHtml(t("discover.directUrlLabel"))}</div>
        <div style="display:flex;gap:8px;flex-wrap:wrap;">
          <input type="text" id="disc-url" placeholder="${escapeHtml(
            t("discover.directUrlPlaceholder")
          )}"
                 style="flex:1;min-width:260px;height:32px;padding:0 10px;border-radius:6px;border:1px solid var(--border-strong);background:var(--bg-elevated);" />
          <button class="btn" id="disc-url-preview">${escapeHtml(t("discover.previewBtn"))}</button>
          <button class="btn btn-primary" id="disc-url-sub">${escapeHtml(
            t("discover.subscribeBtn")
          )}</button>
        </div>
        <label style="display:inline-flex;align-items:center;gap:6px;font-size:12px;color:var(--text-secondary);cursor:pointer;">
          <input type="checkbox" id="disc-private-toggle" />
          ${escapeHtml(t("discover.privateFeedToggle"))}
        </label>
        <div id="disc-private-fields" class="hidden" style="display:flex;gap:8px;flex-wrap:wrap;">
          <input type="text" id="disc-user" placeholder="${escapeHtml(t("discover.username"))}"
                 style="height:30px;padding:0 9px;border-radius:6px;border:1px solid var(--border-strong);background:var(--bg-elevated);" />
          <input type="password" id="disc-pass" placeholder="${escapeHtml(t("discover.password"))}"
                 style="height:30px;padding:0 9px;border-radius:6px;border:1px solid var(--border-strong);background:var(--bg-elevated);" />
        </div>
      </div>

      <!-- Dossier local virtuel -->
      <div class="card" style="margin-bottom:16px;flex-direction:row;align-items:center;justify-content:space-between;flex-wrap:wrap;">
        <div>
          <div style="font-weight:600;">📁 ${escapeHtml(t("discover.localFolderCardTitle"))}</div>
          <div style="font-size:12px;color:var(--text-secondary);">${escapeHtml(
            t("discover.localFolderCardDesc")
          )}</div>
        </div>
        <button class="btn" id="disc-local-btn">${escapeHtml(t("discover.chooseFolderBtn"))}</button>
      </div>

      <!-- Zone Aperçu (Preview) avant abonnement -->
      ${
        activePreview
          ? `
        <div class="card" style="margin-bottom:16px;border-color:var(--accent);">
          <div style="display:flex;gap:14px;align-items:flex-start;">
            ${
              activePreview.image_url
                ? `<img src="${escapeHtml(activePreview.image_url)}" style="width:72px;height:72px;border-radius:10px;object-fit:cover;" />`
                : ""
            }
            <div style="flex:1;">
              <div style="font-size:16px;font-weight:700;">${escapeHtml(activePreview.title)}</div>
              <div style="font-size:12px;color:var(--text-secondary);">${escapeHtml(
                activePreview.author || ""
              )}</div>
              <p style="font-size:12px;color:var(--text-secondary);margin-top:6px;">${escapeHtml(
                (activePreview.description || "").replace(/<[^>]+>/g, "").slice(0, 260)
              )}</p>
            </div>
            <div style="display:flex;gap:6px;">
              <button class="btn btn-primary" id="preview-sub-btn">${escapeHtml(
                t("discover.subscribeBtn")
              )}</button>
              <button class="btn" id="preview-close-btn">✕</button>
            </div>
          </div>
          <div style="font-weight:600;margin-top:8px;">${escapeHtml(
            t("discover.previewEpisodes")
          )}</div>
          <div style="max-height:220px;overflow-y:auto;">
            ${activePreview.episodes
              .map(
                (e) => `
              <div style="display:flex;justify-content:space-between;padding:5px 0;border-bottom:1px solid var(--border-subtle);gap:12px;">
                <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(
                  e.title
                )}</span>
                <span class="tabular" style="color:var(--text-muted);font-size:11.5px;flex-shrink:0;">
                  ${escapeHtml(formatDate(e.pub_date))} · ${escapeHtml(formatDurationMs(e.duration_ms))}
                </span>
              </div>`
              )
              .join("")}
          </div>
        </div>`
          : ""
      }

      <!-- Résultats d'annuaire -->
      ${
        lastResults.length > 0
          ? `<div class="cards-grid">
              ${lastResults
                .map((p, idx) => {
                  const isSub = subscribedUrls.has(p.feed_url.toLowerCase());
                  return `
                <div class="card">
                  <div style="display:flex;gap:10px;align-items:center;">
                    ${
                      p.image_url
                        ? `<img src="${escapeHtml(p.image_url)}" style="width:48px;height:48px;border-radius:8px;object-fit:cover;" loading="lazy" />`
                        : `<div style="width:48px;height:48px;border-radius:8px;background:var(--bg-elevated);display:grid;place-items:center;">🎙</div>`
                    }
                    <div style="min-width:0;flex:1;">
                      <div style="font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(
                        p.title
                      )}</div>
                      <div style="font-size:11.5px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(
                        p.author
                      )}</div>
                    </div>
                  </div>
                  <div style="display:flex;gap:6px;margin-top:auto;">
                    <button class="btn btn-sm" data-prev-idx="${idx}">${escapeHtml(
                    t("discover.previewBtn")
                  )}</button>
                    <button class="btn btn-sm ${isSub ? "" : "btn-primary"}" data-sub-idx="${idx}" ${
                    isSub ? "disabled" : ""
                  }>${escapeHtml(
                    isSub ? t("discover.subscribedBadge") : t("discover.subscribeBtn")
                  )}</button>
                  </div>
                </div>`;
                })
                .join("")}
            </div>`
          : `<div class="empty-state"><p>${escapeHtml(t("discover.noResults"))}</p></div>`
      }
    </div>
  `;

  const queryInput = center.querySelector("#disc-query") as HTMLInputElement;
  const runSearch = async () => {
    lastQuery = queryInput.value.trim();
    if (!lastQuery) return;
    try {
      lastResults = await api.searchDirectory(lastQuery);
      renderDiscoverView(cb);
    } catch (e) {
      showToast(String(e), "error");
    }
  };
  center.querySelector("#disc-search-btn")?.addEventListener("click", () => void runSearch());
  queryInput?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void runSearch();
  });

  const privToggle = center.querySelector("#disc-private-toggle") as HTMLInputElement;
  const privFields = center.querySelector("#disc-private-fields") as HTMLElement;
  privToggle?.addEventListener("change", () => {
    privFields.classList.toggle("hidden", !privToggle.checked);
  });

  const getDirectCreds = () => ({
    url: (center.querySelector("#disc-url") as HTMLInputElement).value.trim(),
    user: privToggle.checked
      ? (center.querySelector("#disc-user") as HTMLInputElement).value.trim()
      : undefined,
    pass: privToggle.checked
      ? (center.querySelector("#disc-pass") as HTMLInputElement).value
      : undefined,
  });

  center.querySelector("#disc-url-preview")?.addEventListener("click", async () => {
    const { url, user, pass } = getDirectCreds();
    if (!url) return;
    try {
      activePreview = await api.previewFeed(url, user, pass);
      renderDiscoverView(cb);
    } catch (e) {
      showToast(String(e), "error");
    }
  });

  center.querySelector("#disc-url-sub")?.addEventListener("click", async () => {
    const { url, user, pass } = getDirectCreds();
    if (!url) return;
    try {
      const id = await api.subscribeFeed(url, user, pass);
      showToast(t("toasts.subscribed"), "success");
      activePreview = null;
      cb.onSubscribed(id);
    } catch (e) {
      showToast(String(e), "error");
    }
  });

  center.querySelector("#disc-local-btn")?.addEventListener("click", async () => {
    const dir = await api.pickFolder(t("nav.addLocalFolder"));
    if (!dir) return;
    try {
      const id = await api.addLocalFolder(dir);
      showToast(t("toasts.localFolderAdded"), "success");
      cb.onSubscribed(id);
    } catch (e) {
      showToast(String(e), "error");
    }
  });

  center.querySelector("#preview-close-btn")?.addEventListener("click", () => {
    activePreview = null;
    renderDiscoverView(cb);
  });

  center.querySelector("#preview-sub-btn")?.addEventListener("click", async () => {
    if (!activePreview) return;
    try {
      const id = await api.subscribeFeed(activePreview.feed_url);
      showToast(t("toasts.subscribed"), "success");
      activePreview = null;
      cb.onSubscribed(id);
    } catch (e) {
      showToast(String(e), "error");
    }
  });

  center.querySelectorAll<HTMLElement>("[data-prev-idx]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const item = lastResults[Number(btn.dataset.prevIdx)];
      if (!item) return;
      try {
        activePreview = await api.previewFeed(item.feed_url);
        renderDiscoverView(cb);
      } catch (e) {
        showToast(String(e), "error");
      }
    });
  });

  center.querySelectorAll<HTMLElement>("[data-sub-idx]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const item = lastResults[Number(btn.dataset.subIdx)];
      if (!item) return;
      try {
        const id = await api.subscribeFeed(item.feed_url);
        showToast(t("toasts.subscribed"), "success");
        cb.onSubscribed(id);
      } catch (e) {
        showToast(String(e), "error");
      }
    });
  });
}
