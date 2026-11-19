//! Client RSS asynchrone (`reqwest` rustls), cache conditionnel (`ETag`/`Last-Modified`),
//! résolution d'URL Apple Podcasts, flux privés HTTP Basic Auth et mise en cache locale des pochettes.

pub mod discovery;
pub mod local_folder;
pub mod opml;
pub mod parser;

use crate::crypto::LocalCrypto;
use crate::db::episodes::UpsertEpisodeInput;
use crate::db::feeds::{matches_feed_keywords, UpsertFeedInput};
use crate::db::Database;
use crate::models::{AppSettings, QueueInsertStrategy};
use crate::rss::parser::{parse_feed_xml, parse_podcast2_chapters_json, ParsedFeed};
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

/// Construit un client HTTP `reqwest` léger partagé (User-Agent propre, TLS pur Rust).
pub fn build_http_client() -> Client {
    Client::builder()
        .user_agent("PodDesktop/1.0 (AntennaPod-compatible desktop client)")
        .connect_timeout(Duration::from_secs(12))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .expect("Impossible de créer le client HTTP reqwest")
}

/// Résultat d'une récupération HTTP conditionnelle d'un flux RSS.
pub enum FetchFeedOutcome {
    NotModified,
    Updated {
        parsed: ParsedFeed,
        etag: Option<String>,
        last_modified: Option<String>,
        final_url: String,
    },
}

/// Télécharge et parse un flux RSS/Atom avec support HTTP Basic Auth et cache conditionnel.
pub async fn fetch_rss_feed(
    client: &Client,
    raw_url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<FetchFeedOutcome, String> {
    // Résout automatiquement les liens Apple Podcasts (`podcasts.apple.com/.../id12345`)
    let resolved_url = discovery::resolve_apple_podcasts_url(client, raw_url).await?;

    let mut req = client.get(&resolved_url);
    if let Some(tag) = etag.filter(|s| !s.is_empty()) {
        req = req.header(IF_NONE_MATCH, tag);
    }
    if let Some(lm) = last_modified.filter(|s| !s.is_empty()) {
        req = req.header(IF_MODIFIED_SINCE, lm);
    }
    if let Some(user) = username.filter(|s| !s.is_empty()) {
        req = req.basic_auth(user, password);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Erreur réseau lors de la lecture du flux : {e}"))?;

    if resp.status() == StatusCode::NOT_MODIFIED {
        return Ok(FetchFeedOutcome::NotModified);
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur RSS a retourné une erreur HTTP {}",
            resp.status()
        ));
    }

    let final_url = resp.url().to_string();
    let new_etag = resp
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let new_last_mod = resp
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let xml = resp
        .text()
        .await
        .map_err(|e| format!("Impossible de lire le corps XML du flux : {e}"))?;

    let parsed = parse_feed_xml(&xml)?;
    Ok(FetchFeedOutcome::Updated {
        parsed,
        etag: new_etag,
        last_modified: new_last_mod,
        final_url,
    })
}

/// Met en cache la pochette d'un podcast dans `./data/covers/` et retourne le chemin local.
pub async fn cache_cover_image(
    client: &Client,
    dir_covers: &Path,
    image_url: &str,
) -> Option<String> {
    if image_url.trim().is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(image_url.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let ext = if image_url.to_lowercase().contains(".png") {
        "png"
    } else if image_url.to_lowercase().contains(".webp") {
        "webp"
    } else {
        "jpg"
    };
    let dest = dir_covers.join(format!("{}.{}", &hash[..24], ext));
    if dest.exists() {
        return Some(dest.to_string_lossy().to_string());
    }

    let resp = client.get(image_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    // Limite de sécurité mémoire/disque (8 Mo max par pochette)
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return None;
    }
    if tokio::fs::write(&dest, &bytes).await.is_ok() {
        Some(dest.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Abonne l'utilisateur à un flux RSS (public ou privé), importe ses épisodes et émet `SUBSCRIBE` en synchro.
pub async fn subscribe_to_feed(
    client: &Client,
    db: &Database,
    crypto: &LocalCrypto,
    dir_covers: &Path,
    raw_url: &str,
    username: Option<String>,
    password_plain: Option<String>,
    emit_sync: bool,
) -> Result<i64, String> {
    let outcome = fetch_rss_feed(
        client,
        raw_url,
        None,
        None,
        username.as_deref(),
        password_plain.as_deref(),
    )
    .await?;

    let FetchFeedOutcome::Updated {
        parsed,
        etag,
        last_modified,
        final_url,
    } = outcome
    else {
        return Err("Flux non modifié inattendu lors d'un nouvel abonnement".to_string());
    };

    let password_enc = match password_plain.as_deref() {
        Some(p) if !p.is_empty() => Some(crypto.encrypt(p)?),
        _ => None,
    };

    let (feed_id, is_new) = db.upsert_feed(&UpsertFeedInput {
        title: parsed.title.clone(),
        feed_url: final_url.clone(),
        image_url: parsed.image_url.clone(),
        description: parsed.description.clone(),
        author: parsed.author.clone(),
        language: parsed.language.clone(),
        is_local_folder: false,
        local_folder_path: None,
        etag,
        last_modified,
        funding_url: parsed.funding_url.clone(),
        username,
        password_enc,
    })?;

    if let Some(ref img) = parsed.image_url {
        if let Some(local_cover) = cache_cover_image(client, dir_covers, img).await {
            let _ = db.set_feed_local_cover(feed_id, &local_cover);
        }
    }

    // Lors du premier abonnement, seul l'épisode le plus récent arrive dans l'Inbox
    // pour éviter de noyer l'utilisateur avec 500 anciens épisodes.
    let inputs: Vec<UpsertEpisodeInput> = parsed
        .episodes
        .iter()
        .enumerate()
        .map(|(idx, ep)| UpsertEpisodeInput {
            guid: ep.guid.clone(),
            title: ep.title.clone(),
            pub_date: ep.pub_date,
            description: ep.description.clone(),
            audio_url: ep.audio_url.clone(),
            mime_type: ep.mime_type.clone(),
            local_file_path: None,
            file_size: ep.file_size,
            duration_ms: ep.duration_ms,
            transcript_url: ep.transcript_url.clone(),
            transcript_type: ep.transcript_type.clone(),
            transcript_data: None,
            auto_mark_played: false,
            put_in_inbox: idx == 0,
        })
        .collect();

    let new_ids = db.upsert_episodes(feed_id, &inputs)?;
    store_inline_chapters(db, feed_id, &parsed);

    if is_new && emit_sync {
        let _ = db.enqueue_sync_action("", &final_url, None, "SUBSCRIBE", None, None, None);
    }

    let _ = new_ids;
    Ok(feed_id)
}

/// Rafraîchit un abonnement existant (RSS ou dossier local), applique les filtres mots-clés
/// et retourne les IDs des nouveaux épisodes détectés.
pub async fn refresh_single_feed(
    client: &Client,
    db: &Database,
    crypto: &LocalCrypto,
    dir_covers: &Path,
    settings: &AppSettings,
    feed_id: i64,
) -> Result<Vec<i64>, String> {
    let Some(feed) = db.get_feed_by_id(feed_id)? else {
        return Err("Abonnement introuvable".to_string());
    };

    if feed.is_local_folder {
        if let Some(ref folder) = feed.local_folder_path {
            return local_folder::scan_local_folder_feed(db, feed_id, Path::new(folder));
        }
        return Ok(Vec::new());
    }

    let (username, password_enc) = db.get_feed_credentials(feed_id)?;
    let password_plain = match password_enc {
        Some(ref enc) if !enc.is_empty() => Some(crypto.decrypt(enc)?),
        _ => None,
    };

    let outcome = fetch_rss_feed(
        client,
        &feed.feed_url,
        feed.etag.as_deref(),
        feed.last_modified.as_deref(),
        username.as_deref(),
        password_plain.as_deref(),
    )
    .await?;

    let FetchFeedOutcome::Updated {
        parsed,
        etag,
        last_modified,
        final_url,
    } = outcome
    else {
        return Ok(Vec::new());
    };

    db.upsert_feed(&UpsertFeedInput {
        title: parsed.title.clone(),
        feed_url: final_url,
        image_url: parsed.image_url.clone(),
        description: parsed.description.clone(),
        author: parsed.author.clone(),
        language: parsed.language.clone(),
        is_local_folder: false,
        local_folder_path: None,
        etag,
        last_modified,
        funding_url: parsed.funding_url.clone(),
        username,
        password_enc,
    })?;

    if let Some(ref img) = parsed.image_url {
        if let Some(local_cover) = cache_cover_image(client, dir_covers, img).await {
            let _ = db.set_feed_local_cover(feed_id, &local_cover);
        }
    }

    let inputs: Vec<UpsertEpisodeInput> = parsed
        .episodes
        .iter()
        .map(|ep| {
            let allowed = matches_feed_keywords(
                &ep.title,
                feed.include_keywords.as_deref(),
                feed.exclude_keywords.as_deref(),
            );
            UpsertEpisodeInput {
                guid: ep.guid.clone(),
                title: ep.title.clone(),
                pub_date: ep.pub_date,
                description: ep.description.clone(),
                audio_url: ep.audio_url.clone(),
                mime_type: ep.mime_type.clone(),
                local_file_path: None,
                file_size: ep.file_size,
                duration_ms: ep.duration_ms,
                transcript_url: ep.transcript_url.clone(),
                transcript_type: ep.transcript_type.clone(),
                transcript_data: None,
                auto_mark_played: !allowed,
                put_in_inbox: allowed,
            }
        })
        .collect();

    let new_ids = db.upsert_episodes(feed_id, &inputs)?;
    store_inline_chapters(db, feed_id, &parsed);

    // Ajoute automatiquement les nouveaux épisodes à la file si auto_download est actif sur le flux
    if !new_ids.is_empty() && (feed.auto_download || settings.auto_download_enabled) {
        let strategy = QueueInsertStrategy::from_str_or_default(&settings.queue_insert_auto);
        for &ep_id in &new_ids {
            let _ = db.enqueue_episode(ep_id, strategy);
        }
    }

    Ok(new_ids)
}

/// Télécharge à la demande les chapitres JSON Podcasting 2.0 ou la transcription d'un épisode.
pub async fn fetch_remote_chapters_if_needed(
    client: &Client,
    db: &Database,
    episode_id: i64,
    chapters_url: &str,
) -> Result<usize, String> {
    let resp = client
        .get(chapters_url)
        .send()
        .await
        .map_err(|e| format!("Erreur téléchargement chapitres : {e}"))?;
    if !resp.status().is_success() {
        return Ok(0);
    }
    let body = resp.text().await.unwrap_or_default();
    let parsed = parse_podcast2_chapters_json(&body);
    if !parsed.is_empty() {
        db.replace_chapters(episode_id, &parsed)?;
    }
    Ok(parsed.len())
}

/// Enregistre les chapitres Podlove déjà inclus dans le XML RSS.
fn store_inline_chapters(db: &Database, feed_id: i64, parsed: &ParsedFeed) {
    let Ok(eps) = db.query_episodes(&crate::models::EpisodeQuery {
        view: "feed".into(),
        feed_id: Some(feed_id),
        ..Default::default()
    }) else {
        return;
    };

    for parsed_ep in &parsed.episodes {
        if parsed_ep.inline_chapters.is_empty() {
            continue;
        }
        if let Some(db_ep) = eps.iter().find(|e| e.guid == parsed_ep.guid) {
            let _ = db.replace_chapters(db_ep.id, &parsed_ep.inline_chapters);
        }
    }
}
