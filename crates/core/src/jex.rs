use anyhow::{Context, Result};
use chrono::DateTime;
use joplin_domain::{joplin_id, Folder, ModelType, Note, NoteTag, Storage, Tag};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct JexSummary {
    pub folders: usize,
    pub notes: usize,
    pub tags: usize,
    pub note_tags: usize,
}

impl JexSummary {
    pub fn describe_export(&self, path: &Path) -> String {
        format!(
            "Exported {} folders, {} notes, {} tags, and {} note links to {}",
            self.folders,
            self.notes,
            self.tags,
            self.note_tags,
            path.display()
        )
    }

    pub fn describe_import(&self, path: &Path) -> String {
        format!(
            "Imported {} folders, {} notes, {} tags, and {} note links from {}",
            self.folders,
            self.notes,
            self.tags,
            self.note_tags,
            path.display()
        )
    }
}

pub async fn export_jex<S: Storage>(storage: &S, path: &Path) -> Result<JexSummary> {
    let all_notes = storage.list_notes(None).await?;
    let folders = storage.list_folders().await?;
    let tags = storage.list_tags().await?;
    let note_tags = storage.get_note_tags_updated_since(0).await?;

    let file = File::create(path)
        .with_context(|| format!("Failed to create JEX archive at {}", path.display()))?;
    let mut builder = tar::Builder::new(file);

    for folder in &folders {
        append_tar_entry(
            &mut builder,
            &format!("{}.md", folder.id),
            serialize_folder(folder)?,
        )?;
    }
    for note in &all_notes {
        append_tar_entry(
            &mut builder,
            &format!("{}.md", note.id),
            serialize_note(note)?,
        )?;
    }
    for tag in &tags {
        append_tar_entry(&mut builder, &format!("{}.md", tag.id), serialize_tag(tag)?)?;
    }
    for note_tag in &note_tags {
        append_tar_entry(
            &mut builder,
            &format!("{}.md", note_tag.id),
            serialize_note_tag(note_tag)?,
        )?;
    }

    builder.finish().context("Failed to finalise JEX archive")?;

    Ok(JexSummary {
        folders: folders.len(),
        notes: all_notes.len(),
        tags: tags.len(),
        note_tags: note_tags.len(),
    })
}

pub async fn import_jex<S: Storage>(storage: &S, path: &Path) -> Result<JexSummary> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open JEX archive {}", path.display()))?;
    let mut archive = tar::Archive::new(file);

    let mut folders = Vec::new();
    let mut notes = Vec::new();
    let mut tags = Vec::new();
    let mut note_tags = Vec::new();

    for entry in archive.entries().context("Failed to read JEX entries")? {
        let mut entry = entry.context("Failed to read JEX entry")?;
        let path = entry.path().context("Failed to read JEX entry path")?;
        let path_str = path.to_string_lossy().to_string();
        if path_str.starts_with("resources/") || !path_str.ends_with(".md") {
            continue;
        }

        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .with_context(|| format!("Failed to read {}", path_str))?;

        match parse_item(&content)? {
            ParsedItem::Folder(item) => folders.push(item),
            ParsedItem::Note(item) => notes.push(*item),
            ParsedItem::Tag(item) => tags.push(item),
            ParsedItem::NoteTag(item) => note_tags.push(item),
        }
    }

    let mut folder_id_map = HashMap::new();
    for folder in &mut folders {
        if storage.get_folder(&folder.id).await?.is_some() {
            let old_id = folder.id.clone();
            folder.id = joplin_id();
            folder_id_map.insert(old_id, folder.id.clone());
        }
    }
    for folder in &mut folders {
        if let Some(new_parent_id) = folder_id_map.get(&folder.parent_id) {
            folder.parent_id = new_parent_id.clone();
        }
    }

    let mut note_id_map = HashMap::new();
    for note in &mut notes {
        if storage.get_note(&note.id).await?.is_some() {
            let old_id = note.id.clone();
            note.id = joplin_id();
            note_id_map.insert(old_id, note.id.clone());
        }
        if let Some(new_parent_id) = folder_id_map.get(&note.parent_id) {
            note.parent_id = new_parent_id.clone();
        }
    }

    let mut tag_id_map = HashMap::new();
    for tag in &mut tags {
        if storage.get_tag(&tag.id).await?.is_some() {
            let old_id = tag.id.clone();
            tag.id = joplin_id();
            tag_id_map.insert(old_id, tag.id.clone());
        }
    }

    for note_tag in &mut note_tags {
        if let Some(new_note_id) = note_id_map.get(&note_tag.note_id) {
            note_tag.note_id = new_note_id.clone();
        }
        if let Some(new_tag_id) = tag_id_map.get(&note_tag.tag_id) {
            note_tag.tag_id = new_tag_id.clone();
        }
        note_tag.id = joplin_id();
    }

    for folder in &folders {
        storage.create_folder(folder).await?;
    }
    for note in &notes {
        storage.create_note(note).await?;
    }
    for tag in &tags {
        storage.create_tag(tag).await?;
    }
    for note_tag in &note_tags {
        if storage.get_note(&note_tag.note_id).await?.is_some()
            && storage.get_tag(&note_tag.tag_id).await?.is_some()
        {
            storage.add_note_tag(note_tag).await?;
        }
    }

    Ok(JexSummary {
        folders: folders.len(),
        notes: notes.len(),
        tags: tags.len(),
        note_tags: note_tags.len(),
    })
}

fn append_tar_entry<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    content: String,
) -> Result<()> {
    let bytes = content.into_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, path, Cursor::new(bytes))
        .with_context(|| format!("Failed to add {} to JEX archive", path))?;
    Ok(())
}

fn serialize_note(note: &Note) -> Result<String> {
    serialize_item(
        Some(&note.title),
        Some(&note.body),
        &[
            ("id", note.id.clone()),
            ("parent_id", note.parent_id.clone()),
            ("created_time", note.created_time.to_string()),
            ("updated_time", note.updated_time.to_string()),
            ("user_created_time", note.user_created_time.to_string()),
            ("user_updated_time", note.user_updated_time.to_string()),
            ("is_shared", note.is_shared.to_string()),
            ("share_id", note.share_id.clone().unwrap_or_default()),
            (
                "master_key_id",
                note.master_key_id.clone().unwrap_or_default(),
            ),
            ("encryption_applied", note.encryption_applied.to_string()),
            (
                "encryption_cipher_text",
                note.encryption_cipher_text.clone().unwrap_or_default(),
            ),
            ("is_conflict", note.is_conflict.to_string()),
            ("is_todo", note.is_todo.to_string()),
            ("todo_completed", note.todo_completed.to_string()),
            ("todo_due", note.todo_due.to_string()),
            ("source", note.source.clone()),
            ("source_application", note.source_application.clone()),
            ("order", note.order.to_string()),
            ("latitude", note.latitude.to_string()),
            ("longitude", note.longitude.to_string()),
            ("altitude", note.altitude.to_string()),
            ("author", note.author.clone()),
            ("source_url", note.source_url.clone()),
            ("application_data", note.application_data.clone()),
            ("markup_language", note.markup_language.to_string()),
            (
                "encryption_blob_encrypted",
                note.encryption_blob_encrypted.to_string(),
            ),
            ("conflict_original_id", note.conflict_original_id.clone()),
            ("deleted_time", note.deleted_time.to_string()),
            ("type_", (ModelType::Note as i32).to_string()),
        ],
    )
}

fn serialize_folder(folder: &Folder) -> Result<String> {
    serialize_item(
        Some(&folder.title),
        None,
        &[
            ("id", folder.id.clone()),
            ("parent_id", folder.parent_id.clone()),
            ("created_time", folder.created_time.to_string()),
            ("updated_time", folder.updated_time.to_string()),
            ("user_created_time", folder.user_created_time.to_string()),
            ("user_updated_time", folder.user_updated_time.to_string()),
            ("is_shared", folder.is_shared.to_string()),
            ("share_id", folder.share_id.clone().unwrap_or_default()),
            (
                "master_key_id",
                folder.master_key_id.clone().unwrap_or_default(),
            ),
            ("encryption_applied", folder.encryption_applied.to_string()),
            (
                "encryption_cipher_text",
                folder.encryption_cipher_text.clone().unwrap_or_default(),
            ),
            ("icon", folder.icon.clone()),
            ("type_", (ModelType::Folder as i32).to_string()),
        ],
    )
}

fn serialize_tag(tag: &Tag) -> Result<String> {
    serialize_item(
        Some(&tag.title),
        None,
        &[
            ("id", tag.id.clone()),
            ("created_time", tag.created_time.to_string()),
            ("updated_time", tag.updated_time.to_string()),
            ("user_created_time", tag.user_created_time.to_string()),
            ("user_updated_time", tag.user_updated_time.to_string()),
            ("parent_id", tag.parent_id.clone()),
            ("is_shared", tag.is_shared.to_string()),
            ("type_", (ModelType::Tag as i32).to_string()),
        ],
    )
}

fn serialize_note_tag(note_tag: &NoteTag) -> Result<String> {
    serialize_item(
        None,
        None,
        &[
            ("id", note_tag.id.clone()),
            ("note_id", note_tag.note_id.clone()),
            ("tag_id", note_tag.tag_id.clone()),
            ("created_time", note_tag.created_time.to_string()),
            ("updated_time", note_tag.updated_time.to_string()),
            ("is_shared", note_tag.is_shared.to_string()),
            ("type_", (ModelType::NoteTag as i32).to_string()),
        ],
    )
}

fn serialize_item(
    title: Option<&str>,
    body: Option<&str>,
    props: &[(&str, String)],
) -> Result<String> {
    let mut chunks = Vec::new();
    if let Some(title) = title {
        chunks.push(title.to_string());
    }
    if let Some(body) = body {
        if title.is_some() {
            chunks.push(body.to_string());
        }
    }

    let props_text = props
        .iter()
        .map(|(key, value)| format!("{}: {}", key, escape_prop_value(value)))
        .collect::<Vec<_>>()
        .join("\n");

    if !props_text.is_empty() {
        chunks.push(props_text);
    }

    Ok(chunks.join("\n\n"))
}

fn escape_prop_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape_prop_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

enum ParsedItem {
    Note(Box<Note>),
    Folder(Folder),
    Tag(Tag),
    NoteTag(NoteTag),
}

fn parse_item(content: &str) -> Result<ParsedItem> {
    let lines: Vec<&str> = content.lines().collect();
    let mut props = HashMap::new();
    let mut property_start_idx = lines.len(); // Default: properties start at end (no prelude)

    // Find the boundary between prelude (title/body) and properties
    // Properties are lines with colons that look like "key: value"
    // We scan from the end backwards to find the first property line
    for idx in (0..lines.len()).rev() {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() {
            // Empty line marks the boundary - properties start after this
            property_start_idx = idx + 1;
            break;
        }

        // Check if this line is a property (contains colon)
        if let Some(colon_idx) = trimmed.find(':') {
            let key = trimmed[..colon_idx].trim();
            // Only treat as property if key looks valid (not empty, not a markdown header, etc.)
            if !key.is_empty() && !key.starts_with('#') {
                let value = unescape_prop_value(trimmed[colon_idx + 1..].trim());
                props.insert(key.to_string(), value);
                // This line is a property, so properties start at or before this index
                property_start_idx = idx;
                continue;
            }
        }

        // If this line doesn't look like a property, it's part of the prelude
        // and properties start after the last property we found
        break;
    }

    let prelude = &lines[..property_start_idx];

    let type_ = props
        .get("type_")
        .context("Missing type_ in JEX item")?
        .parse::<i32>()
        .context("Invalid type_ in JEX item")?;

    let title = prelude
        .first()
        .map(|value| (*value).to_string())
        .unwrap_or_default();
    let body = if type_ == ModelType::Note as i32 && prelude.len() > 2 {
        prelude[2..].join("\n")
    } else {
        String::new()
    };

    // Handle Joplin Desktop export bug: Resources may be exported as type 4 (NoteTag)
    // instead of type 5. Check if this is actually a Resource by looking for mime/size fields.
    let actual_type = if type_ == ModelType::NoteTag as i32
        && (props.contains_key("mime") || props.contains_key("size"))
    {
        // This is a Resource mislabeled as NoteTag
        ModelType::Resource as i32
    } else {
        type_
    };

    Ok(match actual_type {
        x if x == ModelType::Note as i32 => ParsedItem::Note(Box::new(Note {
            title,
            body,
            ..parse_note_props(&props)?
        })),
        x if x == ModelType::Folder as i32 => ParsedItem::Folder(Folder {
            title,
            ..parse_folder_props(&props)?
        }),
        // Handle both ModelType::Tag (3) and Joplin Desktop export Tag type (13)
        x if x == ModelType::Tag as i32 || x == 13 => ParsedItem::Tag(Tag {
            title,
            ..parse_tag_props(&props)?
        }),
        x if x == ModelType::NoteTag as i32 => {
            // Only parse as NoteTag if it has the required fields
            if props.contains_key("note_id") && props.contains_key("tag_id") {
                ParsedItem::NoteTag(parse_note_tag_props(&props)?)
            } else {
                // Skip malformed NoteTag entries (Joplin Desktop export bug)
                tracing::warn!("Skipping malformed NoteTag entry (missing note_id or tag_id)");
                return Ok(ParsedItem::Note(Box::new(Note::default())));
            }
        }
        x if x == ModelType::Resource as i32 => {
            // Parse as Resource - need to add Resource support
            // For now, skip resources as they require additional handling
            tracing::warn!(
                "Skipping Resource entry (type 5) - Resource import not yet implemented"
            );
            return Ok(ParsedItem::Note(Box::new(Note::default())));
        }
        _ => anyhow::bail!("Unsupported JEX item type: {}", actual_type),
    })
}

fn parse_note_props(props: &HashMap<String, String>) -> Result<Note> {
    Ok(Note {
        id: required(props, "id")?,
        parent_id: string_prop(props, "parent_id"),
        created_time: int_prop(props, "created_time")?,
        updated_time: int_prop(props, "updated_time")?,
        user_created_time: int_prop(props, "user_created_time")?,
        user_updated_time: int_prop(props, "user_updated_time")?,
        is_shared: int_prop_i32(props, "is_shared")?,
        share_id: optional_string_prop(props, "share_id"),
        master_key_id: optional_string_prop(props, "master_key_id"),
        encryption_applied: int_prop_i32(props, "encryption_applied")?,
        encryption_cipher_text: optional_string_prop(props, "encryption_cipher_text"),
        is_conflict: int_prop_i32(props, "is_conflict")?,
        is_todo: int_prop_i32(props, "is_todo")?,
        todo_completed: int_prop(props, "todo_completed")?,
        todo_due: int_prop(props, "todo_due")?,
        source: string_prop(props, "source"),
        source_application: string_prop(props, "source_application"),
        order: numeric_prop(props, "order")?,
        latitude: numeric_prop(props, "latitude")?,
        longitude: numeric_prop(props, "longitude")?,
        altitude: numeric_prop(props, "altitude")?,
        author: string_prop(props, "author"),
        source_url: string_prop(props, "source_url"),
        application_data: string_prop(props, "application_data"),
        markup_language: int_prop_i32(props, "markup_language")?,
        encryption_blob_encrypted: int_prop_i32(props, "encryption_blob_encrypted")?,
        conflict_original_id: string_prop(props, "conflict_original_id"),
        deleted_time: int_prop(props, "deleted_time")?,
        ..Default::default()
    })
}

fn parse_folder_props(props: &HashMap<String, String>) -> Result<Folder> {
    Ok(Folder {
        id: required(props, "id")?,
        parent_id: string_prop(props, "parent_id"),
        created_time: int_prop(props, "created_time")?,
        updated_time: int_prop(props, "updated_time")?,
        user_created_time: int_prop(props, "user_created_time")?,
        user_updated_time: int_prop(props, "user_updated_time")?,
        is_shared: int_prop_i32(props, "is_shared")?,
        share_id: optional_string_prop(props, "share_id"),
        master_key_id: optional_string_prop(props, "master_key_id"),
        encryption_applied: int_prop_i32(props, "encryption_applied")?,
        encryption_cipher_text: optional_string_prop(props, "encryption_cipher_text"),
        icon: string_prop(props, "icon"),
        ..Default::default()
    })
}

fn parse_tag_props(props: &HashMap<String, String>) -> Result<Tag> {
    Ok(Tag {
        id: required(props, "id")?,
        created_time: int_prop(props, "created_time")?,
        updated_time: int_prop(props, "updated_time")?,
        user_created_time: int_prop(props, "user_created_time")?,
        user_updated_time: int_prop(props, "user_updated_time")?,
        parent_id: string_prop(props, "parent_id"),
        is_shared: int_prop_i32(props, "is_shared")?,
        ..Default::default()
    })
}

fn parse_note_tag_props(props: &HashMap<String, String>) -> Result<NoteTag> {
    Ok(NoteTag {
        id: required(props, "id")?,
        note_id: required(props, "note_id")?,
        tag_id: required(props, "tag_id")?,
        created_time: int_prop(props, "created_time")?,
        updated_time: int_prop(props, "updated_time")?,
        is_shared: int_prop_i32(props, "is_shared")?,
        ..Default::default()
    })
}

fn required(props: &HashMap<String, String>, key: &str) -> Result<String> {
    props
        .get(key)
        .cloned()
        .with_context(|| format!("Missing {} in JEX item", key))
}

fn string_prop(props: &HashMap<String, String>, key: &str) -> String {
    props.get(key).cloned().unwrap_or_default()
}

fn optional_string_prop(props: &HashMap<String, String>, key: &str) -> Option<String> {
    props.get(key).cloned().filter(|value| !value.is_empty())
}

fn int_prop(props: &HashMap<String, String>, key: &str) -> Result<i64> {
    let value = props.get(key).map(String::as_str).unwrap_or("0");

    // Handle empty string
    if value.is_empty() {
        return Ok(0);
    }

    // Try parsing as integer first (milliseconds since epoch, coordinates in microdegrees)
    if let Ok(parsed) = value.parse::<i64>() {
        return Ok(parsed);
    }

    // Try parsing as ISO 8601 timestamp (e.g., "2025-04-06T15:47:47.008Z")
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.timestamp_millis());
    }

    // Try parsing without timezone (treat as UTC)
    if let Ok(dt) = DateTime::parse_from_rfc2822(value) {
        return Ok(dt.timestamp_millis());
    }

    Err(anyhow::anyhow!("Invalid {} in JEX item: '{}'", key, value))
}

/// Parse a coordinate or numeric value that can be a float string
/// Joplin stores coordinates as floats in JEX but as integers (microdegrees * 1e6) in database
/// This function handles both integer and float string formats
fn numeric_prop(props: &HashMap<String, String>, key: &str) -> Result<i64> {
    let value = props.get(key).map(String::as_str).unwrap_or("0");

    // Handle empty string
    if value.is_empty() {
        return Ok(0);
    }

    // Try parsing as integer first
    if let Ok(parsed) = value.parse::<i64>() {
        return Ok(parsed);
    }

    // Try parsing as float and convert to integer (for coordinates stored as microdegrees)
    // JEX format: latitude/longitude as decimal degrees (e.g., "50.11092210")
    // Database: stored as numeric (INTEGER in SQLite, but can store floats)
    // For compatibility, we'll multiply by 1e6 and round to get microdegrees as integer
    if let Ok(parsed_float) = value.parse::<f64>() {
        return Ok((parsed_float * 1_000_000.0) as i64);
    }

    Err(anyhow::anyhow!("Invalid {} in JEX item: '{}'", key, value))
}

fn int_prop_i32(props: &HashMap<String, String>, key: &str) -> Result<i32> {
    let value = props.get(key).map(String::as_str).unwrap_or("0");

    if value.is_empty() {
        return Ok(0);
    }

    value
        .parse::<i32>()
        .with_context(|| format!("Invalid {} in JEX item", key))
}
