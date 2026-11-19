//! Découverte de podcasts via les annuaires **iTunes Search API** et **Podcast Index API**,
//! résolution automatique des liens Apple Podcasts et mode Aperçu (*Preview*) avant abonnement.

use crate::models::{DiscoveryPodcast, FeedPreview, PreviewEpisode};
use crate::rss::{fetch_rss_feed, FetchFeedOutcome};
use reqwest::Client;
use serde::Deserialize;
use sha1::{Digest, Sha1};

/// Recherche combinée dans les annuaires iTunes Search et Podcast Index (si clés fournies).
pub async fn search_directories(
    client: &Client,
    query: &str,
    podcast_index_key: Option<&str>,
    podcast_index_secret: Option<&str>,
) -> Result<Vec<DiscoveryPodcast>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    // 1. Podcast Index API (si les identifiants gratuits de l'utilisateur sont renseignés)
    if let (Some(key), Some(secret)) = (
        podcast_index_key.filter(|s| !s.is_empty()),
        podcast_index_secret.filter(|s| !s.is_empty()),
    ) {
        if let Ok(mut pi_results) = search_podcast_index(client, trimmed, key, secret).await {
            results.append(&mut pi_results);
        }
    }

    // 2. iTunes Search API (toujours disponible sans authentification)
    let mut itunes_results = search_itunes(client, trimmed).await?;
    for item in itunes_results.drain(..) {
        if !results
            .iter()
            .any(|existing| existing.feed_url.eq_ignore_ascii_case(&item.feed_url))
        {
            results.push(item);
        }
    }

    Ok(results)
}

async fn search_itunes(client: &Client, query: &str) -> Result<Vec<DiscoveryPodcast>, String> {
    #[derive(Deserialize)]
    struct ItunesResp {
        #[serde(default)]
        results: Vec<ItunesItem>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ItunesItem {
        collection_name: Option<String>,
        artist_name: Option<String>,
        feed_url: Option<String>,
        artwork_url600: Option<String>,
        artwork_url100: Option<String>,
        #[serde(default)]
        genres: Vec<String>,
    }

    let resp = client
        .get("https://itunes.apple.com/search")
        .query(&[("media", "podcast"), ("limit", "25"), ("term", query)])
        .send()
        .await
        .map_err(|e| format!("Erreur recherche iTunes : {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("iTunes API HTTP {}", resp.status()));
    }

    let body: ItunesResp = resp
        .json()
        .await
        .map_err(|e| format!("Réponse iTunes JSON invalide : {e}"))?;

    Ok(body
        .results
        .into_iter()
        .filter_map(|i| {
            let feed_url = i.feed_url?;
            Some(DiscoveryPodcast {
                title: i.collection_name.unwrap_or_else(|| "Sans titre".into()),
                author: i.artist_name.unwrap_or_default(),
                feed_url,
                image_url: i.artwork_url600.or(i.artwork_url100),
                description: None,
                categories: i.genres,
                source: "itunes".to_string(),
            })
        })
        .collect())
}

async fn search_podcast_index(
    client: &Client,
    query: &str,
    api_key: &str,
    api_secret: &str,
) -> Result<Vec<DiscoveryPodcast>, String> {
    #[derive(Deserialize)]
    struct PiResp {
        #[serde(default)]
        feeds: Vec<PiFeed>,
    }
    #[derive(Deserialize)]
    struct PiFeed {
        title: Option<String>,
        author: Option<String>,
        url: Option<String>,
        image: Option<String>,
        description: Option<String>,
    }

    let ts = chrono::Utc::now().timestamp().to_string();
    let mut hasher = Sha1::new();
    hasher.update(format!("{api_key}{api_secret}{ts}").as_bytes());
    let auth_hash = format!("{:x}", hasher.finalize());

    let resp = client
        .get("https://api.podcastindex.org/api/1.0/search/byterm")
        .query(&[("q", query), ("max", "20")])
        .header("X-Auth-Key", api_key)
        .header("X-Auth-Date", &ts)
        .header("Authorization", auth_hash)
        .send()
        .await
        .map_err(|e| format!("Erreur Podcast Index : {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Podcast Index HTTP {}", resp.status()));
    }
    let body: PiResp = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body
        .feeds
        .into_iter()
        .filter_map(|f| {
            let feed_url = f.url?;
            Some(DiscoveryPodcast {
                title: f.title.unwrap_or_else(|| "Sans titre".into()),
                author: f.author.unwrap_or_default(),
                feed_url,
                image_url: f.image,
                description: f.description,
                categories: Vec::new(),
                source: "podcastindex".to_string(),
            })
        })
        .collect())
}

/// Résout automatiquement un lien Apple Podcasts (`https://podcasts.apple.com/.../id123456789`)
/// en URL de flux RSS canonique via l'endpoint `itunes.apple.com/lookup`.
pub async fn resolve_apple_podcasts_url(client: &Client, raw_url: &str) -> Result<String, String> {
    let trimmed = raw_url.trim();
    if !trimmed.contains("podcasts.apple.com") && !trimmed.contains("itunes.apple.com") {
        return Ok(trimmed.to_string());
    }

    // Extrait l'identifiant numérique après `/id`
    let id_part = trimmed
        .split("/id")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|s| !s.is_empty());

    let Some(apple_id) = id_part else {
        return Ok(trimmed.to_string());
    };

    #[derive(Deserialize)]
    struct LookupResp {
        #[serde(default)]
        results: Vec<LookupItem>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LookupItem {
        feed_url: Option<String>,
    }

    let resp = client
        .get("https://itunes.apple.com/lookup")
        .query(&[("id", apple_id.as_str()), ("entity", "podcast")])
        .send()
        .await
        .map_err(|e| format!("Erreur résolution lien Apple Podcasts : {e}"))?;

    let body: LookupResp = resp
        .json()
        .await
        .map_err(|e| format!("Réponse lookup Apple invalide : {e}"))?;

    body.results
        .into_iter()
        .find_map(|r| r.feed_url)
        .ok_or_else(|| "Aucun flux RSS public associé à ce lien Apple Podcasts".to_string())
}

/// Prévisualise un flux RSS (métadonnées + 15 derniers épisodes) sans créer d'abonnement.
pub async fn preview_feed(
    client: &Client,
    raw_url: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<FeedPreview, String> {
    let outcome = fetch_rss_feed(client, raw_url, None, None, username, password).await?;
    let FetchFeedOutcome::Updated {
        parsed, final_url, ..
    } = outcome
    else {
        return Err("Réponse inattendue lors de l'aperçu du flux".to_string());
    };

    let episodes = parsed
        .episodes
        .into_iter()
        .take(15)
        .map(|ep| PreviewEpisode {
            title: ep.title,
            pub_date: ep.pub_date,
            duration_ms: ep.duration_ms,
            audio_url: ep.audio_url,
            description: ep.description,
        })
        .collect();

    Ok(FeedPreview {
        title: parsed.title,
        feed_url: final_url,
        author: parsed.author,
        description: parsed.description,
        image_url: parsed.image_url,
        funding_url: parsed.funding_url,
        episodes,
    })
}
