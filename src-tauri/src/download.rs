//! Gestionnaire de téléchargements parallèles avec reprise HTTP (`Range`),
//! pause/reprise individuelle, suivi de progression temps réel et respect des
//! limites de cache (par podcast et globale).

use crate::crypto::LocalCrypto;
use crate::db::Database;
use crate::models::{AppSettings, DownloadTaskInfo};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, RANGE};
use reqwest::{Client, StatusCode};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

/// Drapeau de contrôle par téléchargement (permet Pause ou Annulation instantanée).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlSignal {
    Running,
    Pause,
    Cancel,
}

/// Gestionnaire thread-safe des téléchargements hors-ligne.
pub struct DownloadManager {
    client: Client,
    semaphore: Arc<Semaphore>,
    tasks: Arc<Mutex<HashMap<i64, DownloadTaskInfo>>>,
    signals: Arc<Mutex<HashMap<i64, ControlSignal>>>,
}

impl DownloadManager {
    pub fn new(client: Client, max_concurrent: usize) -> Self {
        Self {
            client,
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            signals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Liste l'état courant de tous les téléchargements actifs/en pause/récents.
    pub fn list_tasks(&self) -> Vec<DownloadTaskInfo> {
        self.tasks
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Indique si au moins un téléchargement est actuellement en cours (pour barre cyan Windows).
    pub fn has_active_downloads(&self) -> bool {
        self.tasks
            .lock()
            .map(|m| m.values().any(|t| t.status == "downloading"))
            .unwrap_or(false)
    }

    /// Met en pause un téléchargement en conservant le fichier `.part` pour reprise ultérieure.
    pub fn pause(&self, episode_id: i64) {
        if let Ok(mut sigs) = self.signals.lock() {
            sigs.insert(episode_id, ControlSignal::Pause);
        }
        if let Ok(mut map) = self.tasks.lock() {
            if let Some(t) = map.get_mut(&episode_id) {
                t.status = "paused".to_string();
            }
        }
    }

    /// Annule un téléchargement et demande la suppression du fichier `.part`.
    pub fn cancel(&self, episode_id: i64) {
        if let Ok(mut sigs) = self.signals.lock() {
            sigs.insert(episode_id, ControlSignal::Cancel);
        }
        if let Ok(mut map) = self.tasks.lock() {
            if let Some(t) = map.get_mut(&episode_id) {
                t.status = "cancelled".to_string();
            }
        }
    }

    /// Lance (ou reprend depuis le fichier `.part`) le téléchargement d'un épisode.
    pub async fn start_or_resume<F>(
        &self,
        db: Arc<Database>,
        crypto: Arc<LocalCrypto>,
        dir_media: PathBuf,
        settings: AppSettings,
        episode_id: i64,
        on_update: F,
    ) -> Result<PathBuf, String>
    where
        F: Fn(DownloadTaskInfo) + Send + Sync + 'static,
    {
        let ep = db
            .get_episode_by_id(episode_id)?
            .ok_or_else(|| format!("Épisode introuvable ({episode_id})"))?;

        // Si le fichier existe déjà sur disque, on le retourne immédiatement
        if let Some(ref existing) = ep.local_file_path {
            let p = PathBuf::from(existing);
            if p.exists() {
                return Ok(p);
            }
        }

        let ext = guess_extension(&ep.audio_url, ep.mime_type.as_deref());
        let feed_dir = dir_media.join(format!("feed_{}", ep.feed_id));
        tokio::fs::create_dir_all(&feed_dir)
            .await
            .map_err(|e| format!("Impossible de créer le dossier média : {e}"))?;

        let final_path = feed_dir.join(format!("ep_{episode_id}.{ext}"));
        let part_path = feed_dir.join(format!("ep_{episode_id}.{ext}.part"));

        let initial_bytes = tokio::fs::metadata(&part_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let mut info = DownloadTaskInfo {
            episode_id,
            title: ep.title.clone(),
            feed_title: ep.feed_title.clone(),
            downloaded_bytes: initial_bytes,
            total_bytes: if ep.file_size > 0 {
                Some(ep.file_size as u64)
            } else {
                None
            },
            status: "queued".to_string(),
            error: None,
        };

        self.set_task(info.clone());
        if let Ok(mut sigs) = self.signals.lock() {
            sigs.insert(episode_id, ControlSignal::Running);
        }
        on_update(info.clone());

        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| "Sémaphore de téléchargement fermé".to_string())?;

        // Vérifie si l'utilisateur a annulé ou mis en pause pendant l'attente du slot
        if self.get_signal(episode_id) != ControlSignal::Running {
            return Err("Téléchargement interrompu avant démarrage".to_string());
        }

        info.status = "downloading".to_string();
        self.set_task(info.clone());
        on_update(info.clone());

        // Support des flux privés HTTP Basic Auth pour les enclosures protégées
        let (username, pwd_enc) = db.get_feed_credentials(ep.feed_id)?;
        let pwd_plain = match pwd_enc {
            Some(ref enc) if !enc.is_empty() => crypto.decrypt(enc).ok(),
            _ => None,
        };

        let mut req = self.client.get(&ep.audio_url);
        if initial_bytes > 0 {
            req = req.header(RANGE, format!("bytes={initial_bytes}-"));
        }
        if let Some(ref u) = username.filter(|s| !s.is_empty()) {
            req = req.basic_auth(u, pwd_plain.as_deref());
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                info.status = "failed".to_string();
                info.error = Some(e.to_string());
                self.set_task(info.clone());
                on_update(info);
                return Err(format!("Erreur réseau téléchargement : {e}"));
            }
        };

        if !resp.status().is_success() && resp.status() != StatusCode::PARTIAL_CONTENT {
            let err_msg = format!("HTTP {}", resp.status());
            info.status = "failed".to_string();
            info.error = Some(err_msg.clone());
            self.set_task(info.clone());
            on_update(info);
            return Err(err_msg);
        }

        // Si le serveur ignore Range et renvoie 200 OK, on réécrit depuis 0
        let resuming = resp.status() == StatusCode::PARTIAL_CONTENT && initial_bytes > 0;
        let content_len = resp
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        if resuming {
            info.downloaded_bytes = initial_bytes;
            if let Some(cl) = content_len {
                info.total_bytes = Some(initial_bytes + cl);
            }
        } else {
            info.downloaded_bytes = 0;
            if let Some(cl) = content_len {
                info.total_bytes = Some(cl);
            }
        }

        let mut file = if resuming {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&part_path)
                .await
                .map_err(|e| format!("Erreur ouverture .part : {e}"))?
        } else {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&part_path)
                .await
                .map_err(|e| format!("Erreur création .part : {e}"))?
        };

        let mut stream = resp.bytes_stream();
        let mut last_notify = std::time::Instant::now();

        while let Some(chunk_res) = stream.next().await {
            match self.get_signal(episode_id) {
                ControlSignal::Pause => {
                    let _ = file.flush().await;
                    info.status = "paused".to_string();
                    self.set_task(info.clone());
                    on_update(info);
                    return Err("Téléchargement mis en pause".to_string());
                }
                ControlSignal::Cancel => {
                    drop(file);
                    let _ = tokio::fs::remove_file(&part_path).await;
                    info.status = "cancelled".to_string();
                    self.set_task(info.clone());
                    on_update(info);
                    return Err("Téléchargement annulé".to_string());
                }
                ControlSignal::Running => {}
            }

            let chunk = chunk_res.map_err(|e| format!("Erreur flux téléchargement : {e}"))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| format!("Erreur écriture disque : {e}"))?;
            info.downloaded_bytes += chunk.len() as u64;

            if last_notify.elapsed() >= std::time::Duration::from_millis(200) {
                self.set_task(info.clone());
                on_update(info.clone());
                last_notify = std::time::Instant::now();
            }
        }

        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);

        // Renommage atomique `.part` -> fichier final
        if final_path.exists() {
            let _ = tokio::fs::remove_file(&final_path).await;
        }
        tokio::fs::rename(&part_path, &final_path)
            .await
            .map_err(|e| format!("Erreur finalisation fichier téléchargé : {e}"))?;

        let final_size = info.downloaded_bytes as i64;
        db.set_local_file_path(
            episode_id,
            Some(&final_path.to_string_lossy()),
            Some(final_size),
        )?;

        // Action DOWNLOAD optionnelle vers AntennaPod (uniquement si activée dans Paramètres)
        if settings.sync_download_delete {
            if let Ok(Some(feed)) = db.get_feed_by_id(ep.feed_id) {
                if !feed.is_local_folder {
                    let _ = db.enqueue_sync_action(
                        &ep.audio_url,
                        &feed.feed_url,
                        Some(&ep.guid),
                        "DOWNLOAD",
                        None,
                        None,
                        None,
                    );
                }
            }
        }

        info.status = "completed".to_string();
        info.total_bytes = Some(info.downloaded_bytes);
        self.set_task(info.clone());
        on_update(info);

        // Applique les limites de cache configurées (max_kept_episodes et cache global)
        enforce_cache_limits(&db, &dir_media, &settings, ep.feed_id);

        Ok(final_path)
    }

    fn set_task(&self, info: DownloadTaskInfo) {
        if let Ok(mut map) = self.tasks.lock() {
            map.insert(info.episode_id, info);
        }
    }

    fn get_signal(&self, episode_id: i64) -> ControlSignal {
        self.signals
            .lock()
            .ok()
            .and_then(|m| m.get(&episode_id).copied())
            .unwrap_or(ControlSignal::Running)
    }
}

/// Applique la rétention par podcast (`max_kept_episodes`) et la limite globale de cache disque,
/// en protégeant toujours les Favoris si l'option est active et les dossiers locaux virtuels.
pub fn enforce_cache_limits(
    db: &Database,
    dir_media: &Path,
    settings: &AppSettings,
    feed_id: i64,
) {
    // 1. Limite par podcast
    if let Ok(Some(feed)) = db.get_feed_by_id(feed_id) {
        if !feed.is_local_folder {
            let limit = feed
                .max_kept_episodes
                .unwrap_or(settings.default_max_kept_episodes);
            if limit > 0 {
                if let Ok(eps) = db.query_episodes(&crate::models::EpisodeQuery {
                    view: "feed".into(),
                    feed_id: Some(feed_id),
                    only_downloaded: true,
                    sort_by: Some("pub_date".into()),
                    sort_asc: false,
                    ..Default::default()
                }) {
                    for ep in eps.into_iter().skip(limit as usize) {
                        if settings.protect_favorites_from_delete && ep.is_favorite {
                            continue;
                        }
                        if let Some(ref p) = ep.local_file_path {
                            let _ = std::fs::remove_file(p);
                            let _ = db.set_local_file_path(ep.id, None, None);
                        }
                    }
                }
            }
        }
    }

    // 2. Limite globale du cache hors-ligne
    if settings.global_cache_limit_mb > 0 {
        let max_bytes = settings.global_cache_limit_mb * 1024 * 1024;
        let mut current = crate::paths::PortablePaths::directory_size_bytes(dir_media);
        if current > max_bytes {
            if let Ok(mut eps) = db.query_episodes(&crate::models::EpisodeQuery {
                view: "downloads".into(),
                sort_by: Some("pub_date".into()),
                sort_asc: true, // Plus anciens d'abord
                ..Default::default()
            }) {
                // Priorité aux épisodes déjà lus avant les non lus
                eps.sort_by_key(|e| (!e.is_played, e.pub_date));
                for ep in eps {
                    if current <= max_bytes {
                        break;
                    }
                    if settings.protect_favorites_from_delete && ep.is_favorite {
                        continue;
                    }
                    if let Ok(Some(f)) = db.get_feed_by_id(ep.feed_id) {
                        if f.is_local_folder {
                            continue;
                        }
                    }
                    if let Some(ref p) = ep.local_file_path {
                        if let Ok(meta) = std::fs::metadata(p) {
                            current = current.saturating_sub(meta.len());
                        }
                        let _ = std::fs::remove_file(p);
                        let _ = db.set_local_file_path(ep.id, None, None);
                    }
                }
            }
        }
    }
}

fn guess_extension(url: &str, mime: Option<&str>) -> &'static str {
    let lower = url.to_lowercase();
    if lower.contains(".m4a") {
        return "m4a";
    }
    if lower.contains(".m4b") {
        return "m4b";
    }
    if lower.contains(".ogg") {
        return "ogg";
    }
    if lower.contains(".opus") {
        return "opus";
    }
    if lower.contains(".flac") {
        return "flac";
    }
    if lower.contains(".wav") {
        return "wav";
    }
    if let Some(m) = mime {
        if m.contains("mp4") || m.contains("aac") {
            return "m4a";
        }
        if m.contains("ogg") {
            return "ogg";
        }
    }
    "mp3"
}
