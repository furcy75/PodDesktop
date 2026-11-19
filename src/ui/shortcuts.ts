/**
 * Gestionnaire de raccourcis clavier Desktop 100 % personnalisables (`settings.shortcuts`) :
 * - Par défaut :
 *   - `Space` : Lecture / Pause
 *   - `[` / `]` : Diminuer / Augmenter la vitesse de `0.05x`
 *   - `KeyM` : Marquer l'épisode comme Lu / Non lu
 *   - `Shift+KeyM` : Marquer tout le podcast affiché comme Lu
 *   - `KeyB` : Poser un signet personnel horodaté avec note rapide
 *   - `KeyF` : Basculer Favori
 *   - `Delete` : Retirer de la file d'attente
 *   - `Ctrl+KeyK` : Ouvrir la Palette de Commandes
 *   - `Ctrl+KeyF` : Recherche instantanée (FTS5)
 *   - `F5` : Rafraîchir les flux
 * - Et `Ctrl+A` pour la multi-sélection complète de la liste courante.
 */
import { api } from "../api";
import { t } from "../i18n";
import { store } from "../state";
import { confirmDialog, openBookmarkDialog } from "./dialogs";

export interface ShortcutCallbacks {
  onOpenPalette: () => void;
  onFocusSearch: () => void;
  onRefreshAll: () => void;
  onSelectAll: () => void;
  onDataChanged: () => void;
  onBookmarkSaved: () => void;
}

/** Convertit un KeyboardEvent en identifiant canonique stable (ex: `Ctrl+KeyK`, `Shift+KeyM`, `[`). */
export function eventToShortcutCombo(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");

  // Pour les caractères de ponctuation ([ / ]) on garde `e.key`, sinon `e.code` indépendant AZERTY/QWERTY
  const keyToken = e.key === "[" || e.key === "]" ? e.key : e.code;
  parts.push(keyToken);
  return parts.join("+");
}

/** Vérifie si un KeyboardEvent correspond au raccourci configuré (ou sa valeur par défaut). */
function matchesShortcut(e: KeyboardEvent, actionId: string, fallback: string): boolean {
  const expected = store.settings?.shortcuts?.[actionId] || fallback;
  const combo = eventToShortcutCombo(e);
  if (combo === expected) return true;
  // Tolère `[` ou `]` même lorsque Shift/AltGr est requis sur clavier AZERTY
  if ((expected === "[" || expected === "]") && e.key === expected) return true;
  return false;
}

/** Indique qu'une capture de nouveau raccourci est en cours dans les Paramètres. */
let isCapturingShortcut = false;
export function setShortcutCaptureMode(active: boolean): void {
  isCapturingShortcut = active;
}

export function registerKeyboardShortcuts(cb: ShortcutCallbacks): void {
  window.addEventListener("keydown", async (e) => {
    if (isCapturingShortcut) return;

    const tag = (e.target as HTMLElement | null)?.tagName;
    const isTyping =
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      (e.target as HTMLElement | null)?.isContentEditable;

    // Raccourcis globaux actifs même depuis un champ texte
    if (matchesShortcut(e, "command_palette", "Ctrl+KeyK")) {
      e.preventDefault();
      cb.onOpenPalette();
      return;
    }
    if (matchesShortcut(e, "focus_search", "Ctrl+KeyF")) {
      e.preventDefault();
      cb.onFocusSearch();
      return;
    }
    if (matchesShortcut(e, "refresh_feeds", "F5")) {
      e.preventDefault();
      cb.onRefreshAll();
      return;
    }

    if (isTyping) return;

    // Ctrl + A : multi-sélection complète de la liste d'épisodes
    if ((e.ctrlKey || e.metaKey) && e.code === "KeyA") {
      e.preventDefault();
      cb.onSelectAll();
      return;
    }

    if (matchesShortcut(e, "play_pause", "Space")) {
      e.preventDefault();
      await api.togglePlayPause();
      return;
    }

    if (matchesShortcut(e, "speed_down", "[")) {
      e.preventDefault();
      const next = Math.max(0.5, Math.round((store.audio.speed - 0.05) * 20) / 20);
      await api.setSpeed(next, true);
      return;
    }
    if (matchesShortcut(e, "speed_up", "]")) {
      e.preventDefault();
      const next = Math.min(3.0, Math.round((store.audio.speed + 0.05) * 20) / 20);
      await api.setSpeed(next, true);
      return;
    }

    if (matchesShortcut(e, "mark_feed_played", "Shift+KeyM")) {
      if (store.view === "feed" && store.activeFeedId !== null) {
        e.preventDefault();
        const feed = store.feeds.find((f) => f.id === store.activeFeedId);
        if (feed && feed.unread_count > 10) {
          const ok = await confirmDialog(
            t("list.markAllPlayed"),
            t("list.markAllPlayedConfirm", { count: feed.unread_count })
          );
          if (!ok) return;
        }
        await api.markFeedAllPlayed(store.activeFeedId);
        cb.onDataChanged();
      }
      return;
    }

    if (matchesShortcut(e, "mark_played", "KeyM")) {
      e.preventDefault();
      const ids = getTargetEpisodeIds();
      if (ids.length === 0) return;
      const first = store.episodes.find((ep) => ep.id === ids[0]);
      await api.markEpisodesPlayed(ids, !(first?.is_played ?? false));
      cb.onDataChanged();
      return;
    }

    if (matchesShortcut(e, "add_bookmark", "KeyB")) {
      e.preventDefault();
      const epId = store.audio.episode_id ?? store.inspectedEpisodeId;
      if (!epId) return;
      const posMs =
        store.audio.episode_id === epId
          ? store.audio.position_ms
          : store.episodes.find((x) => x.id === epId)?.position_ms ?? 0;
      openBookmarkDialog(epId, posMs, cb.onBookmarkSaved);
      return;
    }

    if (matchesShortcut(e, "toggle_favorite", "KeyF")) {
      e.preventDefault();
      const ids = getTargetEpisodeIds();
      if (ids.length === 0) return;
      const first = store.episodes.find((ep) => ep.id === ids[0]);
      await api.setEpisodesFavorite(ids, !(first?.is_favorite ?? false));
      cb.onDataChanged();
      return;
    }

    if (matchesShortcut(e, "remove_from_queue", "Delete")) {
      const ids = getTargetEpisodeIds();
      if (ids.length === 0) return;
      e.preventDefault();
      if (store.view === "inbox") {
        await api.setInboxState(ids, 2);
        store.selectedIds.clear();
      } else {
        await api.removeFromQueue(ids);
      }
      cb.onDataChanged();
    }
  });
}

function getTargetEpisodeIds(): number[] {
  if (store.selectedIds.size > 0) return Array.from(store.selectedIds);
  if (store.audio.episode_id !== null) return [store.audio.episode_id];
  return [];
}
