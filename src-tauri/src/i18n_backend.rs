//! Dictionnaire multilingue côté backend Rust (fr, en, de, es).
//!
//! Utilisé exclusivement pour les surfaces natives OS qui ne dépendent pas de la WebView :
//! - Menu contextuel de l'icône System Tray
//! - Infobulles des boutons de la miniature barre des tâches Windows (Thumbnail Toolbar)
//! - Libellés natifs de synchronisation / erreurs système

/// Codes de langues supportés dès le démarrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendLang {
    Fr,
    En,
    De,
    Es,
}

impl BackendLang {
    /// Convertit un code langue (ex: "fr", "en-US", "de_DE", "es") en `BackendLang`.
    pub fn from_code(code: &str) -> Self {
        let lower = code.trim().to_ascii_lowercase();
        if lower.starts_with("fr") {
            Self::Fr
        } else if lower.starts_with("de") {
            Self::De
        } else if lower.starts_with("es") {
            Self::Es
        } else {
            Self::En
        }
    }

    /// Code ISO 639-1 canonique.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Fr => "fr",
            Self::En => "en",
            Self::De => "de",
            Self::Es => "es",
        }
    }

    /// Détecte la langue par défaut de l'OS sans dépendance externe lourde.
    pub fn detect_system() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                if !val.trim().is_empty() {
                    return Self::from_code(&val);
                }
            }
        }
        #[cfg(windows)]
        {
            // Sous Windows, la détection fine peut aussi être affinée par navigator.language côté UI,
            // avec fallback propre sur "fr" ou "en".
            return Self::Fr;
        }
        #[cfg(not(windows))]
        {
            Self::En
        }
    }
}

/// Traductions des actions natives OS (System Tray & Taskbar).
pub struct NativeStrings {
    pub show_window: &'static str,
    pub play_pause: &'static str,
    pub next_episode: &'static str,
    pub mark_played: &'static str,
    pub speed_menu: &'static str,
    pub mini_player: &'static str,
    pub quit: &'static str,
}

pub fn native_strings(lang: BackendLang) -> NativeStrings {
    match lang {
        BackendLang::Fr => NativeStrings {
            show_window: "Ouvrir PodDesktop",
            play_pause: "Lecture / Pause",
            next_episode: "Épisode suivant",
            mark_played: "Marquer comme lu",
            speed_menu: "Vitesse de lecture",
            mini_player: "Mode Mini-Lecteur",
            quit: "Quitter",
        },
        BackendLang::En => NativeStrings {
            show_window: "Open PodDesktop",
            play_pause: "Play / Pause",
            next_episode: "Next Episode",
            mark_played: "Mark as Played",
            speed_menu: "Playback Speed",
            mini_player: "Mini-Player Mode",
            quit: "Quit",
        },
        BackendLang::De => NativeStrings {
            show_window: "PodDesktop öffnen",
            play_pause: "Wiedergabe / Pause",
            next_episode: "Nächste Folge",
            mark_played: "Als gehört markieren",
            speed_menu: "Wiedergabegeschwindigkeit",
            mini_player: "Mini-Player-Modus",
            quit: "Beenden",
        },
        BackendLang::Es => NativeStrings {
            show_window: "Abrir PodDesktop",
            play_pause: "Reproducir / Pausar",
            next_episode: "Siguiente episodio",
            mark_played: "Marcar como reproducido",
            speed_menu: "Velocidad de reproducción",
            mini_player: "Modo Mini-Reproductor",
            quit: "Salir",
        },
    }
}
