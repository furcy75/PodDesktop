//! Orchestrateur de synchronisation bidirectionnelle PC ↔ Mobile (100 % compatible AntennaPod).
//!
//! Implémente les règles strictes de la section 6 du Cahier des Charges v4.2 :
//! - Support natif **Nextcloud GPodder Sync** (avec Login Flow v2) et **gpodder API v2**
//!   (`gpodder.net`, `oPodSync`, `goPodder`, `Micro-GPodder`, `PodFetch`).
//! - Normalisation canonique des URLs (`feed_url` / `audio_url`) pour résoudre les
//!   redirections publicitaires dynamiques (Megaphone, Acast, Podtrac, Chartable...).
//! - Mapping exact `PLAY` (`started`, `position`, `total`, ISO-8601 UTC), `NEW`,
//!   et optionnellement `DOWNLOAD` / `DELETE`.
//! - Résolution de conflits *last-write-wins* par horodatage UTC.

pub mod gpodder;
pub mod nextcloud;

use crate::crypto::LocalCrypto;
use crate::db::Database;
use crate::models::{AppSettings, SyncQueueItem, SyncReport};
use crate::rss::subscribe_to_feed;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use url::Url;

/// Action d'épisode normalisée échangée avec Nextcloud GPodder Sync ou gpodder v2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEpisodeAction {
    pub podcast: String,
    pub episode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    /// "play", "new", "download", "delete" (normalisé en majuscules/minuscules selon serveur)
    pub action: String,
    /// UTC ISO-8601 (`YYYY-MM-DDTHH:MM:SSZ`)
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// Delta d'abonnements renvoyé par le serveur distant.
#[derive(Debug, Clone, Default)]
pub struct RemoteSubscriptionsDelta {
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub timestamp: i64,
}

/// Normalise une URL de flux ou d'épisode pour fiabiliser la correspondance PC ↔ AntennaPod
/// malgré les paramètres de tracking (`utm_*`, `fbclid`) ou préfixes de régies publicitaires.
pub fn normalize_url_for_sync(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Déballe les préfixes connus de redirection statistique/publicitaire (Podtrac, Chartable, OP3...)
    let unwrapped = unwrap_tracking_prefix(trimmed);

    let Ok(mut parsed) = Url::parse(&unwrapped) else {
        return unwrapped.to_lowercase();
    };

    // Unifie http/https en https pour la comparaison canonique, supprime le port par défaut et les utm_*
    let _ = parsed.set_scheme("https");
    let filtered_query: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| {
            let kl = k.to_ascii_lowercase();
            !kl.starts_with("utm_") && kl != "fbclid" && kl != "gclid" && kl != "dest"
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if filtered_query.is_empty() {
        parsed.set_query(None);
    } else {
        let mut ser = parsed.query_pairs_mut();
        ser.clear();
        for (k, v) in filtered_query {
            ser.append_pair(&k, &v);
        }
    }
    parsed.set_fragment(None);

    let mut out = parsed.to_string();
    while out.ends_with('/') && out.len() > 9 {
        out.pop();
    }
    out
}

fn unwrap_tracking_prefix(url: &str) -> String {
    let prefixes = [
        "https://dts.podtrac.com/redirect.mp3/",
        "http://dts.podtrac.com/redirect.mp3/",
        "https://dts.podtrac.com/redirect.m4a/",
        "http://dts.podtrac.com/redirect.m4a/",
        "https://chrt.fm/track/",
        "https://chtbl.com/track/",
        "https://op3.dev/e/",
        "https://pdst.fm/e/",
        "https://pscrb.fm/rss/p/",
        "https://prfx.byspotify.com/e/",
        "https://swap.fm/track/",
    ];
    let mut current = url.to_string();
    // Déballe en boucle car certains flux empilent 2 régies (ex: op3.dev + podtrac)
    for _ in 0..3 {
        let mut changed = false;
        for p in prefixes {
            if let Some(rest) = current.strip_prefix(p) {
                changed = true;
                if rest.starts_with("http://") || rest.starts_with("https://") {
                    current = rest.to_string();
                } else if let Some(idx) = rest.find('/') {
                    let first_seg = &rest[..idx];
                    let tail = &rest[idx + 1..];
                    if !first_seg.contains('.') && tail.contains('.') {
                        current = format!("https://{tail}");
                    } else {
                        current = format!("https://{rest}");
                    }
                } else {
                    current = format!("https://{rest}");
                }
                break;
            }
        }
        if !changed {
            break;
        }
    }
    current
}

/// Extrait le nom de fichier audio final d'une URL (sans query string) pour le matching de secours.
fn extract_audio_basename(raw_url: &str) -> Option<String> {
    let norm = normalize_url_for_sync(raw_url);
    let parsed = Url::parse(&norm).ok()?;
    let seg = parsed.path_segments()?.next_back()?.trim();
    if seg.len() >= 6 && seg.contains('.') {
        Some(seg.to_ascii_lowercase())
    } else {
        None
    }
}

/// Convertit un `SyncQueueItem` local en `RemoteEpisodeAction` conforme au protocole cible.
pub fn to_remote_action(
    item: &SyncQueueItem,
    device_id: Option<&str>,
    uppercase_action: bool,
) -> RemoteEpisodeAction {
    let action = if uppercase_action {
        item.action.to_ascii_uppercase()
    } else {
        item.action.to_ascii_lowercase()
    };

    RemoteEpisodeAction {
        podcast: item.feed_url.clone(),
        episode: item.episode_url.clone(),
        guid: item.guid.clone(),
        action,
        timestamp: item.timestamp.clone(),
        started: item.started_sec,
        position: item.position_sec,
        total: item.total_sec,
        device: device_id.map(String::from),
    }
}

/// Exécute une synchronisation complète bidirectionnelle (Push des actions locales puis Pull mobile).
pub async fn run_full_sync(
    client: &Client,
    db: &Database,
    crypto: &LocalCrypto,
    dir_covers: &Path,
    settings: &AppSettings,
) -> Result<SyncReport, String> {
    if settings.sync_provider == "none" {
        return Err("Aucun serveur de synchronisation configuré".to_string());
    }
    let effective_server_url = if settings.sync_server_url.trim().is_empty()
        && settings.sync_provider == "gpodder"
    {
        "https://gpodder.net".to_string()
    } else {
        settings.sync_server_url.trim().to_string()
    };
    if effective_server_url.is_empty() {
        return Err("Veuillez renseigner l'URL du serveur de synchronisation".to_string());
    }

    let secret_enc = db
        .get_setting_raw("sync_secret_enc")?
        .unwrap_or_default();
    let secret_plain = crypto.decrypt(&secret_enc)?;
    if settings.sync_username.trim().is_empty() || secret_plain.trim().is_empty() {
        return Err("Identifiants de synchronisation incomplets".to_string());
    }

    let pending = db.get_pending_sync_actions()?;
    let since = settings.sync_last_timestamp;

    // Sépare abonnements et actions d'épisodes
    let mut sub_add = Vec::new();
    let mut sub_remove = Vec::new();
    let mut ep_actions = Vec::new();
    let mut pushed_ids = Vec::new();

    for item in &pending {
        match item.action.as_str() {
            "SUBSCRIBE" => {
                if settings.sync_subscriptions {
                    sub_add.push(item.feed_url.clone());
                }
                pushed_ids.push(item.id);
            }
            "UNSUBSCRIBE" => {
                if settings.sync_subscriptions {
                    sub_remove.push(item.feed_url.clone());
                }
                pushed_ids.push(item.id);
            }
            "DOWNLOAD" | "DELETE" => {
                if settings.sync_download_delete {
                    ep_actions.push(item.clone());
                }
                pushed_ids.push(item.id);
            }
            "PLAY" | "NEW" => {
                if settings.sync_episode_actions {
                    ep_actions.push(item.clone());
                }
                pushed_ids.push(item.id);
            }
            _ => {
                pushed_ids.push(item.id);
            }
        }
    }

    let local_feeds_initial = db.list_feeds()?;
    let force_pull_all_devices = since == 0 || local_feeds_initial.is_empty();

    let (mut pulled_subs, pulled_actions, new_timestamp, discovered_devices) = match settings
        .sync_provider
        .as_str()
    {
        "nextcloud" => {
            let (s, a, ts) = nextcloud::sync_nextcloud(
                client,
                &effective_server_url,
                &settings.sync_username,
                &secret_plain,
                since,
                &sub_add,
                &sub_remove,
                &ep_actions,
            )
            .await?;
            (s, a, ts, Vec::new())
        }
        "gpodder" => {
            let out = gpodder::sync_gpodder_v2(
                client,
                &effective_server_url,
                &settings.sync_username,
                &secret_plain,
                &settings.sync_device_id,
                &settings.sync_device_name,
                since,
                force_pull_all_devices,
                &sub_add,
                &sub_remove,
                &ep_actions,
            )
            .await?;
            (out.subs, out.actions, out.timestamp, out.devices)
        }
        other => return Err(format!("Fournisseur de synchro inconnu : {other}")),
    };

    // Marque les actions envoyées comme synchronisées
    db.mark_sync_actions_done(&pushed_ids)?;

    // Inclut également les podcasts référencés dans les actions d'épisodes reçues d'AntennaPod
    // (garantit l'import même si un serveur oPodSync stocke les abonnements sur un appareil séparé)
    if settings.sync_subscriptions {
        let removed_norms: std::collections::HashSet<String> = pulled_subs
            .remove
            .iter()
            .map(|u| normalize_url_for_sync(u))
            .collect();
        let mut added_norms: std::collections::HashSet<String> = pulled_subs
            .add
            .iter()
            .map(|u| normalize_url_for_sync(u))
            .collect();
        for act in &pulled_actions {
            let p_url = act.podcast.trim();
            if p_url.is_empty() {
                continue;
            }
            let norm = normalize_url_for_sync(p_url);
            if !removed_norms.contains(&norm) && added_norms.insert(norm) {
                pulled_subs.add.push(p_url.to_string());
            }
        }
    }

    // Applique les abonnements ajoutés / supprimés depuis AntennaPod mobile
    let mut added_feeds = 0usize;
    let mut removed_feeds = 0usize;
    let mut failed_feeds = 0usize;
    if settings.sync_subscriptions {
        let local_feeds = db.list_feeds()?;
        let mut to_subscribe: Vec<String> = Vec::new();
        for url in &pulled_subs.add {
            let norm = normalize_url_for_sync(url);
            let exists = local_feeds
                .iter()
                .any(|f| normalize_url_for_sync(&f.feed_url) == norm);
            if !exists {
                to_subscribe.push(url.clone());
            }
        }

        // Abonnement concurrent par lots de 6 pour importer rapidement des dizaines de flux oPodSync
        for chunk in to_subscribe.chunks(6) {
            let mut futs = Vec::with_capacity(chunk.len());
            for url in chunk {
                futs.push(subscribe_to_feed(
                    client,
                    db,
                    crypto,
                    dir_covers,
                    url,
                    None,
                    None,
                    false,
                ));
            }
            for res in futures_util::future::join_all(futs).await {
                if res.is_ok() {
                    added_feeds += 1;
                } else {
                    failed_feeds += 1;
                }
            }
        }

        for url in &pulled_subs.remove {
            let norm = normalize_url_for_sync(url);
            if let Some(feed) = local_feeds
                .iter()
                .find(|f| !f.is_local_folder && normalize_url_for_sync(&f.feed_url) == norm)
            {
                if db.delete_feed(feed.id).is_ok() {
                    removed_feeds += 1;
                }
            }
        }
    }

    // Rafraîchit les flux RSS déjà présents qui ont reçu des actions distantes d'épisodes
    // afin que les épisodes publiés récemment sur le téléphone existent bien en base locale
    // avant le rejeu de `apply_pulled_episode_actions`.
    if settings.sync_episode_actions && !pulled_actions.is_empty() {
        let current_feeds = db.list_feeds()?;
        let current_eps = db.query_episodes(&crate::models::EpisodeQuery {
            view: "all".into(),
            ..Default::default()
        })?;
        let mut feeds_to_refresh: Vec<i64> = Vec::new();
        for act in &pulled_actions {
            let norm_ep = normalize_url_for_sync(&act.episode);
            let exists_ep = current_eps.iter().any(|e| {
                act.guid
                    .as_deref()
                    .filter(|g| !g.is_empty())
                    .map(|g| e.guid == g)
                    .unwrap_or(false)
                    || normalize_url_for_sync(&e.audio_url) == norm_ep
            });
            if !exists_ep {
                let norm_feed = normalize_url_for_sync(&act.podcast);
                if let Some(feed) = current_feeds
                    .iter()
                    .find(|f| !f.is_local_folder && normalize_url_for_sync(&f.feed_url) == norm_feed)
                {
                    if !feeds_to_refresh.contains(&feed.id) {
                        feeds_to_refresh.push(feed.id);
                    }
                }
            }
        }
        for chunk in feeds_to_refresh.chunks(6) {
            let mut futs = Vec::with_capacity(chunk.len());
            for &fid in chunk {
                futs.push(crate::rss::refresh_single_feed(
                    client,
                    db,
                    crypto,
                    dir_covers,
                    settings,
                    fid,
                ));
            }
            let _ = futures_util::future::join_all(futs).await;
        }
    }

    // Applique les actions d'épisodes reçues (DOWNLOAD / PLAY / DELETE / NEW) dans l'ordre chronologique
    let raw_remote_actions = pulled_actions.len();
    let (applied_count, unmatched_actions) = if settings.sync_episode_actions {
        apply_pulled_episode_actions(db, settings, &pulled_actions)?
    } else {
        (0, 0)
    };

    let final_ts = new_timestamp.max(chrono::Utc::now().timestamp());
    db.set_setting_raw("sync_last_timestamp", &final_ts.to_string())?;

    Ok(SyncReport {
        success: true,
        message: "Synchronisation réussie".to_string(),
        pushed_actions: ep_actions.len() + sub_add.len() + sub_remove.len(),
        pulled_actions: applied_count,
        raw_remote_actions,
        unmatched_actions,
        added_feeds,
        removed_feeds,
        failed_feeds,
        devices: discovered_devices,
        synced_at: final_ts,
    })
}

/// Applique les actions d'épisodes venant d'AntennaPod mobile en rejouant le flux chronologique :
/// - `DOWNLOAD` : épisode téléchargé / en attente sur le téléphone -> retiré de l'Inbox et ajouté à la File d'attente PC
/// - `PLAY` (>= 98 % ou position == total > 0) : marqué Lu + retiré de la File et de l'Inbox
/// - `PLAY` (en cours, position > 0) : curseur synchronisé à la seconde exacte + retiré de l'Inbox + ajouté à la File
/// - `DELETE` : retiré de la File d'attente et de l'Inbox
/// - `NEW` : réinitialisé Non lu et replacé dans la Boîte de réception (Inbox)
///
/// Matching robuste en 3 niveaux : GUID exact -> URL audio canonique -> même podcast + même nom de fichier audio.
pub(crate) fn apply_pulled_episode_actions(
    db: &Database,
    settings: &AppSettings,
    actions: &[RemoteEpisodeAction],
) -> Result<(usize, usize), String> {
    if actions.is_empty() {
        return Ok((0, 0));
    }

    let all_eps = db.query_episodes(&crate::models::EpisodeQuery {
        view: "all".into(),
        ..Default::default()
    })?;
    let feeds = db.list_feeds()?;
    let feed_norm_by_id: std::collections::HashMap<i64, String> = feeds
        .iter()
        .map(|f| (f.id, normalize_url_for_sync(&f.feed_url)))
        .collect();

    // Associe chaque action distante à son épisode local puis trie chronologiquement
    let mut matched_actions: Vec<(i64, i64, usize, &RemoteEpisodeAction)> = Vec::new();
    let mut unmatched_count = 0usize;

    for (order_idx, act) in actions.iter().enumerate() {
        let act_ts = parse_sync_timestamp(&act.timestamp);
        let norm_ep_url = normalize_url_for_sync(&act.episode);
        let norm_feed_url = normalize_url_for_sync(&act.podcast);
        let act_basename = extract_audio_basename(&act.episode);

        // Niveau 1 : GUID exact
        let mut matched = act
            .guid
            .as_deref()
            .filter(|g| !g.is_empty())
            .and_then(|g| all_eps.iter().find(|e| e.guid == g));

        // Niveau 2 : URL audio canonique exacte (après déballage régies publicitaires)
        if matched.is_none() && !norm_ep_url.is_empty() {
            matched = all_eps
                .iter()
                .find(|e| normalize_url_for_sync(&e.audio_url) == norm_ep_url);
        }

        // Niveau 3 : même podcast canonique + même nom de fichier audio final
        if matched.is_none() {
            if let Some(ref base_name) = act_basename {
                matched = all_eps.iter().find(|e| {
                    let same_feed = feed_norm_by_id
                        .get(&e.feed_id)
                        .map(|u| !norm_feed_url.is_empty() && u == &norm_feed_url)
                        .unwrap_or(false);
                    same_feed
                        && extract_audio_basename(&e.audio_url).as_deref() == Some(base_name.as_str())
                });
            }
        }

        let Some(ep) = matched else {
            unmatched_count += 1;
            continue;
        };

        // Conflit : n'écrase pas une écoute locale plus récente
        if let Some(local_played_at) = ep.played_at {
            if act_ts > 0 && local_played_at > act_ts {
                continue;
            }
        }

        matched_actions.push((act_ts, ep.id, order_idx, act));
    }

    // Rejoue les actions dans l'ordre chronologique exact (conserve l'ordre serveur à timestamp égal)
    matched_actions.sort_by_key(|(ts, _, idx, _)| (*ts, *idx));

    let mut applied = 0usize;
    for (_ts, ep_id, _idx, act) in matched_actions {
        let kind = act.action.to_ascii_uppercase();
        match kind.as_str() {
            "DOWNLOAD" => {
                // Épisode téléchargé / mis en attente sur AntennaPod :
                // on le place dans la File d'attente PC et on le retire de la Boîte de réception.
                let _ = db.set_inbox_state(&[ep_id], 0);
                if let Ok(Some(ep)) = db.get_episode_by_id(ep_id) {
                    if !ep.is_played && ep.queue_position.is_none() {
                        let _ = db.enqueue_episode(
                            ep_id,
                            crate::models::QueueInsertStrategy::Bottom,
                        );
                    }
                }
                applied += 1;
            }
            "PLAY" => {
                let pos_sec = act.position.unwrap_or(0).max(0);
                let total_sec = act.total.unwrap_or(0).max(0);
                // Règle AntennaPod : si position >= 98 % de total (ou position == total > 0), considéré Lu
                let is_completed = total_sec > 0
                    && (pos_sec >= total_sec || (pos_sec as f64 / total_sec as f64) >= 0.98);
                if is_completed {
                    let res = db.mark_episodes_played(
                        &[ep_id],
                        true,
                        false, // Ne pas ré-émettre vers sync_queue ce qui vient du serveur
                        settings.auto_delete_played,
                        settings.protect_favorites_from_delete,
                    )?;
                    for f in res.files_to_delete {
                        let _ = std::fs::remove_file(f);
                    }
                } else {
                    let dur_ms = if total_sec > 0 {
                        Some(total_sec * 1000)
                    } else {
                        None
                    };
                    let _ = db.update_playback_position(ep_id, pos_sec * 1000, dur_ms);
                    let _ = db.set_inbox_state(&[ep_id], 0);
                    // Un épisode en cours d'écoute sur AntennaPod est ajouté dans la File d'attente PC
                    if pos_sec > 0 {
                        if let Ok(Some(ep)) = db.get_episode_by_id(ep_id) {
                            if ep.queue_position.is_none() && !ep.is_played {
                                let _ = db.enqueue_episode(
                                    ep_id,
                                    crate::models::QueueInsertStrategy::Bottom,
                                );
                            }
                        }
                    }
                }
                applied += 1;
            }
            "DELETE" => {
                // Épisode supprimé sur le téléphone : retiré de la File d'attente et de l'Inbox PC
                let _ = db.remove_from_queue(&[ep_id]);
                let _ = db.set_inbox_state(&[ep_id], 0);
                applied += 1;
            }
            "NEW" => {
                let _ = db.mark_episodes_played(&[ep_id], false, false, false, true);
                let _ = db.set_inbox_state(&[ep_id], 1);
                applied += 1;
            }
            _ => {}
        }
    }

    Ok((applied, unmatched_count))
}

/// Parse un horodatage gpodder/oPodSync/Nextcloud (RFC 3339 avec `Z`/fuseau ou ISO-8601 naïf UTC).
fn parse_sync_timestamp(raw: &str) -> i64 {
    let s = raw.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return dt.timestamp();
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return naive.and_utc().timestamp();
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return naive.and_utc().timestamp();
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_url_normalization_unwraps_ad_trackers() {
        let a = normalize_url_for_sync(
            "https://dts.podtrac.com/redirect.mp3/cdn.example.com/ep1.mp3?utm_source=mobile",
        );
        let b = normalize_url_for_sync("http://cdn.example.com/ep1.mp3/");
        let c = normalize_url_for_sync("https://chrt.fm/track/9A2B3C/cdn.example.com/ep1.mp3");
        assert_eq!(a, b);
        assert_eq!(c, b);
        assert_eq!(a, "https://cdn.example.com/ep1.mp3");
    }

    #[test]
    fn replays_antennapod_download_play_delete_and_new_into_queue_and_inbox() {
        use crate::db::episodes::UpsertEpisodeInput;
        use crate::db::feeds::UpsertFeedInput;

        let db = Database::open_in_memory().unwrap();
        let (fid, _) = db
            .upsert_feed(&UpsertFeedInput {
                title: "Mon Podcast".into(),
                feed_url: "https://podcast.example.com/feed.xml".into(),
                image_url: None,
                description: None,
                author: None,
                language: None,
                is_local_folder: false,
                local_folder_path: None,
                etag: None,
                last_modified: None,
                funding_url: None,
                username: None,
                password_enc: None,
            })
            .unwrap();

        let ids = db
            .upsert_episodes(
                fid,
                &[
                    UpsertEpisodeInput {
                        guid: "g1".into(),
                        title: "Téléchargé sur mobile".into(),
                        pub_date: 100,
                        description: None,
                        audio_url: "https://cdn-a.example.com/audio/ep_001.mp3".into(),
                        mime_type: None,
                        local_file_path: None,
                        file_size: 0,
                        duration_ms: 600_000,
                        transcript_url: None,
                        transcript_type: None,
                        transcript_data: None,
                        auto_mark_played: false,
                        put_in_inbox: true,
                    },
                    UpsertEpisodeInput {
                        guid: "g2".into(),
                        title: "En cours avec autre CDN".into(),
                        pub_date: 200,
                        description: None,
                        audio_url: "https://cdn-b.example.com/path/episode_two_file.mp3".into(),
                        mime_type: None,
                        local_file_path: None,
                        file_size: 0,
                        duration_ms: 1_200_000,
                        transcript_url: None,
                        transcript_type: None,
                        transcript_data: None,
                        auto_mark_played: false,
                        put_in_inbox: true,
                    },
                    UpsertEpisodeInput {
                        guid: "g3".into(),
                        title: "Remis en nouveau".into(),
                        pub_date: 300,
                        description: None,
                        audio_url: "https://cdn-a.example.com/audio/ep_003.mp3".into(),
                        mime_type: None,
                        local_file_path: None,
                        file_size: 0,
                        duration_ms: 300_000,
                        transcript_url: None,
                        transcript_type: None,
                        transcript_data: None,
                        auto_mark_played: true,
                        put_in_inbox: false,
                    },
                ],
            )
            .unwrap();

        // Récupère les 3 IDs (g3 était auto_mark_played donc on requiert la liste)
        let eps = db
            .query_episodes(&crate::models::EpisodeQuery {
                view: "all".into(),
                ..Default::default()
            })
            .unwrap();
        let e1 = ids[0];
        let e2 = ids[1];
        let e3 = eps.iter().find(|e| e.guid == "g3").unwrap().id;

        let actions = vec![
            // 1. Téléchargé sur AntennaPod (sans GUID) -> doit passer en File d'attente et quitter l'Inbox
            RemoteEpisodeAction {
                podcast: "https://podcast.example.com/feed.xml".into(),
                episode: "https://cdn-a.example.com/audio/ep_001.mp3".into(),
                guid: None,
                action: "download".into(),
                timestamp: "2026-11-19T10:00:00".into(),
                started: None,
                position: None,
                total: None,
                device: Some("antennapod".into()),
            },
            // 2. Lecture en cours sur AntennaPod avec domaine CDN différent (matching niveau 3 par basename)
            RemoteEpisodeAction {
                podcast: "https://podcast.example.com/feed.xml".into(),
                episode: "https://megaphone-cdn.other.net/ads/episode_two_file.mp3?sig=xyz".into(),
                guid: None,
                action: "play".into(),
                timestamp: "2026-11-19T10:05:00Z".into(),
                started: Some(0),
                position: Some(320),
                total: Some(1200),
                device: Some("antennapod".into()),
            },
            // 3. Remis en NEW -> doit repasser Non lu et revenir dans la Boîte de réception
            RemoteEpisodeAction {
                podcast: "https://podcast.example.com/feed.xml".into(),
                episode: "https://cdn-a.example.com/audio/ep_003.mp3".into(),
                guid: None,
                action: "new".into(),
                timestamp: "2026-11-19T10:10:00Z".into(),
                started: None,
                position: None,
                total: None,
                device: Some("antennapod".into()),
            },
        ];

        let (applied, unmatched) =
            apply_pulled_episode_actions(&db, &AppSettings::default(), &actions).unwrap();
        assert_eq!(applied, 3);
        assert_eq!(unmatched, 0);

        let q = db.get_queue_ids().unwrap();
        assert_eq!(q, vec![e1, e2], "e1 (DOWNLOAD) et e2 (PLAY en cours) doivent être en file");

        let ep1 = db.get_episode_by_id(e1).unwrap().unwrap();
        assert_eq!(ep1.inbox_state, 0);

        let ep2 = db.get_episode_by_id(e2).unwrap().unwrap();
        assert_eq!(ep2.position_ms, 320_000);
        assert_eq!(ep2.inbox_state, 0);

        let ep3 = db.get_episode_by_id(e3).unwrap().unwrap();
        assert!(!ep3.is_played);
        assert_eq!(ep3.inbox_state, 1);
    }
}
