//! Import/Export OPML 2.0 (abonnements + arborescence de tags/dossiers)
//! et Export des Favoris (HTML, Markdown, JSON).

use crate::models::{Episode, Feed, Tag};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};
use std::collections::HashMap;
use std::io::Cursor;

/// Entrée d'abonnement extraite d'un fichier OPML.
#[derive(Debug, Clone)]
pub struct OpmlEntry {
    pub title: String,
    pub feed_url: String,
    pub tags: Vec<String>,
}

/// Parse un fichier OPML (compatible AntennaPod, Pocket Casts, Overcast, Feedly).
/// Conserve les dossiers/catégories parentes sous forme de tags.
pub fn parse_opml(xml: &str) -> Result<Vec<OpmlEntry>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut folder_stack: Vec<String> = Vec::new();
    let mut entries: Vec<OpmlEntry> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref().eq_ignore_ascii_case(b"outline") => {
                let (title, xml_url, category) = extract_outline_attrs(e);
                if let Some(url) = xml_url {
                    let mut tags = folder_stack.clone();
                    if let Some(cat) = category {
                        for c in cat.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                            if !tags.iter().any(|t| t.eq_ignore_ascii_case(c)) {
                                tags.push(c.to_string());
                            }
                        }
                    }
                    entries.push(OpmlEntry {
                        title: title.unwrap_or_else(|| url.clone()),
                        feed_url: url,
                        tags,
                    });
                    // Pousse un marqueur vide pour équilibrer le Event::End correspondant
                    folder_stack.push(String::new());
                } else {
                    folder_stack.push(title.unwrap_or_default());
                }
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref().eq_ignore_ascii_case(b"outline") => {
                let (title, xml_url, category) = extract_outline_attrs(e);
                if let Some(url) = xml_url {
                    let mut tags: Vec<String> = folder_stack
                        .iter()
                        .filter(|s| !s.is_empty())
                        .cloned()
                        .collect();
                    if let Some(cat) = category {
                        for c in cat.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                            if !tags.iter().any(|t| t.eq_ignore_ascii_case(c)) {
                                tags.push(c.to_string());
                            }
                        }
                    }
                    entries.push(OpmlEntry {
                        title: title.unwrap_or_else(|| url.clone()),
                        feed_url: url,
                        tags,
                    });
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref().eq_ignore_ascii_case(b"outline") => {
                folder_stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Erreur lecture OPML : {e}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(entries)
}

fn extract_outline_attrs(e: &BytesStart<'_>) -> (Option<String>, Option<String>, Option<String>) {
    let mut title = None;
    let mut text = None;
    let mut xml_url = None;
    let mut category = None;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_lowercase();
        if let Ok(val) = attr.unescape_value() {
            let s = val.trim().to_string();
            if s.is_empty() {
                continue;
            }
            match key.as_str() {
                "title" => title = Some(s),
                "text" => text = Some(s),
                "xmlurl" => xml_url = Some(s),
                "category" => category = Some(s),
                _ => {}
            }
        }
    }

    (title.or(text), xml_url, category)
}

/// Génère un fichier OPML 2.0 propre groupé par tags/dossiers (100 % compatible AntennaPod).
pub fn generate_opml(feeds: &[Feed], tags: &[Tag]) -> Result<String, String> {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(|e| e.to_string())?;

    let mut opml = BytesStart::new("opml");
    opml.push_attribute(("version", "2.0"));
    writer.write_event(Event::Start(opml)).map_err(|e| e.to_string())?;

    writer
        .write_event(Event::Start(BytesStart::new("head")))
        .map_err(|e| e.to_string())?;
    writer
        .write_event(Event::Start(BytesStart::new("title")))
        .map_err(|e| e.to_string())?;
    writer
        .write_event(Event::Text(quick_xml::events::BytesText::new(
            "PodDesktop Subscriptions",
        )))
        .map_err(|e| e.to_string())?;
    writer
        .write_event(Event::End(BytesEnd::new("title")))
        .map_err(|e| e.to_string())?;
    writer
        .write_event(Event::End(BytesEnd::new("head")))
        .map_err(|e| e.to_string())?;

    writer
        .write_event(Event::Start(BytesStart::new("body")))
        .map_err(|e| e.to_string())?;

    let tag_map: HashMap<i64, &str> = tags.iter().map(|t| (t.id, t.name.as_str())).collect();

    // Exporte uniquement les flux RSS distants (pas les dossiers locaux virtuels)
    let rss_feeds: Vec<&Feed> = feeds.iter().filter(|f| !f.is_local_folder).collect();

    // 1. Flux sans tag directement à la racine
    for f in rss_feeds.iter().filter(|f| f.tag_ids.is_empty()) {
        write_feed_outline(&mut writer, f, None)?;
    }

    // 2. Flux groupés par dossier/tag
    for tag in tags {
        let tagged: Vec<&&Feed> = rss_feeds
            .iter()
            .filter(|f| f.tag_ids.contains(&tag.id))
            .collect();
        if tagged.is_empty() {
            continue;
        }
        let mut folder = BytesStart::new("outline");
        folder.push_attribute(("text", tag.name.as_str()));
        folder.push_attribute(("title", tag.name.as_str()));
        writer
            .write_event(Event::Start(folder))
            .map_err(|e| e.to_string())?;
        for f in tagged {
            let cat_list: String = f
                .tag_ids
                .iter()
                .filter_map(|id| tag_map.get(id).copied())
                .collect::<Vec<_>>()
                .join(",");
            write_feed_outline(&mut writer, f, Some(&cat_list))?;
        }
        writer
            .write_event(Event::End(BytesEnd::new("outline")))
            .map_err(|e| e.to_string())?;
    }

    writer
        .write_event(Event::End(BytesEnd::new("body")))
        .map_err(|e| e.to_string())?;
    writer
        .write_event(Event::End(BytesEnd::new("opml")))
        .map_err(|e| e.to_string())?;

    String::from_utf8(writer.into_inner().into_inner()).map_err(|e| e.to_string())
}

fn write_feed_outline(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    feed: &Feed,
    category: Option<&str>,
) -> Result<(), String> {
    let mut node = BytesStart::new("outline");
    node.push_attribute(("type", "rss"));
    node.push_attribute(("text", feed.title.as_str()));
    node.push_attribute(("title", feed.title.as_str()));
    node.push_attribute(("xmlUrl", feed.feed_url.as_str()));
    if let Some(cat) = category.filter(|s| !s.is_empty()) {
        node.push_attribute(("category", cat));
    }
    writer
        .write_event(Event::Empty(node))
        .map_err(|e| e.to_string())
}

/// Exporte la liste des épisodes Favoris au format `"html"`, `"markdown"` ou `"json"`.
pub fn export_favorites(episodes: &[Episode], format: &str) -> Result<String, String> {
    match format.to_lowercase().as_str() {
        "json" => serde_json::to_string_pretty(episodes)
            .map_err(|e| format!("Erreur export JSON favoris : {e}")),
        "markdown" | "md" => {
            let mut out = String::from("# PodDesktop — Favoris\n\n");
            for ep in episodes {
                let date = chrono::DateTime::from_timestamp(ep.pub_date, 0)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- **[{}]({})** — *{}* ({})\n",
                    ep.title, ep.audio_url, ep.feed_title, date
                ));
            }
            Ok(out)
        }
        _ => {
            let mut out = String::from(
                "<!DOCTYPE html><html lang=\"fr\"><head><meta charset=\"UTF-8\">\
                 <title>PodDesktop — Favoris</title>\
                 <style>body{font-family:system-ui,sans-serif;max-width:860px;margin:2rem auto;padding:0 1rem;color:#111;}\
                 li{margin:.6rem 0;}small{color:#555;}</style></head><body><h1>PodDesktop — Favoris</h1><ul>",
            );
            for ep in episodes {
                let title = escape_html(&ep.title);
                let feed = escape_html(&ep.feed_title);
                let url = escape_html(&ep.audio_url);
                out.push_str(&format!(
                    "<li><a href=\"{url}\"><strong>{title}</strong></a> — <small>{feed}</small></li>"
                ));
            }
            out.push_str("</ul></body></html>");
            Ok(out)
        }
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_opml_preserves_folders_as_tags() {
        let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <opml version="2.0">
          <body>
            <outline text="Tech">
              <outline type="rss" text="Rustaceans" xmlUrl="https://rust.example/rss.xml" />
            </outline>
            <outline type="rss" text="Histoire" xmlUrl="https://history.example/feed" />
          </body>
        </opml>"#;
        let parsed = parse_opml(opml).expect("OPML valide");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].tags, vec!["Tech".to_string()]);
        assert!(parsed[1].tags.is_empty());
    }
}
