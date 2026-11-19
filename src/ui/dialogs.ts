/**
 * Fenêtres modales accessibles : confirmation, signet rapide (`B`),
 * réglages spécifiques d'un podcast, gestion des tags/dossiers.
 */
import { api } from "../api";
import { formatDurationMs, t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";
import type { Feed, Tag } from "../types";

function openModal(html: string): HTMLElement {
  const root = document.getElementById("modal-root")!;
  root.innerHTML = `<div class="modal-card">${html}</div>`;
  root.classList.remove("hidden");

  const closeOnBackdrop = (e: MouseEvent) => {
    if (e.target === root) {
      closeModal();
      root.removeEventListener("click", closeOnBackdrop);
    }
  };
  root.addEventListener("click", closeOnBackdrop);
  return root.querySelector(".modal-card") as HTMLElement;
}

export function closeModal(): void {
  const root = document.getElementById("modal-root");
  if (root) {
    root.classList.add("hidden");
    root.innerHTML = "";
  }
}

/** Modale de confirmation asynchrone (ex: "Tout marquer comme lu" si > 10 épisodes). */
export function confirmDialog(title: string, message: string, danger = false): Promise<boolean> {
  return new Promise((resolve) => {
    const card = openModal(`
      <h3>${escapeHtml(title)}</h3>
      <p style="color:var(--text-secondary)">${escapeHtml(message)}</p>
      <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:8px;">
        <button class="btn" id="dlg-cancel">${escapeHtml(t("app.cancel"))}</button>
        <button class="btn ${danger ? "btn-danger" : "btn-primary"}" id="dlg-ok">${escapeHtml(
      t("app.confirm")
    )}</button>
      </div>
    `);
    card.querySelector("#dlg-cancel")?.addEventListener("click", () => {
      closeModal();
      resolve(false);
    });
    card.querySelector("#dlg-ok")?.addEventListener("click", () => {
      closeModal();
      resolve(true);
    });
  });
}

/** Modale de pose de signet personnel horodaté (raccourci clavier `B`). */
export function openBookmarkDialog(
  episodeId: number,
  positionMs: number,
  onSaved: () => void
): void {
  const timeStr = formatDurationMs(positionMs);
  const card = openModal(`
    <h3>${escapeHtml(t("actions.addBookmark"))}</h3>
    <div class="form-field">
      <label>${escapeHtml(t("inspector.bookmarkPrompt", { time: timeStr }))}</label>
      <input type="text" id="bm-note" autofocus placeholder="…" />
    </div>
    <div style="display:flex;justify-content:flex-end;gap:8px;">
      <button class="btn" id="bm-cancel">${escapeHtml(t("app.cancel"))}</button>
      <button class="btn btn-primary" id="bm-save">${escapeHtml(t("app.save"))}</button>
    </div>
  `);
  const input = card.querySelector("#bm-note") as HTMLInputElement;
  input?.focus();

  const submit = async () => {
    try {
      await api.addBookmark(episodeId, positionMs, input.value);
      closeModal();
      onSaved();
    } catch (e) {
      showToast(String(e), "error");
    }
  };
  input?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") void submit();
    if (e.key === "Escape") closeModal();
  });
  card.querySelector("#bm-cancel")?.addEventListener("click", closeModal);
  card.querySelector("#bm-save")?.addEventListener("click", () => void submit());
}

/** Modale de création ou édition d'un Tag / Dossier personnalisé. */
export function openTagDialog(existing: Tag | null, onSaved: () => void): void {
  const card = openModal(`
    <h3>${escapeHtml(existing ? t("actions.renameTag") : t("nav.addTag"))}</h3>
    <div class="form-field">
      <label>${escapeHtml(t("list.colTitle"))}</label>
      <input type="text" id="tag-name" value="${escapeHtml(existing?.name || "")}" />
    </div>
    <div class="form-field">
      <label>Color</label>
      <input type="color" id="tag-color" value="${escapeHtml(existing?.color || "#6366f1")}" style="width:64px;height:34px;padding:2px;" />
    </div>
    <div style="display:flex;justify-content:flex-end;gap:8px;">
      <button class="btn" id="tag-cancel">${escapeHtml(t("app.cancel"))}</button>
      <button class="btn btn-primary" id="tag-save">${escapeHtml(t("app.save"))}</button>
    </div>
  `);
  const nameInput = card.querySelector("#tag-name") as HTMLInputElement;
  const colorInput = card.querySelector("#tag-color") as HTMLInputElement;
  nameInput?.focus();

  const submit = async () => {
    if (!nameInput.value.trim()) return;
    try {
      if (existing) {
        await api.updateTag(existing.id, nameInput.value.trim(), colorInput.value);
      } else {
        await api.upsertTag(nameInput.value.trim(), colorInput.value);
      }
      closeModal();
      onSaved();
    } catch (e) {
      showToast(String(e), "error");
    }
  };
  card.querySelector("#tag-cancel")?.addEventListener("click", closeModal);
  card.querySelector("#tag-save")?.addEventListener("click", () => void submit());
}

/** Modale d'assignation des tags à un abonnement. */
export function openAssignFeedTagsDialog(feed: Feed, onSaved: () => void): void {
  const checkboxes = store.tags
    .map(
      (tag) => `
      <label style="display:flex;align-items:center;gap:8px;padding:4px 0;cursor:pointer;">
        <input type="checkbox" value="${tag.id}" ${feed.tag_ids.includes(tag.id) ? "checked" : ""} />
        <span class="tag-dot" style="background:${escapeHtml(tag.color)}"></span>
        <span>${escapeHtml(tag.name)}</span>
      </label>
    `
    )
    .join("");

  const card = openModal(`
    <h3>${escapeHtml(t("actions.manageTags"))} — ${escapeHtml(feed.title)}</h3>
    <div>${checkboxes || `<p style="color:var(--text-muted)">${escapeHtml(t("app.none"))}</p>`}</div>
    <div style="display:flex;justify-content:flex-end;gap:8px;">
      <button class="btn" id="ft-cancel">${escapeHtml(t("app.cancel"))}</button>
      <button class="btn btn-primary" id="ft-save">${escapeHtml(t("app.save"))}</button>
    </div>
  `);
  card.querySelector("#ft-cancel")?.addEventListener("click", closeModal);
  card.querySelector("#ft-save")?.addEventListener("click", async () => {
    const ids = Array.from(card.querySelectorAll<HTMLInputElement>("input[type=checkbox]:checked"))
      .map((el) => Number(el.value));
    try {
      await api.setFeedTags(feed.id, ids);
      closeModal();
      onSaved();
    } catch (e) {
      showToast(String(e), "error");
    }
  });
}

/** Modale complète des réglages spécifiques d'un abonnement (vitesse, filtres, rétention, auth). */
export function openFeedSettingsDialog(feed: Feed, onSaved: () => void): void {
  const card = openModal(`
    <h3>${escapeHtml(t("feedModal.title"))} — ${escapeHtml(feed.title)}</h3>
    <div class="form-field">
      <label>${escapeHtml(t("feedModal.speed"))}</label>
      <input type="number" step="0.05" min="0.5" max="3.0" id="fs-speed" value="${
        feed.playback_speed ?? ""
      }" placeholder="1.00" />
    </div>
    <div class="form-field">
      <label>${escapeHtml(t("feedModal.maxKept"))}</label>
      <input type="number" min="0" id="fs-max" value="${feed.max_kept_episodes ?? ""}" />
    </div>
    <label style="display:flex;align-items:center;gap:8px;">
      <input type="checkbox" id="fs-autodl" ${feed.auto_download ? "checked" : ""} />
      ${escapeHtml(t("feedModal.autoDownload"))}
    </label>
    <label style="display:flex;align-items:center;gap:8px;">
      <input type="checkbox" id="fs-autodel" ${feed.auto_delete ? "checked" : ""} />
      ${escapeHtml(t("feedModal.autoDelete"))}
    </label>
    <label style="display:flex;align-items:center;gap:8px;">
      <input type="checkbox" id="fs-keepupd" ${feed.keep_updated ? "checked" : ""} />
      ${escapeHtml(t("feedModal.keepUpdated"))}
    </label>
    <div class="form-field">
      <label>${escapeHtml(t("feedModal.includeKeywords"))}</label>
      <input type="text" id="fs-inc" value="${escapeHtml(feed.include_keywords || "")}" />
    </div>
    <div class="form-field">
      <label>${escapeHtml(t("feedModal.excludeKeywords"))}</label>
      <input type="text" id="fs-exc" value="${escapeHtml(feed.exclude_keywords || "")}" />
    </div>
    ${
      !feed.is_local_folder
        ? `
    <div class="form-field">
      <label>${escapeHtml(t("feedModal.authUser"))}</label>
      <input type="text" id="fs-user" value="${escapeHtml(feed.username || "")}" />
    </div>
    <div class="form-field">
      <label>${escapeHtml(t("feedModal.authPass"))}</label>
      <input type="password" id="fs-pass" placeholder="${escapeHtml(
        feed.has_password ? "••••••••" : ""
      )}" />
    </div>`
        : ""
    }
    <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:6px;">
      <button class="btn" id="fs-cancel">${escapeHtml(t("app.cancel"))}</button>
      <button class="btn btn-primary" id="fs-save">${escapeHtml(t("app.save"))}</button>
    </div>
  `);

  card.querySelector("#fs-cancel")?.addEventListener("click", closeModal);
  card.querySelector("#fs-save")?.addEventListener("click", async () => {
    const speedRaw = (card.querySelector("#fs-speed") as HTMLInputElement).value.trim();
    const maxRaw = (card.querySelector("#fs-max") as HTMLInputElement).value.trim();
    const passInput = card.querySelector("#fs-pass") as HTMLInputElement | null;
    try {
      await api.updateFeedSettings({
        feed_id: feed.id,
        playback_speed: speedRaw ? Number(speedRaw) : null,
        auto_download: (card.querySelector("#fs-autodl") as HTMLInputElement).checked,
        auto_delete: (card.querySelector("#fs-autodel") as HTMLInputElement).checked,
        keep_updated: (card.querySelector("#fs-keepupd") as HTMLInputElement).checked,
        max_kept_episodes: maxRaw ? Number(maxRaw) : null,
        include_keywords: (card.querySelector("#fs-inc") as HTMLInputElement).value.trim() || null,
        exclude_keywords: (card.querySelector("#fs-exc") as HTMLInputElement).value.trim() || null,
        username: (card.querySelector("#fs-user") as HTMLInputElement | null)?.value.trim() || null,
        password_enc: passInput && passInput.value ? passInput.value : null,
      });
      closeModal();
      onSaved();
    } catch (e) {
      showToast(String(e), "error");
    }
  });
}
