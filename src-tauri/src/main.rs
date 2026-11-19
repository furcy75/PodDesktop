// Empêche l'ouverture d'une console DOS en mode release sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Point d'entrée natif de PodDesktop.
//!
//! # Portabilité stricte (Zéro trace système)
//! Avant toute initialisation de Tauri ou de la WebView (WebView2 sous Windows,
//! WebKitGTK sous Linux), nous résolvons les chemins portables relatifs à
//! l'exécutable (`std::env::current_exe()`) et nous forçons explicitement les
//! variables d'environnement de cache de la WebView vers `./data/.runtime/`.
//! Ainsi, aucune donnée n'est jamais écrite dans `%APPDATA%`, `%LOCALAPPDATA%`,
//! `~/.config`, `~/.cache` ou le registre Windows.

use poddesktop_lib::paths::PortablePaths;

fn main() {
    // 1. Résolution immédiate des chemins portables à partir du dossier de l'exécutable.
    let portable_paths = PortablePaths::initialize_at_startup()
        .expect("Impossible d'initialiser l'arborescence portable de PodDesktop");

    // 2. Isolation stricte de la WebView dans `./data/.runtime/` AVANT le démarrage de Tauri.
    portable_paths.apply_webview_isolation_env();

    // 3. Lancement du noyau applicatif Tauri v2 + moteur audio + base SQLite.
    poddesktop_lib::run(portable_paths);
}
