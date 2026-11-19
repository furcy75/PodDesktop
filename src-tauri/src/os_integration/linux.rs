//! Intégration native Linux (MPRIS2 D-Bus pour GNOME, KDE Plasma, XFCE, Cinnamon).
//!
//! Le crate `souvlaki` expose automatiquement l'interface standard
//! `org.mpris.MediaPlayer2.poddesktop` sur le bus de session D-Bus (contrôles
//! Lecture/Pause, Suivant, Précédent, Seek, Pochette et Métadonnées directement
//! dans la barre système / panneau supérieur).

pub fn log_mpris_ready() {
    log::info!("Intégration MPRIS2 D-Bus active (org.mpris.MediaPlayer2.poddesktop)");
}
