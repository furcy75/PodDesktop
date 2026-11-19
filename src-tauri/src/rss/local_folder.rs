//! Abonnements virtuels pointant vers un dossier local (livres audio, cours MP3/M4B/OGG/FLAC/WAV).
//!
//! Permet d'écouter n'importe quel dossier local de fichiers audio avec toutes les fonctions
//! de PodDesktop : reprise exacte, chapitres, signets, vitesse mémorisée par dossier, Skip Silence,
//! statistiques d'écoute, sans jamais supprimer les fichiers d'origine lors du nettoyage automatique.

use crate::audio::decoder::probe_local_audio_file;
use crate::db::episodes::UpsertEpisodeInput;
use crate::db::feeds::UpsertFeedInput;
use crate::db::Database;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

const SUPPORTED_EXTS: &[&str] = &["mp3", "m4b", "m4a", "ogg", "opus", "flac", "wav"];

/// Crée un abonnement virtuel pointant vers un dossier local puis scanne son contenu audio.
pub fn add_local_folder_feed(
    db: &Database,
    folder_path: &Path,
    custom_title: Option<&str>,
) -> Result<i64, String> {
    if !folder_path.exists() || !folder_path.is_dir() {
        return Err("Le dossier sélectionné n'existe pas ou n'est pas un répertoire".to_string());
    }

    let canonical = folder_path
        .canonicalize()
        .unwrap_or_else(|_| folder_path.to_path_buf());
    let folder_str = canonical.to_string_lossy().to_string();
    let title = custom_title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| {
            canonical
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Dossier local".to_string())
        });

    let feed_url = format!("local://{}", folder_str.replace('\\', "/"));
    let (feed_id, _) = db.upsert_feed(&UpsertFeedInput {
        title,
        feed_url,
        image_url: None,
        description: Some(format!("Dossier local : {folder_str}")),
        author: Some("Local".to_string()),
        language: None,
        is_local_folder: true,
        local_folder_path: Some(folder_str),
        etag: None,
        last_modified: None,
        funding_url: None,
        username: None,
        password_enc: None,
    })?;

    scan_local_folder_feed(db, feed_id, &canonical)?;
    Ok(feed_id)
}

/// Scanne les fichiers audio d'un dossier local et met à jour les épisodes/chapitres associés.
pub fn scan_local_folder_feed(
    db: &Database,
    feed_id: i64,
    folder_path: &Path,
) -> Result<Vec<i64>, String> {
    if !folder_path.exists() {
        return Err(format!("Dossier local introuvable : {:?}", folder_path));
    }

    let mut files = Vec::new();
    collect_audio_files(folder_path, &mut files);
    files.sort();

    let mut inputs = Vec::new();
    let mut chapters_map = Vec::new();

    for path in files {
        let path_str = path.to_string_lossy().to_string();
        let file_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Audio".to_string());
        let meta = fs::metadata(&path).ok();
        let file_size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
        let pub_date = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        let (duration_ms, chapters) = match probe_local_audio_file(&path) {
            Ok(info) => (info.duration_ms, info.chapters),
            Err(_) => (0, Vec::new()),
        };

        let guid = format!("local-file::{}", path_str.replace('\\', "/"));
        if !chapters.is_empty() {
            chapters_map.push((guid.clone(), chapters));
        }

        inputs.push(UpsertEpisodeInput {
            guid,
            title: file_name,
            pub_date,
            description: Some(path_str.clone()),
            audio_url: format!("file://{}", path_str.replace('\\', "/")),
            mime_type: guess_mime(&path),
            local_file_path: Some(path_str),
            file_size,
            duration_ms,
            transcript_url: None,
            transcript_type: None,
            transcript_data: None,
            auto_mark_played: false,
            put_in_inbox: false,
        });
    }

    let new_ids = db.upsert_episodes(feed_id, &inputs)?;

    if !chapters_map.is_empty() {
        if let Ok(eps) = db.query_episodes(&crate::models::EpisodeQuery {
            view: "feed".into(),
            feed_id: Some(feed_id),
            ..Default::default()
        }) {
            for (guid, chs) in chapters_map {
                if let Some(ep) = eps.iter().find(|e| e.guid == guid) {
                    let _ = db.replace_chapters(ep.id, &chs);
                }
            }
        }
    }

    Ok(new_ids)
}

fn collect_audio_files(dir: &Path, acc: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_audio_files(&p, acc);
            } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if SUPPORTED_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
                    acc.push(p);
                }
            }
        }
    }
}

fn guess_mime(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" => "audio/mp4",
        "ogg" => "audio/ogg",
        "opus" => "audio/opus",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        _ => "audio/mpeg",
    };
    Some(mime.to_string())
}
