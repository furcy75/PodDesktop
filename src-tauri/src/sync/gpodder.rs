//! Client natif **gpodder API v2** 100 % conforme à `gpodder.net`, **oPodSync**,
//! `goPodder`, `Micro-GPodder`, `PodFetch` et **AntennaPod**.
//!
//! Garantit la synchronisation complète PC ↔ AntennaPod sur tous les serveurs :
//! 1. **Double authentification adaptative (Cookie `sessionid` + repli Basic Auth sur 401)** :
//!    gère à la fois `gpodder.net` (session cookie stricte) et `oPodSync` / proxys inverses
//!    (PHP session ou HTTP Basic Auth).
//! 2. **Récupération multi-appareils (critique pour oPodSync + AntennaPod)** :
//!    découvre tous les appareils du compte (`/api/2/devices/{user}.json`, ex: téléphone AntennaPod)
//!    et importe les abonnements de **chaque appareil** (`/api/2/subscriptions/{user}/{dev}.json`,
//!    `/subscriptions/{user}/{dev}.json`, et `/subscriptions/{user}.json`), en acceptant aussi bien
//!    le format delta `{add, remove}` que les tableaux JSON simples `["url", ...]`.
//! 3. **Liaison d'appareils (`sync-devices`)** : fusionne l'appareil PC avec AntennaPod mobile.
//! 4. **Résilience réseau** : normalisation `https://`, timeout 45 s, relances automatiques sur
//!    erreurs transitoires 502/503/504 et messages de diagnostic détaillés.

use crate::models::SyncQueueItem;
use crate::sync::{normalize_url_for_sync, to_remote_action, RemoteEpisodeAction, RemoteSubscriptionsDelta};
use reqwest::header::SET_COOKIE;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

/// Résultat étendu d'une synchronisation gpodder v2 / oPodSync.
#[derive(Debug, Clone)]
pub struct GpodderSyncOutput {
    pub subs: RemoteSubscriptionsDelta,
    pub actions: Vec<RemoteEpisodeAction>,
    pub timestamp: i64,
    pub devices: Vec<String>,
}

/// Normalise l'URL du serveur gpodder (`gpodder.net` -> `https://gpodder.net`).
pub fn normalize_gpodder_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return "https://gpodder.net".to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

/// Construit un client HTTP dédié avec magasin de cookies (`sessionid` / `PHPSESSID`) et timeout 45 s.
fn build_gpodder_session_client() -> Result<Client, String> {
    Client::builder()
        .user_agent("PodDesktop/1.0 (AntennaPod-compatible desktop client)")
        .cookie_store(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::limited(6))
        .build()
        .map_err(|e| format!("Impossible d'initialiser le client HTTP gpodder : {e}"))
}

/// Exécute une requête avec relances automatiques sur erreurs transitoires 502/503/504.
async fn send_with_retry<F>(mut make_req: F) -> Result<Response, String>
where
    F: FnMut() -> RequestBuilder,
{
    let mut last_status: Option<StatusCode> = None;
    let mut last_err: Option<String> = None;

    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(800 * attempt as u64)).await;
        }
        match make_req().send().await {
            Ok(resp) => {
                let st = resp.status();
                if matches!(
                    st,
                    StatusCode::BAD_GATEWAY
                        | StatusCode::SERVICE_UNAVAILABLE
                        | StatusCode::GATEWAY_TIMEOUT
                ) {
                    last_status = Some(st);
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                last_err = Some(e.to_string());
                continue;
            }
        }
    }

    if let Some(st) = last_status {
        return Err(format!(
            "Le serveur distant est temporairement indisponible (HTTP {st}). \
             Réessayez plus tard ou vérifiez l'état de votre serveur."
        ));
    }
    Err(format!(
        "Impossible de joindre le serveur gpodder : {}",
        last_err.unwrap_or_else(|| "délai d'attente dépassé".to_string())
    ))
}

/// Envoie une requête authentifiée : utilise d'abord le mode courant (cookie ou Basic),
/// et bascule automatiquement en HTTP Basic Auth si un appel cookie-only reçoit 401/403
/// (cas fréquent sur oPodSync / reverse proxy avec cookie non-session).
async fn send_authed<F>(
    use_basic: &mut bool,
    username: &str,
    password: &str,
    mut build_req: F,
) -> Result<Response, String>
where
    F: FnMut() -> RequestBuilder,
{
    let resp = send_with_retry(|| {
        let r = build_req();
        if *use_basic {
            r.basic_auth(username, Some(password))
        } else {
            r
        }
    })
    .await?;

    if !*use_basic
        && (resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN)
    {
        *use_basic = true;
        return send_with_retry(|| build_req().basic_auth(username, Some(password))).await;
    }

    Ok(resp)
}

#[allow(clippy::too_many_arguments)]
pub async fn sync_gpodder_v2(
    _shared_client: &Client,
    server_url: &str,
    username: &str,
    password: &str,
    device_id: &str,
    device_name: &str,
    since: i64,
    force_pull_all_devices: bool,
    sub_add: &[String],
    sub_remove: &[String],
    pending_actions: &[SyncQueueItem],
) -> Result<GpodderSyncOutput, String> {
    let client = build_gpodder_session_client()?;
    let base = normalize_gpodder_base_url(server_url);
    let user_trim = username.trim();
    let user_enc = encode_path_segment(user_trim);
    let dev_id_raw = if device_id.trim().is_empty() {
        "poddesktop-pc"
    } else {
        device_id.trim()
    };
    let dev_enc = encode_path_segment(dev_id_raw);

    // 1. Authentification API v2 (récupère le cookie `sessionid` / `PHPSESSID` si émis)
    let auth_url = format!("{base}/api/2/auth/{user_enc}/login.json");
    let auth_resp = send_with_retry(|| {
        client
            .post(&auth_url)
            .basic_auth(user_trim, Some(password))
            .body("")
    })
    .await?;

    if auth_resp.status() == StatusCode::UNAUTHORIZED || auth_resp.status() == StatusCode::FORBIDDEN
    {
        return Err(format!(
            "Identifiants refusés par {base} (HTTP {}). Vérifiez votre nom d'utilisateur et mot de passe.",
            auth_resp.status()
        ));
    }
    if !auth_resp.status().is_success() {
        return Err(format!(
            "Échec authentification gpodder v2 sur {base} (HTTP {})",
            auth_resp.status()
        ));
    }

    let has_session_cookie = auth_resp.headers().get(SET_COOKIE).is_some();
    let mut use_basic = !has_session_cookie;

    // 2. Déclaration / mise à jour de l'appareil PC
    let device_url = format!("{base}/api/2/devices/{user_enc}/{dev_enc}.json");
    #[derive(Serialize)]
    struct DeviceMeta<'a> {
        caption: &'a str,
        #[serde(rename = "type")]
        kind: &'a str,
    }
    let _ = send_authed(&mut use_basic, user_trim, password, || {
        client.post(&device_url).json(&DeviceMeta {
            caption: device_name,
            kind: "desktop",
        })
    })
    .await;

    // 3. Découverte de tous les appareils du compte (ex: AntennaPod Android) + liaison sync-devices
    let devices = discover_and_link_devices(
        &client,
        &base,
        &user_enc,
        user_trim,
        password,
        dev_id_raw,
        &mut use_basic,
    )
    .await;

    // 4. Push des changements locaux d'abonnements pour l'appareil PC
    let subs_url = format!("{base}/api/2/subscriptions/{user_enc}/{dev_enc}.json");
    if !sub_add.is_empty() || !sub_remove.is_empty() {
        #[derive(Serialize)]
        struct SubChange<'a> {
            add: &'a [String],
            remove: &'a [String],
        }
        let resp = send_authed(&mut use_basic, user_trim, password, || {
            client.post(&subs_url).json(&SubChange {
                add: sub_add,
                remove: sub_remove,
            })
        })
        .await?;
        if !resp.status().is_success() {
            return Err(format!("gpodder subscriptions POST HTTP {}", resp.status()));
        }
    }

    // 5. Pull des abonnements (delta appareil PC + TOUS les autres appareils du compte oPodSync/gpodder)
    let mut all_add: Vec<String> = Vec::new();
    let mut all_remove: Vec<String> = Vec::new();
    let mut seen_add_norm: HashSet<String> = HashSet::new();
    let mut seen_remove_norm: HashSet<String> = HashSet::new();
    let mut max_sub_ts: i64 = 0;

    let mut push_add = |url: String| {
        let norm = normalize_url_for_sync(&url);
        if !norm.is_empty() && seen_add_norm.insert(norm) {
            all_add.push(url);
        }
    };
    let mut push_remove = |url: String| {
        let norm = normalize_url_for_sync(&url);
        if !norm.is_empty() && seen_remove_norm.insert(norm) {
            all_remove.push(url);
        }
    };

    // 5a. Delta de l'appareil PC courant
    if let Ok((delta_add, delta_rem, ts)) =
        fetch_device_subscriptions(&client, &subs_url, since, user_trim, password, &mut use_basic)
            .await
    {
        max_sub_ts = max_sub_ts.max(ts);
        for u in delta_add {
            push_add(u);
        }
        for u in delta_rem {
            push_remove(u);
        }
    }

    // 5b. Synchronisation multi-appareils : sur oPodSync, les abonnements d'AntennaPod Android
    // sont stockés sous le device_id du téléphone (pas toujours recopiés vers `poddesktop-pc`).
    // On interroge donc TOUS les appareils enregistrés + le catalogue global du compte !
    let pull_other_devices = since == 0 || force_pull_all_devices;
    let other_since = if pull_other_devices { 0 } else { since };

    for other_dev in &devices {
        if other_dev == dev_id_raw {
            continue;
        }
        let other_enc = encode_path_segment(other_dev);
        // API v2 delta pour l'autre appareil (ex: AntennaPod Android)
        let other_v2_url = format!("{base}/api/2/subscriptions/{user_enc}/{other_enc}.json");
        if let Ok((d_add, d_rem, ts)) = fetch_device_subscriptions(
            &client,
            &other_v2_url,
            other_since,
            user_trim,
            password,
            &mut use_basic,
        )
        .await
        {
            max_sub_ts = max_sub_ts.max(ts);
            for u in d_add {
                push_add(u);
            }
            for u in d_rem {
                push_remove(u);
            }
        }

        // API Simple (`GET /subscriptions/{user}/{dev}.json`) largement utilisée par oPodSync & AntennaPod
        if pull_other_devices {
            let simple_dev_url = format!("{base}/subscriptions/{user_enc}/{other_enc}.json");
            if let Ok((s_add, _, _)) = fetch_device_subscriptions(
                &client,
                &simple_dev_url,
                0,
                user_trim,
                password,
                &mut use_basic,
            )
            .await
            {
                for u in s_add {
                    push_add(u);
                }
            }
        }
    }

    // 5c. Catalogue global du compte (`GET /subscriptions/{user}.json`)
    if pull_other_devices {
        let account_subs_url = format!("{base}/subscriptions/{user_enc}.json");
        if let Ok((acc_add, _, _)) = fetch_device_subscriptions(
            &client,
            &account_subs_url,
            0,
            user_trim,
            password,
            &mut use_basic,
        )
        .await
        {
            for u in acc_add {
                push_add(u);
            }
        }
    }

    // Retire des ajouts toute URL explicitement supprimée dans le même delta
    all_add.retain(|u| !seen_remove_norm.contains(&normalize_url_for_sync(u)));

    // 6. Push des actions d'épisodes locales (format gpodder v2 : `play`, `new`, `download`, `delete`)
    let ep_url = format!("{base}/api/2/episodes/{user_enc}.json");
    if !pending_actions.is_empty() {
        let payload: Vec<RemoteEpisodeAction> = pending_actions
            .iter()
            .map(|a| to_remote_action(a, Some(dev_id_raw), false))
            .collect();
        let resp = send_authed(&mut use_basic, user_trim, password, || {
            client.post(&ep_url).json(&payload)
        })
        .await?;
        if !resp.status().is_success() {
            return Err(format!("gpodder episodes POST HTTP {}", resp.status()));
        }
    }

    // 7. Pull des actions d'épisodes depuis `since`
    // Important : comme AntennaPod, on N'ENVOIE PAS `aggregated=true`, afin de récupérer
    // l'ensemble du flux chronologique (DOWNLOAD, PLAY, NEW, DELETE) sans perte d'actions.
    #[derive(Deserialize)]
    struct EpPullResp {
        #[serde(default)]
        actions: Vec<RemoteEpisodeAction>,
        #[serde(default)]
        timestamp: i64,
    }
    let ep_pull = send_authed(&mut use_basic, user_trim, password, || {
        let mut req = client.get(&ep_url);
        if since > 0 {
            req = req.query(&[("since", since.to_string())]);
        }
        req
    })
    .await?;
    if !ep_pull.status().is_success() {
        return Err(format!("gpodder episodes GET HTTP {}", ep_pull.status()));
    }
    let ep_body: EpPullResp = ep_pull
        .json()
        .await
        .map_err(|e| format!("Réponse actions gpodder invalide : {e}"))?;

    let latest_ts = max_sub_ts.max(ep_body.timestamp);
    Ok(GpodderSyncOutput {
        subs: RemoteSubscriptionsDelta {
            add: all_add,
            remove: all_remove,
            timestamp: max_sub_ts,
        },
        actions: ep_body.actions,
        timestamp: latest_ts,
        devices,
    })
}

/// Récupère les abonnements depuis un endpoint gpodder/oPodSync en acceptant
/// indifféremment le format objet `{ "add": [...], "remove": [...], "timestamp": ... }`
/// ou le format tableau plat `["https://...", ...]`.
async fn fetch_device_subscriptions(
    client: &Client,
    url: &str,
    since: i64,
    username: &str,
    password: &str,
    use_basic: &mut bool,
) -> Result<(Vec<String>, Vec<String>, i64), String> {
    let resp = send_authed(use_basic, username, password, || {
        let mut req = client.get(url);
        if since > 0 {
            req = req.query(&[("since", since.to_string())]);
        }
        req
    })
    .await?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok((Vec::new(), Vec::new(), 0));
    }

    // Format tableau JSON simple (`GET /subscriptions/{user}/{dev}.json` ou oPodSync)
    if trimmed.starts_with('[') {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(trimmed) {
            return Ok((list, Vec::new(), 0));
        }
    }

    // Format objet delta API v2 (`{ add, remove, timestamp }`)
    #[derive(Deserialize)]
    struct SubPullResp {
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
        #[serde(default)]
        timestamp: i64,
    }
    let parsed: SubPullResp =
        serde_json::from_str(trimmed).map_err(|e| format!("JSON abonnements invalide : {e}"))?;
    Ok((parsed.add, parsed.remove, parsed.timestamp))
}

/// Récupère tous les appareils du compte (ex: AntennaPod Android + PodDesktop PC)
/// et tente de les lier via `/api/2/sync-devices/{user}.json`.
async fn discover_and_link_devices(
    client: &Client,
    base: &str,
    user_enc: &str,
    username: &str,
    password: &str,
    my_device_id: &str,
    use_basic: &mut bool,
) -> Vec<String> {
    #[derive(Deserialize)]
    struct DeviceEntry {
        id: String,
    }
    let list_url = format!("{base}/api/2/devices/{user_enc}.json");
    let Ok(resp) = send_authed(use_basic, username, password, || client.get(&list_url)).await else {
        return vec![my_device_id.to_string()];
    };
    if !resp.status().is_success() {
        return vec![my_device_id.to_string()];
    }
    let devices: Vec<DeviceEntry> = resp.json().await.unwrap_or_default();
    let mut ids: Vec<String> = devices.into_iter().map(|d| d.id).collect();
    if !ids.iter().any(|id| id == my_device_id) {
        ids.push(my_device_id.to_string());
    }

    if ids.len() >= 2 {
        #[derive(Serialize)]
        struct SyncLink {
            synchronize: Vec<Vec<String>>,
        }
        let sync_url = format!("{base}/api/2/sync-devices/{user_enc}.json");
        let _ = send_authed(use_basic, username, password, || {
            client.post(&sync_url).json(&SyncLink {
                synchronize: vec![ids.clone()],
            })
        })
        .await;
    }

    ids
}

/// Encode un segment de chemin URL (`username`, `device_id`) en pourcentage si nécessaire.
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn normalizes_bare_gpodder_hostname() {
        assert_eq!(normalize_gpodder_base_url("gpodder.net"), "https://gpodder.net");
        assert_eq!(
            normalize_gpodder_base_url("https://gpodder.net/"),
            "https://gpodder.net"
        );
        assert_eq!(
            normalize_gpodder_base_url("http://localhost:8080/"),
            "http://localhost:8080"
        );
    }

    /// Test d'intégration simulant un serveur **oPodSync** réel où :
    /// - Les abonnements AntennaPod sont enregistrés uniquement sous l'appareil `antennapod-android`
    /// - `poddesktop-pc` n'a encore aucun abonnement propre
    /// - Le serveur émet un cookie mais exige Basic Auth en repli (401 sans Authorization)
    #[tokio::test]
    async fn opodsync_pulls_antennapod_device_subscriptions_with_basic_fallback() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            for _ in 0..20 {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let first_line = req.lines().next().unwrap_or("");
                let has_basic = req.to_ascii_lowercase().contains("authorization: basic ");

                let (status, extra_hdr, body) =
                    if first_line.starts_with("POST /api/2/auth/bob/login.json") {
                        // Émet un cookie non-session (ex: proxy/PHP) mais les endpoints exigent Basic Auth
                        ("200 OK", "Set-Cookie: proxy_id=1; Path=/\r\n", "{}")
                    } else if !has_basic {
                        ("401 Unauthorized", "", "{\"error\":\"basic auth required\"}")
                    } else if first_line.starts_with("GET /api/2/devices/bob.json") {
                        (
                            "200 OK",
                            "",
                            "[{\"id\":\"antennapod-android\"},{\"id\":\"poddesktop-pc\"}]",
                        )
                    } else if first_line.starts_with("GET /api/2/subscriptions/bob/poddesktop-pc.json")
                    {
                        ("200 OK", "", "{\"add\":[],\"remove\":[],\"timestamp\":1700000100}")
                    } else if first_line
                        .starts_with("GET /api/2/subscriptions/bob/antennapod-android.json")
                    {
                        (
                            "200 OK",
                            "",
                            "{\"add\":[\"https://antennapod.example.com/feed.xml\"],\"remove\":[],\"timestamp\":1700000200}",
                        )
                    } else if first_line.starts_with("GET /subscriptions/bob/antennapod-android.json")
                    {
                        (
                            "200 OK",
                            "",
                            "[\"https://antennapod.example.com/feed.xml\",\"https://second.example.com/rss\"]",
                        )
                    } else if first_line.starts_with("GET /api/2/episodes/bob.json") {
                        (
                            "200 OK",
                            "",
                            "{\"actions\":[{\"podcast\":\"https://antennapod.example.com/feed.xml\",\"episode\":\"https://cdn.example.com/ep1.mp3\",\"action\":\"play\",\"timestamp\":\"2026-11-19T10:00:00\",\"position\":240,\"total\":1200}],\"timestamp\":1700000300}",
                        )
                    } else {
                        ("200 OK", "", "{}")
                    };

                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra_hdr}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        let dummy_client = Client::new();
        let out = sync_gpodder_v2(
            &dummy_client,
            &format!("http://{addr}"),
            "bob",
            "secret",
            "poddesktop-pc",
            "PodDesktop PC",
            0,
            true,
            &[],
            &[],
            &[],
        )
        .await
        .expect("sync oPodSync multi-appareils");

        assert_eq!(out.devices.len(), 2);
        assert_eq!(out.subs.add.len(), 2);
        assert!(out
            .subs
            .add
            .contains(&"https://antennapod.example.com/feed.xml".to_string()));
        assert!(out
            .subs
            .add
            .contains(&"https://second.example.com/rss".to_string()));
        assert_eq!(out.actions.len(), 1);
        server.abort();
    }
}
