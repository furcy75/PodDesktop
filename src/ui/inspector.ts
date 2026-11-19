/**
 * Panneau droit (Inspecteur / Détails) :
 * - Notes d'émission HTML nettoyées (anti-XSS) avec timestamps cliquables (`12:34`, `1:02:15`)
 *   et ouverture des liens dans le navigateur externe
 * - Chapitres avec miniature et saut direct au clic
 * - Signets personnels horodatés (création, saut, suppression)
 * - Transcriptions Podcasting 2.0 (SRT/VTT/JSON) avec recherche textuelle et suivi synchronisé
 * - Bouton de soutien créateur (`<podcast:funding>`) et partage avec horodatage (`#t=`)
 */
import { api, resolveCoverUrl } from "../api";
import { formatDate, formatDurationMs, t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";
import type { Bookmark, Chapter, Episode, TranscriptCue } from "../types";
import { openBookmarkDialog } from "./dialogs";

let cachedEpisodeId: number | null = null;
let cachedChapters: Chapter[] = [];
let cachedBookmarks: Bookmark[] = [];
let cachedTranscript: TranscriptCue[] = [];
let transcriptSearch = "";

export async function renderInspector(episodeId: number | null): Promise<void> {
  const panel = document.getElementById("inspector");
  if (!panel) return;

  if (!episodeId) {
    panel.innerHTML = `<div class="empty-state" style="margin-top:40px;"><p>${escapeHtml(
      t("inspector.noSelection")
    )}</p></div>`;
    return;
  }

  let ep: Episode | undefined = store.episodes.find((e) => e.id === episodeId);
  if (!ep) {
    ep = (await api.getEpisode(episodeId)) ?? undefined;
  }
  if (!ep) return;

  if (cachedEpisodeId !== episodeId) {
    cachedEpisodeId = episodeId;
    transcriptSearch = "";
    const [chapters, bookmarks] = await Promise.all([
      api.getChapters(episodeId).catch(() => []),
      api.listBookmarks(episodeId).catch(() => []),
    ]);
    cachedChapters = chapters;
    cachedBookmarks = bookmarks;
    cachedTranscript = [];
    if (ep.transcript_data) {
      cachedTranscript = await api.loadTranscript(episodeId).catch(() => []);
    }
  }

  const cover = resolveCoverUrl(ep.feed_image_url);
  const currentEp = ep;

  panel.innerHTML = `
    <div class="inspector-header">
      <div class="inspector-top-row">
        ${
          cover
            ? `<img class="inspector-cover" src="${escapeHtml(cover)}" alt="" />`
            : `<div class="inspector-cover" style="display:grid;place-items:center;font-size:22px;">🎙</div>`
        }
        <div style="min-width:0;flex:1;">
          <div style="font-weight:700;font-size:13.5px;line-height:1.3;">${escapeHtml(
            currentEp.title
          )}</div>
          <div style="font-size:12px;color:var(--text-secondary);margin-top:2px;">${escapeHtml(
            currentEp.feed_title
          )}</div>
          <div style="font-size:11.5px;color:var(--text-muted);margin-top:2px;" class="tabular">
            ${escapeHtml(formatDate(currentEp.pub_date))} · ${escapeHtml(
    formatDurationMs(currentEp.duration_ms)
  )}
          </div>
        </div>
      </div>
      <div style="display:flex;gap:6px;flex-wrap:wrap;">
        <button class="btn btn-primary btn-sm" id="insp-play">▶ ${escapeHtml(
          t("actions.playNow")
        )}</button>
        <button class="btn btn-sm" id="insp-share" title="${escapeHtml(
          t("actions.copyTimestampLink")
        )}">🔗 #t=</button>
        ${
          currentEp.feed_funding_url
            ? `<button class="btn btn-sm" id="insp-funding" style="color:var(--success);">♥ ${escapeHtml(
                t("inspector.supportCreator")
              )}</button>`
            : ""
        }
      </div>
    </div>

    <div class="inspector-tabs">
      <button class="inspector-tab ${store.inspectorTab === "notes" ? "active" : ""}" data-tab="notes">
        ${escapeHtml(t("inspector.tabNotes"))}
      </button>
      <button class="inspector-tab ${store.inspectorTab === "chapters" ? "active" : ""}" data-tab="chapters">
        ${escapeHtml(t("inspector.tabChapters"))}${cachedChapters.length ? ` (${cachedChapters.length})` : ""}
      </button>
      <button class="inspector-tab ${store.inspectorTab === "bookmarks" ? "active" : ""}" data-tab="bookmarks">
        ${escapeHtml(t("inspector.tabBookmarks"))}${cachedBookmarks.length ? ` (${cachedBookmarks.length})` : ""}
      </button>
      <button class="inspector-tab ${store.inspectorTab === "transcript" ? "active" : ""}" data-tab="transcript">
        ${escapeHtml(t("inspector.tabTranscript"))}
      </button>
    </div>

    <div class="inspector-body" id="inspector-body"></div>
  `;

  panel.querySelector("#insp-play")?.addEventListener("click", () => {
    void api.playEpisode(currentEp.id);
  });

  panel.querySelector("#insp-share")?.addEventListener("click", async () => {
    const posMs =
      store.audio.episode_id === currentEp.id ? store.audio.position_ms : currentEp.position_ms;
    const sec = Math.floor(posMs / 1000);
    await navigator.clipboard.writeText(`${currentEp.audio_url}#t=${sec}`);
    showToast(t("inspector.copyShareLink"), "success");
  });

  panel.querySelector("#insp-funding")?.addEventListener("click", () => {
    if (currentEp.feed_funding_url) void api.openExternalUrl(currentEp.feed_funding_url);
  });

  panel.querySelectorAll<HTMLElement>("[data-tab]").forEach((tab) => {
    tab.addEventListener("click", () => {
      store.inspectorTab = tab.dataset.tab as typeof store.inspectorTab;
      void renderInspector(currentEp.id);
    });
  });

  const body = panel.querySelector("#inspector-body") as HTMLElement;
  renderInspectorTabContent(body, currentEp);
}

/** Met en surbrillance la ligne de transcription correspondant à la position de lecture active. */
export function syncTranscriptHighlight(episodeId: number | null, positionMs: number): void {
  if (
    !episodeId ||
    episodeId !== cachedEpisodeId ||
    store.inspectorTab !== "transcript" ||
    cachedTranscript.length === 0
  ) {
    return;
  }
  const container = document.getElementById("transcript-list");
  if (!container) return;

  let activeIdx = -1;
  for (let i = 0; i < cachedTranscript.length; i++) {
    if (positionMs >= cachedTranscript[i].start_ms) {
      activeIdx = i;
    } else {
      break;
    }
  }

  container.querySelectorAll<HTMLElement>(".cue-item").forEach((el) => {
    const idx = Number(el.dataset.idx);
    const wasActive = el.classList.contains("active");
    const isNow = idx === activeIdx;
    el.classList.toggle("active", isNow);
    if (isNow && !wasActive) {
      el.scrollIntoView({ block: "nearest", behavior: "smooth" });
    }
  });
}

function renderInspectorTabContent(body: HTMLElement, ep: Episode): void {
  if (store.inspectorTab === "notes") {
    if (!ep.description) {
      body.innerHTML = `<p style="color:var(--text-muted)">${escapeHtml(
        t("inspector.noNotes")
      )}</p>`;
      return;
    }
    body.innerHTML = `<div class="show-notes">${sanitizeAndLinkifyNotes(ep.description)}</div>`;
    // Intercepte les liens externes et les timestamps cliquables
    body.querySelectorAll<HTMLAnchorElement>("a[href]").forEach((a) => {
      a.addEventListener("click", (e) => {
        e.preventDefault();
        const href = a.getAttribute("href");
        if (href && (href.startsWith("http://") || href.startsWith("https://"))) {
          void api.openExternalUrl(href);
        }
      });
    });
    body.querySelectorAll<HTMLElement>(".timestamp-link").forEach((tsEl) => {
      tsEl.addEventListener("click", () => {
        const ms = Number(tsEl.dataset.ms || "0");
        void jumpToEpisodeTime(ep.id, ms);
      });
    });
    return;
  }

  if (store.inspectorTab === "chapters") {
    if (cachedChapters.length === 0) {
      body.innerHTML = `<p style="color:var(--text-muted)">${escapeHtml(
        t("inspector.noChapters")
      )}</p>`;
      return;
    }
    body.innerHTML = cachedChapters
      .map(
        (ch) => `
      <div class="chapter-item" data-ms="${ch.start_time_ms}">
        ${
          ch.image_url
            ? `<img src="${escapeHtml(ch.image_url)}" style="width:36px;height:36px;border-radius:6px;object-fit:cover;" />`
            : ""
        }
        <span class="time-tag">${escapeHtml(formatDurationMs(ch.start_time_ms))}</span>
        <div style="flex:1;min-width:0;">
          <div>${escapeHtml(ch.title)}</div>
          ${
            ch.url
              ? `<a href="${escapeHtml(ch.url)}" style="font-size:11px;color:var(--accent-cyan);">↗ Link</a>`
              : ""
          }
        </div>
      </div>
    `
      )
      .join("");
    body.querySelectorAll<HTMLElement>(".chapter-item").forEach((el) => {
      el.addEventListener("click", (e) => {
        if ((e.target as HTMLElement).tagName === "A") {
          e.preventDefault();
          const href = (e.target as HTMLAnchorElement).getAttribute("href");
          if (href) void api.openExternalUrl(href);
          return;
        }
        void jumpToEpisodeTime(ep.id, Number(el.dataset.ms || "0"));
      });
    });
    return;
  }

  if (store.inspectorTab === "bookmarks") {
    const posMs = store.audio.episode_id === ep.id ? store.audio.position_ms : ep.position_ms;
    body.innerHTML = `
      <div style="margin-bottom:10px;">
        <button class="btn btn-sm btn-primary" id="btn-add-bm">+ ${escapeHtml(
          t("inspector.addBookmarkBtn", { time: formatDurationMs(posMs) })
        )}</button>
      </div>
      ${
        cachedBookmarks.length === 0
          ? `<p style="color:var(--text-muted)">${escapeHtml(t("inspector.noBookmarks"))}</p>`
          : cachedBookmarks
              .map(
                (bm) => `
          <div class="bookmark-item" data-ms="${bm.position_ms}">
            <span class="time-tag">${escapeHtml(formatDurationMs(bm.position_ms))}</span>
            <div style="flex:1;">${escapeHtml(bm.note || "—")}</div>
            <button class="icon-btn" data-del-bm="${bm.id}" style="width:22px;height:22px;">✕</button>
          </div>`
              )
              .join("")
      }
    `;
    body.querySelector("#btn-add-bm")?.addEventListener("click", () => {
      openBookmarkDialog(ep.id, posMs, () => {
        cachedEpisodeId = null;
        void renderInspector(ep.id);
      });
    });
    body.querySelectorAll<HTMLElement>(".bookmark-item").forEach((el) => {
      el.addEventListener("click", () => {
        void jumpToEpisodeTime(ep.id, Number(el.dataset.ms || "0"));
      });
    });
    body.querySelectorAll<HTMLElement>("[data-del-bm]").forEach((btn) => {
      btn.addEventListener("click", async (e) => {
        e.stopPropagation();
        await api.deleteBookmark(Number(btn.dataset.delBm));
        cachedEpisodeId = null;
        void renderInspector(ep.id);
      });
    });
    return;
  }

  // Onglet Transcription (Podcasting 2.0)
  if (cachedTranscript.length === 0) {
    if (!ep.transcript_url) {
      body.innerHTML = `<p style="color:var(--text-muted)">${escapeHtml(
        t("inspector.noTranscript")
      )}</p>`;
      return;
    }
    body.innerHTML = `
      <button class="btn btn-primary" id="btn-fetch-tr">${escapeHtml(
        t("inspector.loadTranscript")
      )}</button>
    `;
    body.querySelector("#btn-fetch-tr")?.addEventListener("click", async () => {
      try {
        cachedTranscript = await api.loadTranscript(ep.id);
        renderInspectorTabContent(body, ep);
      } catch (e) {
        showToast(String(e), "error");
      }
    });
    return;
  }

  const filterLower = transcriptSearch.trim().toLowerCase();
  const filtered = cachedTranscript
    .map((cue, idx) => ({ cue, idx }))
    .filter(
      ({ cue }) =>
        !filterLower ||
        cue.text.toLowerCase().includes(filterLower) ||
        (cue.speaker?.toLowerCase().includes(filterLower) ?? false)
    );

  body.innerHTML = `
    <div style="margin-bottom:8px;">
      <input type="text" id="tr-search" value="${escapeHtml(transcriptSearch)}"
             placeholder="${escapeHtml(t("inspector.searchTranscript"))}"
             style="width:100%;height:28px;padding:0 8px;border-radius:6px;border:1px solid var(--border-strong);background:var(--bg-elevated);" />
    </div>
    <div id="transcript-list">
      ${filtered
        .map(
          ({ cue, idx }) => `
        <div class="cue-item" data-idx="${idx}" data-ms="${cue.start_ms}">
          <span class="time-tag">${escapeHtml(formatDurationMs(cue.start_ms))}</span>
          <div style="flex:1;">
            ${cue.speaker ? `<strong>${escapeHtml(cue.speaker)} : </strong>` : ""}
            ${escapeHtml(cue.text)}
          </div>
        </div>`
        )
        .join("")}
    </div>
  `;

  body.querySelector("#tr-search")?.addEventListener("input", (e) => {
    transcriptSearch = (e.target as HTMLInputElement).value;
    renderInspectorTabContent(body, ep);
    const nextInput = body.querySelector("#tr-search") as HTMLInputElement | null;
    if (nextInput) {
      nextInput.focus();
      nextInput.setSelectionRange(transcriptSearch.length, transcriptSearch.length);
    }
  });

  body.querySelectorAll<HTMLElement>(".cue-item").forEach((el) => {
    el.addEventListener("click", () => {
      void jumpToEpisodeTime(ep.id, Number(el.dataset.ms || "0"));
    });
  });
}

async function jumpToEpisodeTime(episodeId: number, ms: number): Promise<void> {
  if (store.audio.episode_id === episodeId) {
    await api.seekAudio(ms);
  } else {
    await api.playEpisode(episodeId, ms);
  }
}

/**
 * Nettoie le HTML des Show Notes (supprime `<script>`, `<iframe>`, gestionnaires `on*`)
 * et transforme les horodatages (`01:23:45` ou `12:34`) en badges cliquables.
 */
function sanitizeAndLinkifyNotes(rawHtml: string): string {
  const parser = new DOMParser();
  const doc = parser.parseFromString(rawHtml, "text/html");
  doc.querySelectorAll("script, style, iframe, object, embed, form").forEach((n) => n.remove());
  doc.querySelectorAll("*").forEach((el) => {
    for (const attr of Array.from(el.attributes)) {
      if (attr.name.toLowerCase().startsWith("on")) {
        el.removeAttribute(attr.name);
      }
      if (attr.name === "href" && attr.value.trim().toLowerCase().startsWith("javascript:")) {
        el.removeAttribute(attr.name);
      }
    }
  });

  let clean = doc.body.innerHTML;
  // Convertit les timestamps en texte libre (`12:34` ou `1:02:30`) en boutons de saut
  clean = clean.replace(/(?<!\d)(\d{1,2}):(\d{2})(?::(\d{2}))?(?!\d)/g, (match, a, b, c) => {
    const secs = c !== undefined ? Number(a) * 3600 + Number(b) * 60 + Number(c) : Number(a) * 60 + Number(b);
    return `<span class="timestamp-link" data-ms="${secs * 1000}">${match}</span>`;
  });
  return clean;
}

export function invalidateInspectorCache(): void {
  cachedEpisodeId = null;
}
