/**
 * Barre de lecture persistante en bas + Mode Mini-Lecteur Flottant (Always-on-top, 44 px de haut,
 * ancrable en haut ou en bas de l'écran).
 *
 * Fonctionnalités desktop :
 * - Barre de progression large cliquable au pixel près
 * - Molette de la souris sur le volume ET sur la vitesse de lecture (paliers 0.05x)
 * - Bouton Marquer comme lu (passe au suivant), bouton Favori, FX (Skip Silence / Voice Boost),
 *   Minuterie de sommeil (X min ou Fin d'épisode avec fade-out 15 s) et accès File d'attente
 */
import { LogicalPosition, LogicalSize, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { api, resolveCoverUrl } from "../api";
import { formatDurationMs, t } from "../i18n";
import { escapeHtml, showToast, store } from "../state";
import { showContextMenu } from "./contextMenu";
import { icons } from "./icons";

export interface PlayerBarCallbacks {
  onOpenQueue: () => void;
  onDataChanged: () => void;
}

let savedWindowSize: { w: number; h: number } | null = null;
let savedWindowPos: { x: number; y: number } | null = null;
let lastNonZeroVolume = 1.0;

export function renderPlayerBar(cb: PlayerBarCallbacks): void {
  const bar = document.getElementById("player-bar");
  if (!bar) return;

  const a = store.audio;
  const cover = resolveCoverUrl(a.cover_url);
  const pct = a.duration_ms > 0 ? Math.min(100, (a.position_ms / a.duration_ms) * 100) : 0;
  const currentEp = store.episodes.find((e) => e.id === a.episode_id);
  const isFav = currentEp?.is_favorite ?? false;

  const sleepLabel = a.sleep_at_end_of_episode
    ? `${icons.moon(14)} <span>End</span>`
    : a.sleep_timer_remaining_sec !== null
      ? `${icons.moon(14)} <span>${Math.ceil(a.sleep_timer_remaining_sec / 60)}m</span>`
      : icons.moon(16);

  bar.innerHTML = `
    <div class="player-track">
      ${
        cover
          ? `<img class="player-cover" src="${escapeHtml(cover)}" alt="" />`
          : `<div class="player-cover" style="display:grid;place-items:center;color:var(--accent-hover);">${icons.podcast(20)}</div>`
      }
      <div class="player-meta">
        <div class="player-title">${escapeHtml(a.title || t("player.noTrack"))}</div>
        ${
          !store.isMiniPlayer
            ? `<div class="player-feed">${escapeHtml(a.feed_title || t("app.tagline"))}</div>`
            : ""
        }
      </div>
    </div>

    <div class="player-center">
      <div class="player-controls">
        <button class="icon-btn" id="pl-rew" title="${escapeHtml(t("player.rewind"))}" style="width:auto;padding:0 6px;font-size:11.5px;font-weight:600;">↺ 15</button>
        <button class="play-main-btn" id="pl-play" title="${escapeHtml(
          a.is_playing ? t("player.pause") : t("player.play")
        )}">
          ${a.is_loading ? icons.refresh(16) : a.is_playing ? icons.pause(16) : icons.play(16)}
        </button>
        <button class="icon-btn" id="pl-fwd" title="${escapeHtml(t("player.forward"))}" style="width:auto;padding:0 6px;font-size:11.5px;font-weight:600;">30 ↻</button>
        <button class="icon-btn" id="pl-next" title="${escapeHtml(t("player.next"))}">${icons.next(16)}</button>
      </div>

      <div class="progress-row">
        <span class="progress-time tabular" id="pl-pos">${escapeHtml(
          formatDurationMs(a.position_ms)
        )}</span>
        <div class="seek-bar" id="pl-seek">
          <div class="seek-track">
            <div class="seek-fill" id="pl-seek-fill" style="width:${pct.toFixed(2)}%"></div>
          </div>
        </div>
        <span class="progress-time tabular" id="pl-dur">${escapeHtml(
          formatDurationMs(a.duration_ms)
        )}</span>
      </div>
    </div>

    <div class="player-right">
      <!-- Contrôle accessible Vitesse de lecture (boutons -/+ + barre glissière + menu paliers + molette) -->
      <div class="player-control-pill" id="pl-speed-box" title="${escapeHtml(t("player.speedTooltip"))}">
        <button class="player-step-btn" id="pl-speed-dec" title="-0.05x">−</button>
        ${
          !store.isMiniPlayer
            ? `<input type="range" class="range-modern speed-slider" id="pl-speed-range" min="50" max="300" step="5"
                      value="${Math.round(a.speed * 100)}" />`
            : ""
        }
        <button class="speed-pill tabular" id="pl-speed">${a.speed.toFixed(2)}x</button>
        <button class="player-step-btn" id="pl-speed-inc" title="+0.05x">+</button>
      </div>

      <!-- Contrôle accessible Volume (bouton muet + barre glissière + pourcentage + molette) -->
      <div class="player-control-pill" id="pl-vol-box" title="${escapeHtml(t("player.volumeTooltip"))}">
        <button class="player-step-btn" id="pl-mute" style="width:24px;height:24px;color:${
          a.volume <= 0.001 ? "var(--danger)" : "var(--text-primary)"
        };">${icons.volume(15)}</button>
        <input type="range" class="range-modern volume-slider" id="pl-vol" min="0" max="150" step="1"
               value="${Math.round(a.volume * 100)}" />
        ${
          !store.isMiniPlayer
            ? `<span class="volume-pct tabular" id="pl-vol-pct">${Math.round(a.volume * 100)}%</span>`
            : ""
        }
      </div>

      ${
        !store.isMiniPlayer
          ? `
        <button class="icon-btn ${a.skip_silence ? "active" : ""}" id="pl-silence" title="${escapeHtml(
              t("player.skipSilence")
            )}">${icons.scissors(15)}</button>
        <button class="icon-btn ${a.voice_boost ? "active" : ""}" id="pl-voice" title="${escapeHtml(
              t("player.voiceBoost")
            )}">${icons.mic(15)}</button>
        <button class="icon-btn ${a.sleep_at_end_of_episode || a.sleep_timer_remaining_sec !== null ? "active" : ""}" id="pl-sleep" title="${escapeHtml(
              t("player.sleepTimer")
            )}" style="width:auto;padding:0 7px;font-size:11.5px;gap:4px;">${sleepLabel}</button>
        <button class="icon-btn" id="pl-mark-played" title="${escapeHtml(
              t("player.markPlayed")
            )}">${icons.check(16)}</button>
        <button class="icon-btn" id="pl-fav" title="${escapeHtml(
              t("player.favorite")
            )}" style="color:${isFav ? "var(--star)" : ""};">${icons.star(16, isFav)}</button>
        <button class="icon-btn" id="pl-queue" title="${escapeHtml(t("player.queueBtn"))}">${icons.queue(16)}</button>
      `
          : `
        <button class="icon-btn" id="pl-mark-played" title="${escapeHtml(t("player.markPlayed"))}">${icons.check(16)}</button>
        <button class="icon-btn" id="pl-dock" title="${escapeHtml(
          store.miniDock === "top" ? t("app.dockBottom") : t("app.dockTop")
        )}">${store.miniDock === "top" ? "⤓" : "⤒"}</button>
      `
      }
      <button class="icon-btn" id="pl-mini" title="${escapeHtml(
        store.isMiniPlayer ? t("app.exitMiniPlayer") : t("app.miniPlayer")
      )}">${store.isMiniPlayer ? icons.expandWindow(15) : icons.miniPlayer(16)}</button>
    </div>
  `;

  bar.querySelector("#pl-play")?.addEventListener("click", () => void api.togglePlayPause());
  bar.querySelector("#pl-rew")?.addEventListener("click", () => {
    void api.seekAudio(Math.max(0, store.audio.position_ms - 15_000));
  });
  bar.querySelector("#pl-fwd")?.addEventListener("click", () => {
    void api.seekAudio(store.audio.position_ms + 30_000);
  });
  bar.querySelector("#pl-next")?.addEventListener("click", () => void api.playNextInQueue());

  // Barre de progression cliquable au pixel près
  const seekEl = bar.querySelector("#pl-seek") as HTMLElement | null;
  seekEl?.addEventListener("click", (e) => {
    if (store.audio.duration_ms <= 0) return;
    const rect = seekEl.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    void api.seekAudio(Math.round(ratio * store.audio.duration_ms));
  });

  // Contrôle complet Vitesse : boutons -/+, barre glissière, menu paliers et molette (±0.05x)
  const applySpeed = (val: number) => {
    const clamped = Math.max(0.5, Math.min(3.0, Math.round(val * 20) / 20));
    store.audio.speed = clamped;
    const lbl = bar.querySelector("#pl-speed");
    if (lbl) lbl.textContent = `${clamped.toFixed(2)}x`;
    const rng = bar.querySelector("#pl-speed-range") as HTMLInputElement | null;
    if (rng) rng.value = String(Math.round(clamped * 100));
    void api.setSpeed(clamped, true);
  };

  bar.querySelector("#pl-speed-dec")?.addEventListener("click", () => applySpeed(store.audio.speed - 0.05));
  bar.querySelector("#pl-speed-inc")?.addEventListener("click", () => applySpeed(store.audio.speed + 0.05));
  const speedRange = bar.querySelector("#pl-speed-range") as HTMLInputElement | null;
  speedRange?.addEventListener("input", () => applySpeed(Number(speedRange.value) / 100));
  bar.querySelector("#pl-speed-box")?.addEventListener("wheel", (e) => {
    e.preventDefault();
    const delta = (e as WheelEvent).deltaY < 0 ? 0.05 : -0.05;
    applySpeed(store.audio.speed + delta);
  });
  bar.querySelector("#pl-speed")?.addEventListener("click", (e) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const presets = [0.75, 1.0, 1.15, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
    showContextMenu(
      rect.left,
      rect.top - 220,
      presets.map((sp) => ({
        label: `${Math.abs(store.audio.speed - sp) < 0.02 ? "● " : "   "}${sp.toFixed(2)}x`,
        onClick: () => applySpeed(sp),
      }))
    );
  });

  // Contrôle complet Volume : bouton Muet, barre glissière, pourcentage et molette
  const volInput = bar.querySelector("#pl-vol") as HTMLInputElement | null;
  const volPct = bar.querySelector("#pl-vol-pct") as HTMLElement | null;
  const applyVolume = (val: number) => {
    const clamped = Math.max(0, Math.min(1.5, Math.round(val * 100) / 100));
    if (clamped > 0.01) lastNonZeroVolume = clamped;
    store.audio.volume = clamped;
    if (volInput) volInput.value = String(Math.round(clamped * 100));
    if (volPct) volPct.textContent = `${Math.round(clamped * 100)}%`;
    void api.setVolume(clamped);
  };
  bar.querySelector("#pl-mute")?.addEventListener("click", () => {
    if (store.audio.volume > 0.01) applyVolume(0);
    else applyVolume(lastNonZeroVolume || 1.0);
    renderPlayerBar(cb);
  });
  volInput?.addEventListener("input", () => applyVolume(Number(volInput.value) / 100));
  bar.querySelector("#pl-vol-box")?.addEventListener("wheel", (e) => {
    e.preventDefault();
    const delta = (e as WheelEvent).deltaY < 0 ? 0.05 : -0.05;
    applyVolume(store.audio.volume + delta);
  });

  // FX audio temps réel
  bar.querySelector("#pl-silence")?.addEventListener("click", () => {
    void api.setAudioFx(!a.skip_silence, a.voice_boost, a.normalize_loudness);
  });
  bar.querySelector("#pl-voice")?.addEventListener("click", () => {
    void api.setAudioFx(a.skip_silence, !a.voice_boost, a.normalize_loudness);
  });

  // Minuterie de sommeil
  bar.querySelector("#pl-sleep")?.addEventListener("click", (e) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    showContextMenu(rect.left - 80, rect.top - 220, [
      { label: t("player.sleepOff"), onClick: () => void api.setSleepTimer(null, false) },
      { separator: true },
      ...[10, 15, 30, 45, 60].map((min) => ({
        label: t("player.sleepMinutes", { min }),
        onClick: () => void api.setSleepTimer(min, false),
      })),
      { separator: true },
      { label: t("player.sleepEndOfEpisode"), onClick: () => void api.setSleepTimer(null, true) },
    ]);
  });

  // Marquer comme lu et passer au suivant
  bar.querySelector("#pl-mark-played")?.addEventListener("click", async () => {
    if (!a.episode_id) return;
    await api.markEpisodesPlayed([a.episode_id], true);
    cb.onDataChanged();
  });

  // Favori
  bar.querySelector("#pl-fav")?.addEventListener("click", async () => {
    if (!a.episode_id) return;
    await api.setEpisodesFavorite([a.episode_id], !isFav);
    cb.onDataChanged();
  });

  bar.querySelector("#pl-queue")?.addEventListener("click", cb.onOpenQueue);
  bar.querySelector("#pl-mini")?.addEventListener("click", () => void toggleMiniPlayerMode(cb));
  bar.querySelector("#pl-dock")?.addEventListener("click", () => {
    store.miniDock = store.miniDock === "top" ? "bottom" : "top";
    void positionMiniPlayerWindow();
    renderPlayerBar(cb);
  });
}

/** Met à jour uniquement le curseur/temps toutes les 250 ms sans reconstruire tout le DOM du player. */
export function patchPlayerProgressOnly(): void {
  const posEl = document.getElementById("pl-pos");
  const durEl = document.getElementById("pl-dur");
  const fillEl = document.getElementById("pl-seek-fill");
  const a = store.audio;
  if (posEl) posEl.textContent = formatDurationMs(a.position_ms);
  if (durEl) durEl.textContent = formatDurationMs(a.duration_ms);
  if (fillEl) {
    const pct = a.duration_ms > 0 ? Math.min(100, (a.position_ms / a.duration_ms) * 100) : 0;
    fillEl.style.width = `${pct.toFixed(2)}%`;
  }
}

/** Bascule en 1 clic entre la fenêtre Desktop complète et la barre Mini-Lecteur 44 px Always-on-top. */
export async function toggleMiniPlayerMode(cb: PlayerBarCallbacks): Promise<void> {
  const shell = document.getElementById("app");
  if (!shell) return;
  const win = getCurrentWindow();

  try {
    if (!store.isMiniPlayer) {
      const [sz, pos] = await Promise.all([win.outerSize(), win.outerPosition()]);
      const scale = window.devicePixelRatio || 1;
      savedWindowSize = { w: Math.round(sz.width / scale), h: Math.round(sz.height / scale) };
      savedWindowPos = { x: Math.round(pos.x / scale), y: Math.round(pos.y / scale) };

      store.isMiniPlayer = true;
      shell.classList.add("mini-mode");
      await win.setDecorations(false);
      await win.setAlwaysOnTop(true);
      await positionMiniPlayerWindow();
    } else {
      store.isMiniPlayer = false;
      shell.classList.remove("mini-mode");
      await win.setAlwaysOnTop(false);
      await win.setDecorations(true);
      if (savedWindowSize) {
        await win.setSize(new LogicalSize(savedWindowSize.w, savedWindowSize.h));
      }
      if (savedWindowPos) {
        await win.setPosition(new LogicalPosition(savedWindowPos.x, savedWindowPos.y));
      }
    }
    renderPlayerBar(cb);
  } catch (e) {
    showToast(String(e), "error");
  }
}

async function positionMiniPlayerWindow(): Promise<void> {
  const win = getCurrentWindow();
  const mon = await currentMonitor();
  const scale = mon?.scaleFactor || window.devicePixelRatio || 1;
  const screenW = mon ? Math.round(mon.size.width / scale) : 1280;
  const screenH = mon ? Math.round(mon.size.height / scale) : 800;
  const width = Math.min(920, screenW - 40);
  const x = Math.round((screenW - width) / 2);
  const y = store.miniDock === "top" ? 8 : Math.max(8, screenH - 94);
  await win.setSize(new LogicalSize(width, 44));
  await win.setPosition(new LogicalPosition(x, y));
}
