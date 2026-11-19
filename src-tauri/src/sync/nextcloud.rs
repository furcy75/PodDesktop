//! Client natif **Nextcloud GPodder Sync** (`gpoddersync` API) + **Nextcloud Login Flow v2**.
//!
//! Implémente exactement les endpoints recommandés par AntennaPod :
//! - `POST /index.php/login/v2` (ouverture navigateur + polling du App Password)
//! - `GET/POST /index.php/apps/gpoddersync/subscriptions`
//! - `GET/POST /index.php/apps/gpoddersync/episode_action`

use crate::models::SyncQueueItem;
use crate::sync::{to_remote_action, RemoteEpisodeAction, RemoteSubscriptionsDelta};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Session temporaire renvoyée lors du démarrage du Login Flow v2 Nextcloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextcloudLoginInit {
    pub login_url: String,
    pub poll_endpoint: String,
    pub poll_token: String,
}

/// Identifiants définitifs (App Password dédié) obtenus après validation dans le navigateur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextcloudLoginResult {
    pub server: String,
    pub login_name: String,
    pub app_password: String,
}

/// Démarre le Nextcloud Login Flow v2 (`POST <server>/index.php/login/v2`).
pub async fn start_login_flow_v2(
    client: &Client,
    server_url: &str,
) -> Result<NextcloudLoginInit, String> {
    #[derive(Deserialize)]
    struct PollInfo {
        token: String,
        endpoint: String,
    }
    #[derive(Deserialize)]
    struct InitResp {
        login: String,
        poll: PollInfo,
    }

    let base = server_url.trim().trim_end_matches('/');
    let url = format!("{base}/index.php/login/v2");
    let resp = client
        .post(&url)
        .send()
        .await
        .map_err(|e| format!("Impossible de contacter Nextcloud ({url}) : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Nextcloud Login Flow v2 HTTP {}", resp.status()));
    }
    let body: InitResp = resp
        .json()
        .await
        .map_err(|e| format!("Réponse Login Flow v2 invalide : {e}"))?;

    Ok(NextcloudLoginInit {
        login_url: body.login,
        poll_endpoint: body.poll.endpoint,
        poll_token: body.poll.token,
    })
}

/// Interroge le endpoint de polling Nextcloud Login Flow v2 jusqu'à confirmation utilisateur.
/// Retourne `Ok(None)` tant que l'utilisateur n'a pas encore validé dans son navigateur (HTTP 404).
pub async fn poll_login_flow_v2(
    client: &Client,
    poll_endpoint: &str,
    poll_token: &str,
) -> Result<Option<NextcloudLoginResult>, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PollResp {
        server: String,
        login_name: String,
        app_password: String,
    }

    let resp = client
        .post(poll_endpoint)
        .form(&[("token", poll_token)])
        .send()
        .await
        .map_err(|e| format!("Erreur polling Nextcloud : {e}"))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("Polling Nextcloud HTTP {}", resp.status()));
    }
    let body: PollResp = resp
        .json()
        .await
        .map_err(|e| format!("Réponse polling Nextcloud invalide : {e}"))?;

    Ok(Some(NextcloudLoginResult {
        server: body.server,
        login_name: body.login_name,
        app_password: body.app_password,
    }))
}

/// Effectue la synchronisation complète (abonnements + actions d'épisodes) avec Nextcloud GPodder Sync.
#[allow(clippy::too_many_arguments)]
pub async fn sync_nextcloud(
    client: &Client,
    server_url: &str,
    username: &str,
    app_password: &str,
    since: i64,
    sub_add: &[String],
    sub_remove: &[String],
    pending_actions: &[SyncQueueItem],
) -> Result<(RemoteSubscriptionsDelta, Vec<RemoteEpisodeAction>, i64), String> {
    let base = server_url.trim().trim_end_matches('/');
    let subs_url = format!("{base}/index.php/apps/gpoddersync/subscriptions");
    let actions_url = format!("{base}/index.php/apps/gpoddersync/episode_action");

    // 1. Push abonnements si nécessaire
    if !sub_add.is_empty() || !sub_remove.is_empty() {
        #[derive(Serialize)]
        struct SubPayload<'a> {
            add: &'a [String],
            remove: &'a [String],
        }
        let resp = client
            .post(&subs_url)
            .basic_auth(username, Some(app_password))
            .header("OCS-APIRequest", "true")
            .json(&SubPayload {
                add: sub_add,
                remove: sub_remove,
            })
            .send()
            .await
            .map_err(|e| format!("Erreur push abonnements Nextcloud : {e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "Nextcloud gpoddersync subscriptions POST HTTP {}",
                resp.status()
            ));
        }
    }

    // 2. Pull abonnements depuis `since`
    #[derive(Deserialize)]
    struct SubsResp {
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
        #[serde(default)]
        timestamp: i64,
    }
    let sub_resp = client
        .get(&subs_url)
        .query(&[("since", since.to_string())])
        .basic_auth(username, Some(app_password))
        .header("OCS-APIRequest", "true")
        .send()
        .await
        .map_err(|e| format!("Erreur pull abonnements Nextcloud : {e}"))?;
    if !sub_resp.status().is_success() {
        return Err(format!(
            "Nextcloud gpoddersync subscriptions GET HTTP {}",
            sub_resp.status()
        ));
    }
    let subs_body: SubsResp = sub_resp
        .json()
        .await
        .map_err(|e| format!("Réponse abonnements Nextcloud invalide : {e}"))?;

    // 3. Push actions d'épisodes (Nextcloud gpoddersync attend les actions en MAJUSCULES : PLAY, NEW, ...)
    if !pending_actions.is_empty() {
        let payload: Vec<RemoteEpisodeAction> = pending_actions
            .iter()
            .map(|a| to_remote_action(a, None, true))
            .collect();
        let resp = client
            .post(&actions_url)
            .basic_auth(username, Some(app_password))
            .header("OCS-APIRequest", "true")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Erreur push episode_action Nextcloud : {e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "Nextcloud gpoddersync episode_action POST HTTP {}",
                resp.status()
            ));
        }
    }

    // 4. Pull actions d'épisodes depuis `since`
    #[derive(Deserialize)]
    struct ActionsResp {
        #[serde(default)]
        actions: Vec<RemoteEpisodeAction>,
        #[serde(default)]
        timestamp: i64,
    }
    let act_resp = client
        .get(&actions_url)
        .query(&[("since", since.to_string())])
        .basic_auth(username, Some(app_password))
        .header("OCS-APIRequest", "true")
        .send()
        .await
        .map_err(|e| format!("Erreur pull episode_action Nextcloud : {e}"))?;
    if !act_resp.status().is_success() {
        return Err(format!(
            "Nextcloud gpoddersync episode_action GET HTTP {}",
            act_resp.status()
        ));
    }
    let act_body: ActionsResp = act_resp
        .json()
        .await
        .map_err(|e| format!("Réponse actions Nextcloud invalide : {e}"))?;

    let latest_ts = subs_body.timestamp.max(act_body.timestamp);
    Ok((
        RemoteSubscriptionsDelta {
            add: subs_body.add,
            remove: subs_body.remove,
            timestamp: subs_body.timestamp,
        },
        act_body.actions,
        latest_ts,
    ))
}
