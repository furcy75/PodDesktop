/**
 * Panneau latéral gauche (Navigation 1 clic vers les 9 vues, Tags/Dossiers & Abonnements).
 */
import { api, resolveCoverUrl } from "../api";
import { t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";
import type { Feed, Tag, ViewKind } from "../types";
import { showContextMenu } from "./contextMenu";
import {
  confirmDialog,
  openAssignFeedTagsDialog,
  openFeedSettingsDialog,
  openTagDialog,
} from "./dialogs";
import { icons } from "./icons";

export interface SidebarCallbacks {
  onNavigate: (view: ViewKind, feedId?: number | null, tagId?: number | null) => void;
  onDataChanged: () => void;
}

export function renderSidebar(cb: SidebarCallbacks): void {
  const el = document.getElementById("sidebar");
  if (!el) return;

  const navItems: { view: ViewKind; icon: string; label: string; count?: number }[] = [
    { view: "queue", icon: icons.queue(15), label: t("nav.queue"), count: store.counts.queue },
    { view: "inbox", icon: icons.inbox(15), label: t("nav.inbox"), count: store.counts.inbox },
    { view: "all", icon: icons.allEpisodes(15), label: t("nav.all"), count: store.counts.all_unplayed },
    { view: "downloads", icon: icons.download(15), label: t("nav.downloads"), count: store.counts.downloads },
    { view: "favorites", icon: icons.star(15, false), label: t("nav.favorites"), count: store.counts.favorites },
    { view: "history", icon: icons.history(15), label: t("nav.history"), count: store.counts.history },
    { view: "discover", icon: icons.discover(15), label: t("nav.discover") },
    { view: "stats", icon: icons.stats(15), label: t("nav.stats") },
  ];

  const mainHtml = navItems
    .map(
      (n) => `
      <div class="nav-item ${store.view === n.view ? "active" : ""}" data-view="${n.view}">
        <span>${n.icon}</span>
        <span class="nav-label">${escapeHtml(n.label)}</span>
        ${n.count && n.count > 0 ? `<span class="nav-badge">${n.count}</span>` : ""}
      </div>
    `
    )
    .join("");

  const tagsHtml = store.tags
    .map(
      (tag) => `
      <div class="nav-item ${store.view === "tag" && store.activeTagId === tag.id ? "active" : ""}"
           data-tag-id="${tag.id}">
        <span class="tag-dot" style="background:${escapeHtml(tag.color)}"></span>
        <span class="nav-label">${escapeHtml(tag.name)}</span>
        ${tag.unread_count > 0 ? `<span class="nav-badge">${tag.unread_count}</span>` : ""}
      </div>
    `
    )
    .join("");

  const feedsHtml = store.feeds
    .map((f) => {
      const cover = resolveCoverUrl(f.local_cover_path || f.image_url);
      return `
      <div class="nav-item ${store.view === "feed" && store.activeFeedId === f.id ? "active" : ""}"
           data-feed-id="${f.id}" title="${escapeHtml(f.title)}">
        ${
          cover
            ? `<img class="feed-thumb" src="${escapeHtml(cover)}" alt="" loading="lazy" />`
            : `<span class="feed-thumb" style="display:grid;place-items:center;font-size:11px;">${
                f.is_local_folder ? "📁" : "🎙"
              }</span>`
        }
        <span class="nav-label">${escapeHtml(f.title)}</span>
        ${f.unread_count > 0 ? `<span class="nav-badge">${f.unread_count}</span>` : ""}
      </div>
    `;
    })
    .join("");

  el.innerHTML = `
    <div class="sidebar-scroll">
      <div class="sidebar-section"><span>${escapeHtml(t("nav.library"))}</span></div>
      ${mainHtml}

      <div class="sidebar-section">
        <span>${escapeHtml(t("nav.tagsAndFolders"))}</span>
        <button class="icon-btn" id="btn-new-tag" title="${escapeHtml(t("nav.addTag"))}" style="width:24px;height:24px;">${icons.plus(14)}</button>
      </div>
      ${tagsHtml}

      <div class="sidebar-section">
        <span>${escapeHtml(t("nav.subscriptions"))} (${store.feeds.length})</span>
        <button class="icon-btn" id="btn-add-local" title="${escapeHtml(t("nav.addLocalFolder"))}" style="width:24px;height:24px;">${icons.folder(14)}</button>
      </div>
      ${feedsHtml}
    </div>
    <div class="sidebar-footer">
      <div class="nav-item ${store.view === "settings" ? "active" : ""}" data-view="settings">
        <span>${icons.settings(16)}</span>
        <span class="nav-label">${escapeHtml(t("app.settings"))}</span>
      </div>
    </div>
  `;

  // Navigation vues principales
  el.querySelectorAll<HTMLElement>("[data-view]").forEach((node) => {
    node.addEventListener("click", () => {
      cb.onNavigate(node.dataset.view as ViewKind, null, null);
    });
  });

  // Navigation & clic droit Tags/Dossiers
  el.querySelectorAll<HTMLElement>("[data-tag-id]").forEach((node) => {
    const tagId = Number(node.dataset.tagId);
    const tag = store.tags.find((x) => x.id === tagId);
    node.addEventListener("click", () => cb.onNavigate("tag", null, tagId));
    if (tag) {
      node.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        openTagContextMenu(e.clientX, e.clientY, tag, cb);
      });
    }
  });

  // Navigation & clic droit Abonnements
  el.querySelectorAll<HTMLElement>("[data-feed-id]").forEach((node) => {
    const feedId = Number(node.dataset.feedId);
    const feed = store.feeds.find((x) => x.id === feedId);
    node.addEventListener("click", () => cb.onNavigate("feed", feedId, null));
    if (feed) {
      node.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        openFeedContextMenu(e.clientX, e.clientY, feed, cb);
      });
    }
  });

  el.querySelector("#btn-new-tag")?.addEventListener("click", () => {
    openTagDialog(null, cb.onDataChanged);
  });

  el.querySelector("#btn-add-local")?.addEventListener("click", async () => {
    const dir = await api.pickFolder(t("nav.addLocalFolder"));
    if (!dir) return;
    try {
      const feedId = await api.addLocalFolder(dir);
      showToast(t("toasts.localFolderAdded"), "success");
      cb.onDataChanged();
      cb.onNavigate("feed", feedId, null);
    } catch (e) {
      showToast(String(e), "error");
    }
  });
}

function openTagContextMenu(x: number, y: number, tag: Tag, cb: SidebarCallbacks): void {
  showContextMenu(x, y, [
    {
      label: t("list.markAllPlayed"),
      onClick: async () => {
        if (tag.unread_count > 10) {
          const ok = await confirmDialog(
            t("list.markAllPlayed"),
            t("list.markAllPlayedConfirm", { count: tag.unread_count })
          );
          if (!ok) return;
        }
        await api.markTagAllPlayed(tag.id);
        cb.onDataChanged();
      },
    },
    { separator: true },
    {
      label: t("actions.renameTag"),
      onClick: () => openTagDialog(tag, cb.onDataChanged),
    },
    {
      label: t("actions.deleteTag"),
      danger: true,
      onClick: async () => {
        await api.deleteTag(tag.id);
        cb.onDataChanged();
      },
    },
  ]);
}

function openFeedContextMenu(x: number, y: number, feed: Feed, cb: SidebarCallbacks): void {
  showContextMenu(x, y, [
    {
      label: t("list.markAllPlayed"),
      onClick: async () => {
        if (feed.unread_count > 10) {
          const ok = await confirmDialog(
            t("list.markAllPlayed"),
            t("list.markAllPlayedConfirm", { count: feed.unread_count })
          );
          if (!ok) return;
        }
        await api.markFeedAllPlayed(feed.id);
        cb.onDataChanged();
      },
    },
    {
      label: t("list.refreshFeed"),
      onClick: async () => {
        const count = await api.refreshFeed(feed.id);
        showToast(t("toasts.refreshed", { count }), "success");
        cb.onDataChanged();
      },
    },
    { separator: true },
    {
      label: t("actions.manageTags"),
      onClick: () => openAssignFeedTagsDialog(feed, cb.onDataChanged),
    },
    {
      label: t("list.feedSettings"),
      onClick: () => openFeedSettingsDialog(feed, cb.onDataChanged),
    },
    { separator: true },
    {
      label: t("list.unsubscribe"),
      danger: true,
      onClick: async () => {
        const ok = await confirmDialog(
          t("list.unsubscribe"),
          t("list.unsubscribeConfirm", { title: feed.title }),
          true
        );
        if (!ok) return;
        await api.deleteFeed(feed.id);
        if (store.activeFeedId === feed.id) {
          cb.onNavigate("all", null, null);
        }
        cb.onDataChanged();
      },
    },
  ]);
}
