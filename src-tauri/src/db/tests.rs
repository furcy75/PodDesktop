//! Suite de tests unitaires pour la base SQLite (WAL, FTS5, File d'attente,
//! Marquage intelligent, Chiffrement AES-256-GCM).

use crate::crypto::LocalCrypto;
use crate::db::episodes::UpsertEpisodeInput;
use crate::db::feeds::{matches_feed_keywords, UpsertFeedInput};
use crate::db::Database;
use crate::models::{EpisodeQuery, QueueInsertStrategy};

fn seed_sample_db() -> (Database, i64, Vec<i64>) {
    let db = Database::open_in_memory().expect("ouverture DB mémoire");
    let (feed_id, _) = db
        .upsert_feed(&UpsertFeedInput {
            title: "Podcast Rust".into(),
            feed_url: "https://example.com/rust.xml".into(),
            image_url: None,
            description: Some("Développement systèmes".into()),
            author: Some("Équipe".into()),
            language: Some("fr".into()),
            is_local_folder: false,
            local_folder_path: None,
            etag: None,
            last_modified: None,
            funding_url: Some("https://example.com/donate".into()),
            username: None,
            password_enc: None,
        })
        .expect("insert feed");

    let eps = vec![
        UpsertEpisodeInput {
            guid: "ep-1".into(),
            title: "Introduction à Tauri v2".into(),
            pub_date: 1_700_000_100,
            description: Some("Architecture ultra-légère".into()),
            audio_url: "https://cdn.example.com/1.mp3".into(),
            mime_type: Some("audio/mpeg".into()),
            local_file_path: Some("/tmp/fake_1.mp3".into()),
            file_size: 10_000,
            duration_ms: 1_800_000,
            transcript_url: None,
            transcript_type: None,
            transcript_data: Some("Bonjour et bienvenue dans ce guide sur Symphonia".into()),
            auto_mark_played: false,
            put_in_inbox: true,
        },
        UpsertEpisodeInput {
            guid: "ep-2".into(),
            title: "Normalisation EBU R128".into(),
            pub_date: 1_700_000_200,
            description: Some("Traitement du signal audio".into()),
            audio_url: "https://cdn.example.com/2.mp3".into(),
            mime_type: Some("audio/mpeg".into()),
            local_file_path: Some("/tmp/fake_2.mp3".into()),
            file_size: 20_000,
            duration_ms: 2_400_000,
            transcript_url: None,
            transcript_type: None,
            transcript_data: None,
            auto_mark_played: false,
            put_in_inbox: true,
        },
        UpsertEpisodeInput {
            guid: "ep-3".into(),
            title: "Synchronisation AntennaPod".into(),
            pub_date: 1_700_000_300,
            description: Some("Protocole Nextcloud gpodder".into()),
            audio_url: "https://cdn.example.com/3.mp3".into(),
            mime_type: Some("audio/mpeg".into()),
            local_file_path: None,
            file_size: 15_000,
            duration_ms: 900_000,
            transcript_url: None,
            transcript_type: None,
            transcript_data: None,
            auto_mark_played: false,
            put_in_inbox: true,
        },
    ];
    let ids = db.upsert_episodes(feed_id, &eps).expect("insert episodes");
    (db, feed_id, ids)
}

#[test]
fn fts5_searches_across_title_description_and_transcript() {
    let (db, _, _) = seed_sample_db();

    let by_transcript = db
        .query_episodes(&EpisodeQuery {
            view: "all".into(),
            search: Some("Symphonia".into()),
            ..Default::default()
        })
        .expect("FTS5 transcript search");
    assert_eq!(by_transcript.len(), 1);
    assert_eq!(by_transcript[0].guid, "ep-1");

    let by_desc = db
        .query_episodes(&EpisodeQuery {
            view: "all".into(),
            search: Some("Nextcloud".into()),
            ..Default::default()
        })
        .expect("FTS5 desc search");
    assert_eq!(by_desc.len(), 1);
    assert_eq!(by_desc[0].guid, "ep-3");
}

#[test]
fn queue_insertion_strategies_work_accurately() {
    let (db, _, ids) = seed_sample_db();
    let (e1, e2, e3) = (ids[0], ids[1], ids[2]);

    db.enqueue_episode(e1, QueueInsertStrategy::Bottom).unwrap();
    db.enqueue_episode(e2, QueueInsertStrategy::Bottom).unwrap();
    // AfterCurrent doit insérer e3 en position 1 (juste après e1 en cours)
    db.enqueue_episode(e3, QueueInsertStrategy::AfterCurrent)
        .unwrap();
    assert_eq!(db.get_queue_ids().unwrap(), vec![e1, e3, e2]);

    // Top déplace e2 en position 0
    db.enqueue_episode(e2, QueueInsertStrategy::Top).unwrap();
    assert_eq!(db.get_queue_ids().unwrap(), vec![e2, e1, e3]);

    // Reorder Drag & Drop déplace e3 en position 0
    db.reorder_queue(e3, 0).unwrap();
    assert_eq!(db.get_queue_ids().unwrap(), vec![e3, e2, e1]);
}

#[test]
fn mark_played_emits_antennapod_play_sync_and_protects_favorites() {
    let (db, _, ids) = seed_sample_db();
    let (e1, e2) = (ids[0], ids[1]);

    // e1 est mis en Favori, e2 non ; les deux sont dans la file
    db.set_episodes_favorite(&[e1], true).unwrap();
    db.enqueue_episode(e1, QueueInsertStrategy::Bottom).unwrap();
    db.enqueue_episode(e2, QueueInsertStrategy::Bottom).unwrap();

    let res = db
        .mark_episodes_played(&[e1, e2], true, true, true, true)
        .expect("mark played");

    // Les deux sont retirés de la file et de l'Inbox
    assert!(db.get_queue_ids().unwrap().is_empty());
    assert_eq!(db.get_sidebar_counts().unwrap().inbox, 1);

    // Seul e2 (non favori) est proposé à l'auto-suppression ; e1 (favori) est protégé !
    assert_eq!(res.files_to_delete, vec!["/tmp/fake_2.mp3".to_string()]);

    // Deux actions PLAY complètes (position == total) prêtes pour AntennaPod mobile
    let pending = db.get_pending_sync_actions().unwrap();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].action, "PLAY");
    assert_eq!(pending[0].position_sec, pending[0].total_sec);
    assert!(pending[0].total_sec.unwrap() > 0);
}

#[test]
fn keyword_filtering_include_and_exclude() {
    assert!(matches_feed_keywords("Épisode spécial Rust", Some("rust, audio"), Some("bande-annonce")));
    assert!(!matches_feed_keywords("Bande-annonce saison 2", Some("rust, saison"), Some("bande-annonce")));
    assert!(!matches_feed_keywords("Cuisine du monde", Some("rust, audio"), None));
}

#[test]
fn aes_256_gcm_roundtrip_and_export_excludes_secrets() {
    let crypto = LocalCrypto::from_raw_key([42u8; 32]);
    let enc = crypto.encrypt("super-secret-token-nextcloud").unwrap();
    assert!(enc.starts_with("enc:v1:"));
    assert_ne!(enc, "super-secret-token-nextcloud");
    assert_eq!(crypto.decrypt(&enc).unwrap(), "super-secret-token-nextcloud");

    let (db, _, _) = seed_sample_db();
    db.set_setting_raw("theme", "dark").unwrap();
    db.set_setting_raw("sync_secret_enc", &enc).unwrap();

    let exported = db.export_settings_json(false).unwrap();
    assert!(exported.contains("\"theme\""));
    assert!(!exported.contains("sync_secret_enc"));
}

#[test]
fn wipe_remote_catalog_clears_remote_feeds_but_keeps_settings_and_local_folders() {
    let (db, _, ids) = seed_sample_db();
    db.set_setting_raw("sync_username", "alice").unwrap();
    db.set_setting_raw("sync_last_timestamp", "1700009999").unwrap();
    db.enqueue_episode(ids[0], QueueInsertStrategy::Bottom).unwrap();
    db.enqueue_sync_action("https://cdn.example.com/1.mp3", "https://example.com/rust.xml", None, "PLAY", Some(0), Some(60), Some(600)).unwrap();

    // Ajoute un dossier local virtuel qui NE DOIT PAS être effacé
    db.upsert_feed(&UpsertFeedInput {
        title: "Mes Livres Audio".into(),
        feed_url: "local:///audiobooks".into(),
        image_url: None,
        description: None,
        author: None,
        language: None,
        is_local_folder: true,
        local_folder_path: Some("/audiobooks".into()),
        etag: None,
        last_modified: None,
        funding_url: None,
        username: None,
        password_enc: None,
    })
    .unwrap();

    let (deleted, files) = db.wipe_remote_catalog_for_resync().unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(files.len(), 2);
    assert!(db.get_queue_ids().unwrap().is_empty());
    assert!(db.get_pending_sync_actions().unwrap().is_empty());
    assert_eq!(db.get_setting_raw("sync_username").unwrap().as_deref(), Some("alice"));
    assert_eq!(db.get_setting_raw("sync_last_timestamp").unwrap().as_deref(), Some("0"));
    let remaining_feeds = db.list_feeds().unwrap();
    assert_eq!(remaining_feeds.len(), 1);
    assert!(remaining_feeds[0].is_local_folder);
}
