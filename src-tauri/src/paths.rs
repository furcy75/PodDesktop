//! Gestion de l'arborescence 100 % portable de PodDesktop.
//!
//! Tous les chemins par défaut sont résolus relativement au dossier de l'exécutable
//! (`std::env::current_exe()`). Le fichier `bootstrap.json` placé à côté du binaire
//! indique l'emplacement de la base SQLite (`./data/poddesktop.db` par défaut),
//! permettant à l'utilisateur de déplacer ses données ou de basculer entre chemins
//! relatifs (portables) et absolus depuis les Paramètres.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration minimale stockée dans `bootstrap.json` à côté de l'exécutable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Chemin relatif (ex: `./data/poddesktop.db`) ou absolu vers la base SQLite.
    pub db_path: String,
    /// Chemin optionnel personnalisé pour le dossier de données racine.
    #[serde(default)]
    pub dir_data: Option<String>,
    /// Chemin optionnel personnalisé pour les fichiers audio téléchargés.
    #[serde(default)]
    pub dir_media: Option<String>,
    /// Chemin optionnel personnalisé pour les pochettes en cache.
    #[serde(default)]
    pub dir_covers: Option<String>,
    /// Chemin optionnel personnalisé pour les exports (OPML, Favoris, sauvegardes).
    #[serde(default)]
    pub dir_exports: Option<String>,
    /// Indique si l'utilisateur préfère enregistrer les chemins en relatif (portable).
    #[serde(default = "default_true")]
    pub prefer_relative_paths: bool,
}

fn default_true() -> bool {
    true
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            db_path: "./data/poddesktop.db".to_string(),
            dir_data: Some("./data".to_string()),
            dir_media: Some("./data/media".to_string()),
            dir_covers: Some("./data/covers".to_string()),
            dir_exports: Some("./data/exports".to_string()),
            prefer_relative_paths: true,
        }
    }
}

/// Ensemble des chemins résolus sur le disque pour la session courante.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortablePaths {
    pub exe_dir: PathBuf,
    pub bootstrap_file: PathBuf,
    pub db_path: PathBuf,
    pub dir_data: PathBuf,
    pub dir_media: PathBuf,
    pub dir_covers: PathBuf,
    pub dir_exports: PathBuf,
    pub dir_runtime: PathBuf,
    pub prefer_relative_paths: bool,
}

impl PortablePaths {
    /// Initialise les chemins au démarrage à partir du dossier de l'exécutable.
    /// Crée automatiquement `bootstrap.json` et l'arborescence `./data/*` si absents.
    pub fn initialize_at_startup() -> Result<Self, String> {
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Impossible de localiser l'exécutable courant : {e}"))?;
        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| "Dossier parent de l'exécutable introuvable".to_string())?
            .to_path_buf();

        Self::from_base_dir(exe_dir)
    }

    /// Variante testable permettant d'initialiser l'arborescence dans n'importe quel dossier.
    pub fn from_base_dir(exe_dir: PathBuf) -> Result<Self, String> {
        let bootstrap_file = exe_dir.join("bootstrap.json");

        // Lecture ou création du fichier bootstrap.json
        let config = if bootstrap_file.exists() {
            let content = fs::read_to_string(&bootstrap_file)
                .map_err(|e| format!("Erreur lecture bootstrap.json : {e}"))?;
            serde_json::from_str::<BootstrapConfig>(&content).unwrap_or_default()
        } else {
            let default_cfg = BootstrapConfig::default();
            // On essaie d'écrire bootstrap.json ; en cas de dossier en lecture seule,
            // on continue avec les valeurs par défaut sans bloquer le lancement.
            if let Ok(json) = serde_json::to_string_pretty(&default_cfg) {
                let _ = fs::write(&bootstrap_file, json);
            }
            default_cfg
        };

        let db_path = resolve_path(&exe_dir, &config.db_path);
        let dir_data = config
            .dir_data
            .as_deref()
            .map(|p| resolve_path(&exe_dir, p))
            .unwrap_or_else(|| {
                db_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| exe_dir.join("data"))
            });
        let dir_media = config
            .dir_media
            .as_deref()
            .map(|p| resolve_path(&exe_dir, p))
            .unwrap_or_else(|| dir_data.join("media"));
        let dir_covers = config
            .dir_covers
            .as_deref()
            .map(|p| resolve_path(&exe_dir, p))
            .unwrap_or_else(|| dir_data.join("covers"));
        let dir_exports = config
            .dir_exports
            .as_deref()
            .map(|p| resolve_path(&exe_dir, p))
            .unwrap_or_else(|| dir_data.join("exports"));
        let dir_runtime = dir_data.join(".runtime");

        let paths = Self {
            exe_dir,
            bootstrap_file,
            db_path,
            dir_data,
            dir_media,
            dir_covers,
            dir_exports,
            dir_runtime,
            prefer_relative_paths: config.prefer_relative_paths,
        };

        paths.ensure_directories()?;
        Ok(paths)
    }

    /// Crée tous les dossiers nécessaires de l'arborescence portable.
    pub fn ensure_directories(&self) -> Result<(), String> {
        let dirs = [
            &self.dir_data,
            &self.dir_media,
            &self.dir_covers,
            &self.dir_exports,
            &self.dir_runtime,
        ];
        for dir in dirs {
            fs::create_dir_all(dir)
                .map_err(|e| format!("Impossible de créer le dossier {:?} : {e}", dir))?;
        }
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Impossible de créer le dossier DB {:?} : {e}", parent))?;
        }
        Ok(())
    }

    /// Force les variables d'environnement WebView2 (Windows) et XDG (Linux) vers `./data/.runtime/`.
    /// Doit être appelé AVANT le démarrage de la boucle Tauri.
    pub fn apply_webview_isolation_env(&self) {
        #[cfg(target_os = "windows")]
        {
            let webview2_dir = self.dir_runtime.join("webview2");
            let _ = fs::create_dir_all(&webview2_dir);
            // SAFETY: exécuté en monothread au tout début de main(), avant Tauri/WebView2.
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview2_dir);
        }

        #[cfg(target_os = "linux")]
        {
            let xdg_cache = self.dir_runtime.join("cache");
            let xdg_data = self.dir_runtime.join("share");
            let xdg_config = self.dir_runtime.join("config");
            let _ = fs::create_dir_all(&xdg_cache);
            let _ = fs::create_dir_all(&xdg_data);
            let _ = fs::create_dir_all(&xdg_config);
            std::env::set_var("XDG_CACHE_HOME", &xdg_cache);
            std::env::set_var("XDG_DATA_HOME", &xdg_data);
            std::env::set_var("XDG_CONFIG_HOME", &xdg_config);
        }
    }

    /// Met à jour et sauvegarde `bootstrap.json` (avec conversion relatif/absolu selon préférence).
    pub fn save_bootstrap(&self) -> Result<(), String> {
        let format_path = |p: &Path| -> String {
            if self.prefer_relative_paths {
                make_relative_if_possible(&self.exe_dir, p)
            } else {
                p.to_string_lossy().to_string()
            }
        };

        let cfg = BootstrapConfig {
            db_path: format_path(&self.db_path),
            dir_data: Some(format_path(&self.dir_data)),
            dir_media: Some(format_path(&self.dir_media)),
            dir_covers: Some(format_path(&self.dir_covers)),
            dir_exports: Some(format_path(&self.dir_exports)),
            prefer_relative_paths: self.prefer_relative_paths,
        };

        let json = serde_json::to_string_pretty(&cfg)
            .map_err(|e| format!("Erreur sérialisation bootstrap.json : {e}"))?;
        fs::write(&self.bootstrap_file, json)
            .map_err(|e| format!("Erreur écriture bootstrap.json : {e}"))?;
        Ok(())
    }

    /// Met à jour un ou plusieurs dossiers de stockage et persiste `bootstrap.json`.
    pub fn update_directories(
        &mut self,
        dir_data: Option<String>,
        dir_media: Option<String>,
        dir_covers: Option<String>,
        dir_exports: Option<String>,
        prefer_relative: Option<bool>,
    ) -> Result<(), String> {
        if let Some(rel) = prefer_relative {
            self.prefer_relative_paths = rel;
        }
        if let Some(d) = dir_data.filter(|s| !s.trim().is_empty()) {
            self.dir_data = resolve_path(&self.exe_dir, &d);
            self.db_path = self.dir_data.join("poddesktop.db");
            self.dir_runtime = self.dir_data.join(".runtime");
        }
        if let Some(d) = dir_media.filter(|s| !s.trim().is_empty()) {
            self.dir_media = resolve_path(&self.exe_dir, &d);
        }
        if let Some(d) = dir_covers.filter(|s| !s.trim().is_empty()) {
            self.dir_covers = resolve_path(&self.exe_dir, &d);
        }
        if let Some(d) = dir_exports.filter(|s| !s.trim().is_empty()) {
            self.dir_exports = resolve_path(&self.exe_dir, &d);
        }
        self.ensure_directories()?;
        self.save_bootstrap()
    }

    /// Nettoie le cache WebView (`.runtime`) en ignorant les fichiers verrouillés en cours d'usage.
    pub fn clear_webview_cache(&self) -> Result<u64, String> {
        let mut freed_bytes = 0u64;
        if self.dir_runtime.exists() {
            freed_bytes = remove_dir_contents_best_effort(&self.dir_runtime);
        }
        Ok(freed_bytes)
    }

    /// Calcule la taille totale occupée par un dossier sur le disque (en octets).
    pub fn directory_size_bytes(path: &Path) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    total += Self::directory_size_bytes(&p);
                } else if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
        total
    }
}

/// Résout un chemin relatif par rapport au dossier de l'exécutable, ou retourne le chemin absolu tel quel.
pub fn resolve_path(base: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        candidate
    } else {
        clean_path(&base.join(candidate))
    }
}

/// Convertit un chemin absolu en chemin relatif `./...` s'il se trouve sous `base`.
pub fn make_relative_if_possible(base: &Path, target: &Path) -> String {
    if let Ok(rel) = target.strip_prefix(base) {
        let normalized = rel.to_string_lossy().replace('\\', "/");
        if normalized.is_empty() {
            ".".to_string()
        } else {
            format!("./{normalized}")
        }
    } else {
        target.to_string_lossy().to_string()
    }
}

/// Normalise les segments `.` et `..` d'un chemin sans exiger son existence sur disque.
fn clean_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Supprime au mieux les fichiers d'un dossier en ignorant les fichiers verrouillés par WebView2.
fn remove_dir_contents_best_effort(dir: &Path) -> u64 {
    let mut freed = 0u64;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                freed += remove_dir_contents_best_effort(&p);
                let _ = fs::remove_dir(&p);
            } else if let Ok(meta) = entry.metadata() {
                let len = meta.len();
                if fs::remove_file(&p).is_ok() {
                    freed += len;
                }
            }
        }
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_paths_stay_strictly_inside_base_dir() {
        let tmp = std::env::temp_dir().join(format!(
            "poddesktop_portable_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let paths = PortablePaths::from_base_dir(tmp.clone()).expect("init portable paths");
        assert!(paths.bootstrap_file.exists());
        assert!(paths.dir_data.starts_with(&tmp));
        assert!(paths.dir_media.starts_with(&tmp));
        assert!(paths.dir_covers.starts_with(&tmp));
        assert!(paths.dir_exports.starts_with(&tmp));
        assert!(paths.dir_runtime.starts_with(&tmp));
        assert_eq!(
            make_relative_if_possible(&tmp, &paths.db_path),
            "./data/poddesktop.db"
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}

