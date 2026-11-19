//! Parsing ultra-rapide (`quick-xml`) des flux RSS 2.0, Atom, balises iTunes,
//! Podlove Simple Chapters (`psc:chapters`) et Podcasting 2.0 (`podcast:chapters`,
//! `podcast:transcript`, `podcast:funding`).

use crate::db::extras::NewChapter;
use chrono::DateTime;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

/// Flux RSS/Atom parsé en mémoire.
#[derive(Debug, Clone)]
pub struct ParsedFeed {
    pub title: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub image_url: Option<String>,
    pub funding_url: Option<String>,
    pub episodes: Vec<ParsedEpisode>,
}

/// Épisode extrait d'un flux RSS ou Atom.
#[derive(Debug, Clone)]
pub struct ParsedEpisode {
    pub guid: String,
    pub title: String,
    pub pub_date: i64,
    pub description: Option<String>,
    pub audio_url: String,
    pub mime_type: Option<String>,
    pub file_size: i64,
    pub duration_ms: i64,
    pub chapters_url: Option<String>,
    pub inline_chapters: Vec<NewChapter>,
    pub transcript_url: Option<String>,
    pub transcript_type: Option<String>,
}

/// Segment de transcription synchronisée (SRT, VTT ou Podcasting 2.0 JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptCue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: Option<String>,
    pub text: String,
}

/// Parse un document XML RSS 2.0 ou Atom.
pub fn parse_feed_xml(xml: &str) -> Result<ParsedFeed, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut feed = ParsedFeed {
        title: String::new(),
        description: None,
        author: None,
        language: None,
        image_url: None,
        funding_url: None,
        episodes: Vec::new(),
    };

    let mut buf = Vec::new();
    let mut in_item = false;
    let mut in_channel_image = false;
    let mut current_tag = String::new();
    let mut current_ep: Option<ParsedEpisode> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = local_and_prefixed_name(e);
                match name.as_str() {
                    "item" | "entry" => {
                        in_item = true;
                        current_ep = Some(ParsedEpisode {
                            guid: String::new(),
                            title: String::new(),
                            pub_date: 0,
                            description: None,
                            audio_url: String::new(),
                            mime_type: None,
                            file_size: 0,
                            duration_ms: 0,
                            chapters_url: None,
                            inline_chapters: Vec::new(),
                            transcript_url: None,
                            transcript_type: None,
                        });
                    }
                    "image" if !in_item => {
                        in_channel_image = true;
                    }
                    _ => {
                        handle_tag_attributes(
                            &name,
                            e,
                            in_item,
                            &mut feed,
                            current_ep.as_mut(),
                        );
                    }
                }
                current_tag = name;
            }
            Ok(Event::Empty(ref e)) => {
                let name = local_and_prefixed_name(e);
                handle_tag_attributes(&name, e, in_item, &mut feed, current_ep.as_mut());
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                apply_text_to_state(
                    &current_tag,
                    text,
                    in_item,
                    in_channel_image,
                    &mut feed,
                    current_ep.as_mut(),
                );
            }
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).trim().to_string();
                if text.is_empty() {
                    continue;
                }
                apply_text_to_state(
                    &current_tag,
                    text,
                    in_item,
                    in_channel_image,
                    &mut feed,
                    current_ep.as_mut(),
                );
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                if name == "item" || name == "entry" {
                    in_item = false;
                    if let Some(mut ep) = current_ep.take() {
                        if !ep.audio_url.is_empty() {
                            if ep.guid.is_empty() {
                                ep.guid = ep.audio_url.clone();
                            }
                            if ep.title.is_empty() {
                                ep.title = "Sans titre".to_string();
                            }
                            feed.episodes.push(ep);
                        }
                    }
                } else if name == "image" && !in_item {
                    in_channel_image = false;
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Erreur XML dans le flux RSS : {e}")),
            _ => {}
        }
        buf.clear();
    }

    if feed.title.is_empty() {
        feed.title = "Podcast sans titre".to_string();
    }
    Ok(feed)
}

fn local_and_prefixed_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).to_lowercase()
}

fn get_attr(e: &BytesStart<'_>, target: &str) -> Option<String> {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_lowercase();
        if key == target {
            if let Ok(val) = attr.unescape_value() {
                let s = val.trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

fn handle_tag_attributes(
    name: &str,
    e: &BytesStart<'_>,
    in_item: bool,
    feed: &mut ParsedFeed,
    ep: Option<&mut ParsedEpisode>,
) {
    if !in_item {
        match name {
            "itunes:image" => {
                if let Some(href) = get_attr(e, "href") {
                    feed.image_url = Some(href);
                }
            }
            "podcast:funding" => {
                if let Some(url) = get_attr(e, "url") {
                    feed.funding_url = Some(url);
                }
            }
            _ => {}
        }
        return;
    }

    let Some(ep) = ep else { return };
    match name {
        "enclosure" => {
            if let Some(url) = get_attr(e, "url") {
                let mime = get_attr(e, "type");
                let is_media = mime
                    .as_deref()
                    .map(|m| m.starts_with("audio/") || m.starts_with("video/") || m.contains("octet-stream"))
                    .unwrap_or(true);
                if is_media || ep.audio_url.is_empty() {
                    ep.audio_url = url;
                    ep.mime_type = mime;
                    ep.file_size = get_attr(e, "length")
                        .and_then(|v| v.parse::<i64>().ok())
                        .unwrap_or(0);
                }
            }
        }
        "link" => {
            // Atom enclosure: <link rel="enclosure" href="..." type="audio/mpeg" length="..." />
            if get_attr(e, "rel").as_deref() == Some("enclosure") {
                if let Some(href) = get_attr(e, "href") {
                    ep.audio_url = href;
                    ep.mime_type = get_attr(e, "type");
                    ep.file_size = get_attr(e, "length")
                        .and_then(|v| v.parse::<i64>().ok())
                        .unwrap_or(0);
                }
            }
        }
        "media:content" => {
            if ep.audio_url.is_empty() {
                if let Some(url) = get_attr(e, "url") {
                    ep.audio_url = url;
                    ep.mime_type = get_attr(e, "type");
                }
            }
        }
        "podcast:chapters" => {
            if let Some(url) = get_attr(e, "url") {
                ep.chapters_url = Some(url);
            }
        }
        "podcast:transcript" => {
            // Préfère VTT, SRT ou JSON pour la synchronisation fluide
            if let Some(url) = get_attr(e, "url") {
                let t = get_attr(e, "type").unwrap_or_default();
                let preferred = t.contains("vtt") || t.contains("srt") || t.contains("json");
                if ep.transcript_url.is_none() || preferred {
                    ep.transcript_url = Some(url);
                    ep.transcript_type = Some(t);
                }
            }
        }
        "psc:chapter" => {
            if let (Some(start), Some(title)) = (get_attr(e, "start"), get_attr(e, "title")) {
                ep.inline_chapters.push(NewChapter {
                    start_time_ms: parse_duration_ms(&start),
                    title,
                    url: get_attr(e, "href"),
                    image_url: get_attr(e, "image"),
                });
            }
        }
        _ => {}
    }
}

fn apply_text_to_state(
    tag: &str,
    text: String,
    in_item: bool,
    in_channel_image: bool,
    feed: &mut ParsedFeed,
    ep: Option<&mut ParsedEpisode>,
) {
    if !in_item {
        if in_channel_image {
            if tag == "url" && feed.image_url.is_none() {
                feed.image_url = Some(text);
            }
            return;
        }
        match tag {
            "title" if feed.title.is_empty() => feed.title = text,
            "description" | "subtitle" | "itunes:summary" if feed.description.is_none() => {
                feed.description = Some(text);
            }
            "itunes:author" | "author" | "dc:creator" if feed.author.is_none() => {
                feed.author = Some(text);
            }
            "language" if feed.language.is_none() => feed.language = Some(text),
            _ => {}
        }
        return;
    }

    let Some(ep) = ep else { return };
    match tag {
        "title" if ep.title.is_empty() => ep.title = text,
        "guid" | "id" if ep.guid.is_empty() => ep.guid = text,
        "pubdate" | "published" | "updated" | "dc:date" => {
            let ts = parse_rss_timestamp(&text);
            if ts > 0 {
                ep.pub_date = ts;
            }
        }
        "content:encoded" => {
            // content:encoded est généralement plus riche (HTML complet) que description
            ep.description = Some(text);
        }
        "description" | "summary" | "itunes:summary" => {
            if ep.description.is_none() {
                ep.description = Some(text);
            }
        }
        "itunes:duration" => {
            ep.duration_ms = parse_duration_ms(&text);
        }
        _ => {}
    }
}

/// Parse les dates RSS (RFC 2822) et Atom (RFC 3339 / ISO 8601) en timestamp Unix (secondes).
pub fn parse_rss_timestamp(raw: &str) -> i64 {
    let trimmed = raw.trim();
    if let Ok(dt) = DateTime::parse_from_rfc2822(trimmed) {
        return dt.timestamp();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return dt.timestamp();
    }
    // Variante fréquente avec fuseau "GMT" / "UTC" mal formaté
    let normalized = trimmed.replace("GMT", "+0000").replace("UTC", "+0000");
    if let Ok(dt) = DateTime::parse_from_rfc2822(&normalized) {
        return dt.timestamp();
    }
    0
}

/// Convertit une durée iTunes / Podlove (`HH:MM:SS.mmm`, `MM:SS`, ou secondes brutes) en ms.
pub fn parse_duration_ms(raw: &str) -> i64 {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 0;
    }
    if !trimmed.contains(':') {
        return trimmed
            .replace(',', ".")
            .parse::<f64>()
            .map(|s| (s * 1000.0) as i64)
            .unwrap_or(0)
            .max(0);
    }

    let parts: Vec<&str> = trimmed.split(':').collect();
    let parse_f = |s: &str| s.replace(',', ".").parse::<f64>().unwrap_or(0.0);
    let total_sec = match parts.len() {
        2 => parse_f(parts[0]) * 60.0 + parse_f(parts[1]),
        3 => parse_f(parts[0]) * 3600.0 + parse_f(parts[1]) * 60.0 + parse_f(parts[2]),
        _ => 0.0,
    };
    (total_sec * 1000.0).max(0.0) as i64
}

/// Parse un fichier JSON de chapitres Podcasting 2.0 (`application/json+chapters`).
pub fn parse_podcast2_chapters_json(json: &str) -> Vec<NewChapter> {
    #[derive(Deserialize)]
    struct Root {
        #[serde(default)]
        chapters: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        #[serde(rename = "startTime")]
        start_time: f64,
        #[serde(default)]
        title: String,
        url: Option<String>,
        img: Option<String>,
    }

    let Ok(root) = serde_json::from_str::<Root>(json) else {
        return Vec::new();
    };
    root.chapters
        .into_iter()
        .map(|c| NewChapter {
            start_time_ms: (c.start_time.max(0.0) * 1000.0) as i64,
            title: if c.title.is_empty() {
                "Chapitre".to_string()
            } else {
                c.title
            },
            url: c.url,
            image_url: c.img,
        })
        .collect()
}

/// Parse une transcription Podcasting 2.0 (SRT, WebVTT ou JSON) en segments horodatés.
pub fn parse_transcript(raw: &str, mime_hint: Option<&str>) -> Vec<TranscriptCue> {
    let hint = mime_hint.unwrap_or("").to_lowercase();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    // 1. Format Podcasting 2.0 JSON (`{ "segments": [...] }`)
    if hint.contains("json") || trimmed.starts_with('{') {
        #[derive(Deserialize)]
        struct JsonTranscript {
            #[serde(default)]
            segments: Vec<JsonSeg>,
        }
        #[derive(Deserialize)]
        struct JsonSeg {
            #[serde(rename = "startTime", default)]
            start_time: f64,
            #[serde(rename = "endTime", default)]
            end_time: f64,
            speaker: Option<String>,
            #[serde(default)]
            body: String,
        }
        if let Ok(parsed) = serde_json::from_str::<JsonTranscript>(trimmed) {
            return parsed
                .segments
                .into_iter()
                .filter(|s| !s.body.trim().is_empty())
                .map(|s| TranscriptCue {
                    start_ms: (s.start_time * 1000.0) as i64,
                    end_ms: (s.end_time * 1000.0) as i64,
                    speaker: s.speaker,
                    text: s.body.trim().to_string(),
                })
                .collect();
        }
    }

    // 2. Format SRT ou WebVTT (blocs séparés par ligne vide avec ligne `start --> end`)
    let mut cues = Vec::new();
    let normalized = trimmed.replace("\r\n", "\n");
    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() || lines[0].starts_with("WEBVTT") || lines[0].starts_with("NOTE") {
            continue;
        }
        let Some(arrow_idx) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let timing = lines[arrow_idx];
        let mut split = timing.split("-->");
        let start_str = split.next().unwrap_or("").trim();
        let end_str = split
            .next()
            .unwrap_or("")
            .trim()
            .split_whitespace()
            .next()
            .unwrap_or("");
        let start_ms = parse_duration_ms(start_str);
        let end_ms = parse_duration_ms(end_str);
        let text = lines[arrow_idx + 1..].join(" ").trim().to_string();
        if !text.is_empty() {
            cues.push(TranscriptCue {
                start_ms,
                end_ms,
                speaker: None,
                text,
            });
        }
    }
    cues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rss2_itunes_and_podcasting2_tags() {
        let sample = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
             xmlns:podcast="https://podcastindex.org/namespace/1.0"
             xmlns:psc="http://podlove.org/simple-chapters">
          <channel>
            <title>Tech Hebdo</title>
            <description>Actualités logiciel libre</description>
            <itunes:author>Alice</itunes:author>
            <itunes:image href="https://example.com/cover.jpg" />
            <podcast:funding url="https://liberapay.com/techhebdo">Soutenir</podcast:funding>
            <item>
              <guid>ep-42</guid>
              <title>Épisode 42 — Rust &amp; Audio</title>
              <pubDate>Thu, 19 Nov 2026 08:00:00 +0000</pubDate>
              <enclosure url="https://cdn.example.com/ep42.mp3" length="12345678" type="audio/mpeg" />
              <itunes:duration>01:02:03</itunes:duration>
              <podcast:chapters url="https://cdn.example.com/ep42.chapters.json" type="application/json+chapters" />
              <podcast:transcript url="https://cdn.example.com/ep42.vtt" type="text/vtt" />
              <psc:chapters>
                <psc:chapter start="00:00:00" title="Intro" />
                <psc:chapter start="00:12:30" title="Moteur Audio" />
              </psc:chapters>
            </item>
          </channel>
        </rss>"#;

        let parsed = parse_feed_xml(sample).expect("parsing RSS valide");
        assert_eq!(parsed.title, "Tech Hebdo");
        assert_eq!(parsed.author.as_deref(), Some("Alice"));
        assert_eq!(
            parsed.funding_url.as_deref(),
            Some("https://liberapay.com/techhebdo")
        );
        assert_eq!(parsed.episodes.len(), 1);
        let ep = &parsed.episodes[0];
        assert_eq!(ep.guid, "ep-42");
        assert_eq!(ep.duration_ms, 3_723_000);
        assert_eq!(ep.inline_chapters.len(), 2);
        assert_eq!(ep.inline_chapters[1].start_time_ms, 750_000);
        assert_eq!(
            ep.transcript_url.as_deref(),
            Some("https://cdn.example.com/ep42.vtt")
        );
    }

    #[test]
    fn parses_vtt_and_json_transcripts() {
        let vtt = "WEBVTT\n\n00:00:01.500 --> 00:00:04.000\nBonjour tout le monde\n\n00:00:05.000 --> 00:00:08.500\nBienvenue sur PodDesktop";
        let cues = parse_transcript(vtt, Some("text/vtt"));
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 1500);
        assert_eq!(cues[1].text, "Bienvenue sur PodDesktop");
    }
}
