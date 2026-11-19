/**
 * Bibliothèque d'icônes SVG vectorielles nettes (style Lucide, zéro dépendance externe).
 * Garantit un contraste élevé et un rendu parfaitement net en HiDPI sur Windows & Linux.
 */

function svg(body: string, size = 16, cls = "svg-icon"): string {
  return `<svg class="${cls}" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
}

export const icons = {
  // Barre supérieure & Actions principales
  search: (s = 15) => svg('<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>', s),
  refresh: (s = 17) =>
    svg(
      '<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 16h5v5"/>',
      s
    ),
  sync: (s = 17) =>
    svg(
      '<path d="M17 2.1l4 4-4 4"/><path d="M3 12.2v-2a4 4 0 0 1 4-4h12.8"/><path d="M7 21.9l-4-4 4-4"/><path d="M21 11.8v2a4 4 0 0 1-4 4H4.2"/>',
      s
    ),
  miniPlayer: (s = 17) =>
    svg(
      '<rect x="2" y="4" width="20" height="16" rx="2"/><rect x="12" y="12" width="7" height="5" rx="1" fill="currentColor" stroke="none"/>',
      s
    ),
  expandWindow: (s = 16) =>
    svg(
      '<path d="M15 3h6v6"/><path d="M9 21H3v-6"/><path d="M21 3l-7 7"/><path d="M3 21l7-7"/>',
      s
    ),
  settings: (s = 17) =>
    svg(
      '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>',
      s
    ),
  podcast: (s = 16) =>
    svg(
      '<path d="M16.85 18.58a9 9 0 1 0-9.7 0"/><path d="M8 14a5 5 0 1 1 8 0"/><circle cx="12" cy="11" r="1.2" fill="currentColor"/><path d="M13 17a1 1 0 1 0-2 0l.5 4.5a.5.5 0 1 0 1 0Z"/>',
      s
    ),

  // Navigation Sidebar
  queue: (s = 16) =>
    svg(
      '<path d="M3 6h18"/><path d="M3 12h12"/><path d="M3 18h8"/><path d="m17 14 4 3-4 3v-6Z" fill="currentColor"/>',
      s
    ),
  inbox: (s = 16) =>
    svg(
      '<polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>',
      s
    ),
  allEpisodes: (s = 16) =>
    svg('<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><circle cx="4" cy="6" r="1" fill="currentColor"/><circle cx="4" cy="12" r="1" fill="currentColor"/><circle cx="4" cy="18" r="1" fill="currentColor"/>', s),
  download: (s = 16) =>
    svg('<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>', s),
  star: (s = 16, filled = false) =>
    svg(
      `<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"${filled ? ' fill="currentColor"' : ""}/>`,
      s
    ),
  history: (s = 16) =>
    svg('<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M12 7v5l4 2"/>', s),
  discover: (s = 16) =>
    svg('<circle cx="12" cy="12" r="10"/><polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"/>', s),
  stats: (s = 16) =>
    svg('<line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/>', s),
  folder: (s = 15) =>
    svg('<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>', s),
  plus: (s = 15) => svg('<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>', s),

  // Contrôles lecteur & sections Paramètres
  play: (s = 16) => svg('<polygon points="6 4 20 12 6 20 6 4" fill="currentColor" stroke="none"/>', s),
  pause: (s = 16) =>
    svg('<rect x="6" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none"/><rect x="14" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none"/>', s),
  next: (s = 16) =>
    svg('<polygon points="5 4 15 12 5 20 5 4" fill="currentColor" stroke="none"/><line x1="19" y1="5" x2="19" y2="19"/>', s),
  check: (s = 16) => svg('<polyline points="20 6 9 17 4 12"/>', s),
  moon: (s = 15) => svg('<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>', s),
  volume: (s = 16) =>
    svg('<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>', s),
  scissors: (s = 15) =>
    svg('<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><line x1="20" y1="4" x2="8.12" y2="15.88"/><line x1="14.47" y1="14.48" x2="20" y2="20"/><line x1="8.12" y1="8.12" x2="12" y2="12"/>', s),
  mic: (s = 15) =>
    svg('<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/><line x1="8" y1="23" x2="16" y2="23"/>', s),
  sliders: (s = 16) =>
    svg('<line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/><line x1="1" y1="14" x2="7" y2="14"/><line x1="9" y1="8" x2="15" y2="8"/><line x1="17" y1="16" x2="23" y2="16"/>', s),
  palette: (s = 16) =>
    svg('<circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"/>', s),
  database: (s = 16) =>
    svg('<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>', s),
  globe: (s = 16) =>
    svg('<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>', s),
  cloud: (s = 16) =>
    svg('<path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z"/>', s),
  shield: (s = 16) =>
    svg('<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>', s),
  keyboard: (s = 16) =>
    svg('<rect x="2" y="4" width="20" height="16" rx="2"/><path d="M6 8h.001"/><path d="M10 8h.001"/><path d="M14 8h.001"/><path d="M18 8h.001"/><path d="M8 12h.001"/><path d="M12 12h.001"/><path d="M16 12h.001"/><path d="M7 16h10"/>', s),
};

/**
 * Drapeaux SVG vectoriels nets (rendu couleur garanti sous Windows 10/11 et Linux,
 * contrairement aux emojis drapeaux affichés en lettres sur Windows).
 */
export function flagIcon(code: string, width = 18, height = 13): string {
  const common = `class="flag-svg" width="${width}" height="${height}" viewBox="0 0 24 16" aria-hidden="true"`;
  switch (code) {
    case "fr":
      return `<svg ${common}><rect width="8" height="16" fill="#0055A4"/><rect x="8" width="8" height="16" fill="#FFFFFF"/><rect x="16" width="8" height="16" fill="#EF4135"/></svg>`;
    case "es":
      return `<svg ${common}><rect width="24" height="16" fill="#AA151B"/><rect y="4" width="24" height="8" fill="#F1BF00"/><rect x="5" y="6" width="3" height="4" rx="0.6" fill="#AA151B"/></svg>`;
    case "de":
      return `<svg ${common}><rect width="24" height="5.34" fill="#111111"/><rect y="5.33" width="24" height="5.34" fill="#DD0000"/><rect y="10.66" width="24" height="5.34" fill="#FFCE00"/></svg>`;
    case "en":
    default:
      return `<svg ${common}><rect width="24" height="16" fill="#012169"/><path d="M0 0L24 16M24 0L0 16" stroke="#FFFFFF" stroke-width="3"/><path d="M0 0L24 16M24 0L0 16" stroke="#C8102E" stroke-width="1.4"/><path d="M12 0V16M0 8H24" stroke="#FFFFFF" stroke-width="5"/><path d="M12 0V16M0 8H24" stroke="#C8102E" stroke-width="2.8"/></svg>`;
  }
}
