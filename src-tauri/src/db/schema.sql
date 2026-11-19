-- Schéma SQLite complet de PodDesktop (mode WAL + extension FTS5).
-- Conforme à la section 4 du Cahier des Charges v4.2.

PRAGMA foreign_keys = ON;

-- 1. Abonnements RSS (flux publics, flux privés HTTP Basic, dossiers locaux virtuels)
CREATE TABLE IF NOT EXISTS feeds (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    title               TEXT    NOT NULL,
    feed_url            TEXT    NOT NULL UNIQUE,
    image_url           TEXT,
    local_cover_path    TEXT,
    description         TEXT,
    author              TEXT,
    language            TEXT,
    is_local_folder     INTEGER NOT NULL DEFAULT 0,
    local_folder_path   TEXT,
    last_update         INTEGER,
    etag                TEXT,
    last_modified       TEXT,
    playback_speed      REAL,
    auto_download       INTEGER NOT NULL DEFAULT 0,
    auto_delete         INTEGER NOT NULL DEFAULT 0,
    keep_updated        INTEGER NOT NULL DEFAULT 1,
    max_kept_episodes   INTEGER,
    include_keywords    TEXT,
    exclude_keywords    TEXT,
    funding_url         TEXT,
    username            TEXT,
    password_enc        TEXT
);

-- 2. Tags / Dossiers personnalisés pour organiser les abonnements
CREATE TABLE IF NOT EXISTS tags (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    name    TEXT    NOT NULL UNIQUE,
    color   TEXT    NOT NULL DEFAULT '#6366f1'
);

-- 3. Association N-N entre podcasts et tags
CREATE TABLE IF NOT EXISTS feed_tags (
    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
    PRIMARY KEY (feed_id, tag_id)
);

-- 4. Catalogue complet des épisodes
CREATE TABLE IF NOT EXISTS episodes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    feed_id         INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    guid            TEXT    NOT NULL,
    title           TEXT    NOT NULL,
    pub_date        INTEGER NOT NULL DEFAULT 0,
    description     TEXT,
    audio_url       TEXT    NOT NULL,
    mime_type       TEXT,
    local_file_path TEXT,
    file_size       INTEGER NOT NULL DEFAULT 0,
    duration_ms     INTEGER NOT NULL DEFAULT 0,
    position_ms     INTEGER NOT NULL DEFAULT 0,
    is_played       INTEGER NOT NULL DEFAULT 0,
    is_favorite     INTEGER NOT NULL DEFAULT 0,
    inbox_state     INTEGER NOT NULL DEFAULT 0,
    played_at       INTEGER,
    transcript_url  TEXT,
    transcript_type TEXT,
    transcript_data TEXT,
    UNIQUE(feed_id, guid)
);

-- Index de performance pour les listes virtualisées (> 50 000 épisodes)
CREATE INDEX IF NOT EXISTS idx_episodes_feed_date ON episodes(feed_id, pub_date DESC);
CREATE INDEX IF NOT EXISTS idx_episodes_pub_date  ON episodes(pub_date DESC);
CREATE INDEX IF NOT EXISTS idx_episodes_inbox     ON episodes(inbox_state, pub_date DESC);
CREATE INDEX IF NOT EXISTS idx_episodes_fav       ON episodes(is_favorite, pub_date DESC);
CREATE INDEX IF NOT EXISTS idx_episodes_played_at ON episodes(played_at DESC);
CREATE INDEX IF NOT EXISTS idx_episodes_audio_url ON episodes(audio_url);

-- 5. Table virtuelle FTS5 (recherche plein texte instantanée sur titre, notes, transcription)
CREATE VIRTUAL TABLE IF NOT EXISTS episodes_fts USING fts5(
    title,
    description,
    transcript_data,
    content='episodes',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

-- Triggers de synchronisation automatique entre `episodes` et `episodes_fts`
CREATE TRIGGER IF NOT EXISTS episodes_ai AFTER INSERT ON episodes BEGIN
    INSERT INTO episodes_fts(rowid, title, description, transcript_data)
    VALUES (new.id, new.title, COALESCE(new.description, ''), COALESCE(new.transcript_data, ''));
END;

CREATE TRIGGER IF NOT EXISTS episodes_ad AFTER DELETE ON episodes BEGIN
    INSERT INTO episodes_fts(episodes_fts, rowid, title, description, transcript_data)
    VALUES ('delete', old.id, old.title, COALESCE(old.description, ''), COALESCE(old.transcript_data, ''));
END;

CREATE TRIGGER IF NOT EXISTS episodes_au AFTER UPDATE OF title, description, transcript_data ON episodes BEGIN
    INSERT INTO episodes_fts(episodes_fts, rowid, title, description, transcript_data)
    VALUES ('delete', old.id, old.title, COALESCE(old.description, ''), COALESCE(old.transcript_data, ''));
    INSERT INTO episodes_fts(rowid, title, description, transcript_data)
    VALUES (new.id, new.title, COALESCE(new.description, ''), COALESCE(new.transcript_data, ''));
END;

-- 6. File d'attente ordonnée (`position = 0` correspond à l'épisode en cours / suivant)
CREATE TABLE IF NOT EXISTS queue (
    position    INTEGER NOT NULL,
    episode_id  INTEGER NOT NULL UNIQUE REFERENCES episodes(id) ON DELETE CASCADE,
    PRIMARY KEY (position)
);

-- 7. Chapitres (ID3, Vorbis, Podcasting 2.0 JSON)
CREATE TABLE IF NOT EXISTS chapters (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    episode_id    INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
    start_time_ms INTEGER NOT NULL,
    title         TEXT    NOT NULL,
    url           TEXT,
    image_url     TEXT
);
CREATE INDEX IF NOT EXISTS idx_chapters_episode ON chapters(episode_id, start_time_ms ASC);

-- 8. Signets personnels horodatés ajoutés par l'utilisateur
CREATE TABLE IF NOT EXISTS bookmarks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    episode_id  INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
    position_ms INTEGER NOT NULL,
    note        TEXT    NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_bookmarks_episode ON bookmarks(episode_id, position_ms ASC);

-- 9. Historique détaillé du temps d'écoute par jour et par podcast
CREATE TABLE IF NOT EXISTS playback_stats (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    episode_id  INTEGER NOT NULL REFERENCES episodes(id) ON DELETE CASCADE,
    feed_id     INTEGER NOT NULL REFERENCES feeds(id)    ON DELETE CASCADE,
    date_day    TEXT    NOT NULL, -- Format YYYY-MM-DD
    listened_ms INTEGER NOT NULL DEFAULT 0,
    saved_ms    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_stats_day  ON playback_stats(date_day);
CREATE INDEX IF NOT EXISTS idx_stats_feed ON playback_stats(feed_id);

-- 10. File d'attente de synchronisation bidirectionnelle vers AntennaPod mobile
CREATE TABLE IF NOT EXISTS sync_queue (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    episode_url  TEXT    NOT NULL,
    feed_url     TEXT    NOT NULL,
    guid         TEXT,
    action       TEXT    NOT NULL, -- PLAY, NEW, DOWNLOAD, DELETE, SUBSCRIBE, UNSUBSCRIBE
    timestamp    TEXT    NOT NULL, -- UTC ISO-8601: YYYY-MM-DDTHH:MM:SSZ
    started_sec  INTEGER,
    position_sec INTEGER,
    total_sec    INTEGER,
    synced       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_sync_pending ON sync_queue(synced, id ASC);

-- 11. Préférences globales et identifiants chiffrés
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
