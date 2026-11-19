/**
 * Moteur d'internationalisation (i18n) léger et extensible pour PodDesktop.
 *
 * - Support natif de 4 langues dès le démarrage : Français (fr), English (en), Deutsch (de), Español (es).
 * - Détection automatique de la locale OS (`navigator.language`) lorsque le réglage est `"auto"`.
 * - Changement de langue à chaud sans rechargement de la fenêtre.
 * - Interpolation `{param}` et repli sûr sur l'anglais si une clé venait à manquer.
 */
import fr from "./fr.json";
import en from "./en.json";
import de from "./de.json";
import es from "./es.json";

export type SupportedLang = "fr" | "en" | "de" | "es";

export const SUPPORTED_LANGUAGES: { code: SupportedLang; label: string }[] = [
  { code: "fr", label: "Français" },
  { code: "en", label: "English" },
  { code: "de", label: "Deutsch" },
  { code: "es", label: "Español" },
];

type Dict = Record<string, unknown>;

const DICTIONARIES: Record<SupportedLang, Dict> = { fr, en, de, es };

let activeLang: SupportedLang = detectBrowserLang();
const listeners = new Set<() => void>();

/** Détecte la langue système du navigateur/WebView parmi les 4 langues supportées. */
export function detectBrowserLang(): SupportedLang {
  const nav = (navigator.language || "fr").toLowerCase();
  if (nav.startsWith("fr")) return "fr";
  if (nav.startsWith("de")) return "de";
  if (nav.startsWith("es")) return "es";
  return "en";
}

/** Retourne le code langue actif résolu (`fr`, `en`, `de`, `es`). */
export function getActiveLang(): SupportedLang {
  return activeLang;
}

/** Applique une nouvelle langue (`"auto"` ou code ISO) et notifie l'interface. */
export function setLanguage(pref: string): void {
  const next: SupportedLang =
    pref === "auto"
      ? detectBrowserLang()
      : (["fr", "en", "de", "es"].includes(pref) ? (pref as SupportedLang) : "en");

  if (next !== activeLang) {
    activeLang = next;
    document.documentElement.lang = activeLang;
    for (const fn of listeners) fn();
  } else {
    document.documentElement.lang = activeLang;
  }
}

/** S'abonne aux changements de langue à chaud. */
export function onLanguageChange(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Résout une clé pointée (ex: `"list.markAllPlayed"`) avec interpolation `{param}`. */
export function t(key: string, params?: Record<string, string | number>): string {
  const raw = lookup(DICTIONARIES[activeLang], key) ?? lookup(DICTIONARIES.en, key) ?? key;
  if (!params) return raw;
  return raw.replace(/\{(\w+)\}/g, (_, name: string) =>
    params[name] !== undefined ? String(params[name]) : `{${name}}`
  );
}

function lookup(dict: Dict, path: string): string | undefined {
  let cur: unknown = dict;
  for (const part of path.split(".")) {
    if (cur && typeof cur === "object" && part in (cur as Dict)) {
      cur = (cur as Dict)[part];
    } else {
      return undefined;
    }
  }
  return typeof cur === "string" ? cur : undefined;
}

/** Formate une durée en millisecondes (`1:02:03` ou `12:34`). */
export function formatDurationMs(ms: number): string {
  const totalSec = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${m}:${String(s).padStart(2, "0")}`;
}

/** Formate un temps cumulé lisible pour les statistiques (`14 h 32 min`). */
export function formatHumanTime(ms: number): string {
  const totalMin = Math.max(0, Math.round(ms / 60000));
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h > 0) return `${h} h ${m} min`;
  return `${m} min`;
}

/** Formate une taille en octets selon la locale active. */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "—";
  const units = activeLang === "fr" ? ["o", "Ko", "Mo", "Go"] : ["B", "KB", "MB", "GB"];
  let val = bytes;
  let i = 0;
  while (val >= 1024 && i < units.length - 1) {
    val /= 1024;
    i++;
  }
  return `${val >= 100 ? Math.round(val) : val.toFixed(1)} ${units[i]}`;
}

/** Formate un timestamp Unix (secondes) en date courte localisée. */
export function formatDate(unixSec: number): string {
  if (!unixSec || unixSec <= 0) return "—";
  try {
    return new Intl.DateTimeFormat(activeLang, {
      year: "numeric",
      month: "short",
      day: "2-digit",
    }).format(new Date(unixSec * 1000));
  } catch {
    return new Date(unixSec * 1000).toISOString().slice(0, 10);
  }
}
