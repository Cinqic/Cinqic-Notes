use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NoteFormat {
    Markdown,
    Text,
}

impl NoteFormat {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "md" => Some(Self::Markdown),
            "txt" => Some(Self::Text),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Text => "txt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummary {
    pub id: String,
    pub path: String,
    pub title: String,
    pub format: NoteFormat,
    pub preview: String,
    pub hash: String,
    pub modified_at: String,
    pub created_at: String,
    pub archived: bool,
    pub trashed: bool,
    pub project: bool,
    pub tags: Vec<String>,
    pub task_count: i64,
    pub open_task_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDocument {
    #[serde(flatten)]
    pub summary: NoteSummary,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub path: String,
    pub note_count: i64,
    pub last_indexed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    pub id: String,
    pub note_path: String,
    pub note_title: String,
    pub line: i64,
    pub checked: bool,
    pub text: String,
    pub project: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklinkItem {
    pub source_path: String,
    pub source_title: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkItem {
    pub source_path: String,
    pub source_title: String,
    pub target_path: String,
    pub target_title: Option<String>,
    pub kind: String,
    pub label: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagItem {
    pub tag: String,
    pub note_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentItem {
    pub path: String,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryDraftInfo {
    pub path: String,
    pub note_path: String,
    pub created_at: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityInfo {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub path: String,
    pub title: String,
    pub project: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub path: String,
    pub local_content: String,
    pub disk_content: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionItem {
    pub id: String,
    pub note_path: String,
    pub actor: String,
    pub reason: String,
    pub created_at: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ParsedNote {
    pub title: String,
    pub tags: Vec<String>,
    pub project: bool,
    pub tasks: Vec<TaskSeed>,
    pub links: Vec<LinkSeed>,
}

#[derive(Debug, Clone)]
pub struct TaskSeed {
    pub line: i64,
    pub checked: bool,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct LinkSeed {
    pub target: String,
    pub kind: String,
    pub label: String,
}

pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn parse_note(path: &Path, content: &str) -> ParsedNote {
    let mut title = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled")
        .replace(['_', '-'], " ");
    let mut project = false;
    let mut in_frontmatter = false;
    // Which frontmatter key's block list we are currently inside, if any. Without
    // this, every `- item` in the frontmatter was collected as a tag, so an
    // `authors:` or `aliases:` list silently became tags.
    let mut frontmatter_key: Option<String> = None;
    let mut heading_seen = false;
    let mut tags = Vec::new();
    let mut tasks = Vec::new();
    let mut links = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if index == 0 && trimmed == "---" {
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter && trimmed == "---" {
            in_frontmatter = false;
            frontmatter_key = None;
            continue;
        }
        if in_frontmatter {
            if trimmed.is_empty() {
                continue;
            }
            // A block-list entry belongs to the key that opened the list.
            if let Some(value) = trimmed.strip_prefix("- ").or_else(|| {
                (trimmed == "-")
                    .then_some("")
                    .or_else(|| trimmed.strip_prefix('-'))
            }) {
                if frontmatter_key.as_deref() == Some("tags") {
                    push_tag(value, &mut tags);
                }
                continue;
            }
            if let Some((key, value)) = split_frontmatter_entry(trimmed) {
                frontmatter_key = Some(key.to_ascii_lowercase());
                match key.to_ascii_lowercase().as_str() {
                    "type" => project = value.eq_ignore_ascii_case("project"),
                    "tags" => {
                        // Inline forms: `tags: [a, b]` and `tags: a, b`.
                        let inline = value.trim();
                        let inline = inline
                            .strip_prefix('[')
                            .and_then(|rest| rest.strip_suffix(']'))
                            .unwrap_or(inline);
                        for entry in inline.split(',') {
                            push_tag(entry, &mut tags);
                        }
                    }
                    _ => {}
                }
                continue;
            }
            continue;
        }
        if !heading_seen && title_from_heading(&mut title, line) {
            heading_seen = true;
        }
        if let Some(task) = parse_task(index as i64 + 1, line) {
            tasks.push(task);
        }
        collect_tags(line, &mut tags);
        collect_links(line, &mut links);
    }

    tags.sort();
    tags.dedup();
    ParsedNote {
        title,
        tags,
        project,
        tasks,
        links,
    }
}

fn title_from_heading(title: &mut String, line: &str) -> bool {
    let Some(value) = line.strip_prefix("# ") else {
        return false;
    };
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    *title = value.to_owned();
    true
}

fn parse_task(line_number: i64, line: &str) -> Option<TaskSeed> {
    let trimmed = line.trim_start();
    let marker = trimmed.get(0..6)?;
    if !(marker.starts_with("- [") || marker.starts_with("* [")) || !marker.ends_with("] ") {
        return None;
    }
    let checked = matches!(marker.as_bytes().get(3), Some(b'x' | b'X'));
    Some(TaskSeed {
        line: line_number,
        checked,
        text: trimmed[6..].trim().to_owned(),
    })
}

/// Split a `key: value` frontmatter entry, ignoring lines that are not one.
fn split_frontmatter_entry(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.contains(' ') || key.starts_with('#') {
        return None;
    }
    Some((key, value.trim()))
}

/// Record one frontmatter tag, matching the `#tag` character set used in the body.
fn push_tag(value: &str, tags: &mut Vec<String>) {
    let value = value.trim().trim_matches(['"', '\'']).trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_' || character == '-')
    {
        return;
    }
    tags.push(value.to_ascii_lowercase());
}

fn collect_tags(line: &str, tags: &mut Vec<String>) {
    for (index, character) in line.char_indices() {
        let preceded_by_space = index == 0
            || line[..index]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        if character != '#' || !preceded_by_space {
            continue;
        }
        let value = line[index + 1..]
            .chars()
            .take_while(|value| value.is_alphanumeric() || *value == '_' || *value == '-')
            .collect::<String>();
        if !(value.is_empty() || index == 0 && line.starts_with("# ")) {
            tags.push(value.to_ascii_lowercase());
        }
    }
}

fn collect_links(line: &str, links: &mut Vec<LinkSeed>) {
    let mut rest = line;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let label = after[..end].trim();
        if !label.is_empty() {
            links.push(LinkSeed {
                target: label.to_owned(),
                kind: "wiki".into(),
                label: label.into(),
            });
        }
        rest = &after[end + 2..];
    }
    rest = line;
    while let Some(start) = rest.find("](") {
        let label_start = rest[..start].rfind('[').unwrap_or(start);
        let label = &rest[label_start + 1..start];
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else { break };
        let target = after[..end].trim().trim_matches('<');
        if !target.starts_with("http://")
            && !target.starts_with("https://")
            && !target.starts_with('#')
            && !target.is_empty()
        {
            links.push(LinkSeed {
                target: target.to_owned(),
                kind: "markdown".into(),
                label: label.to_owned(),
            });
        }
        rest = &after[end + 1..];
    }
}

pub fn normalized_link_target(value: &str) -> String {
    let value = value
        .split('#')
        .next()
        .unwrap_or(value)
        .trim()
        .replace('\\', "/");
    let value = value.strip_prefix("./").unwrap_or(&value);
    if value.to_ascii_lowercase().ends_with(".md") || value.to_ascii_lowercase().ends_with(".txt") {
        value.to_owned()
    } else {
        format!("{value}.md")
    }
}

pub fn preview(content: &str) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .unwrap_or("")
        .trim()
        .chars()
        .take(140)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse(content: &str) -> ParsedNote {
        parse_note(Path::new("Notes/Sample.md"), content)
    }

    #[test]
    fn frontmatter_tags_come_only_from_the_tags_key() {
        let note = parse(
            "---\ntype: project\nauthors:\n  - alice\n  - bob\ntags:\n  - work\n  - urgent\n---\n\n# Sample\n",
        );
        assert_eq!(note.tags, vec!["urgent", "work"]);
        assert!(note.project);
    }

    #[test]
    fn an_unrelated_yaml_list_is_not_treated_as_tags() {
        let note = parse("---\naliases:\n  - Alias One\n  - second\nrelated:\n  - other\n---\n");
        assert!(
            note.tags.is_empty(),
            "unexpected tags from unrelated lists: {:?}",
            note.tags
        );
    }

    #[test]
    fn inline_frontmatter_tag_forms_are_supported() {
        assert_eq!(
            parse("---\ntags: [alpha, beta]\n---\n").tags,
            vec!["alpha", "beta"]
        );
        assert_eq!(
            parse("---\ntags: alpha, beta\n---\n").tags,
            vec!["alpha", "beta"]
        );
        assert_eq!(parse("---\ntags: \"quoted\"\n---\n").tags, vec!["quoted"]);
        assert_eq!(parse("---\ntags: #hashed\n---\n").tags, vec!["hashed"]);
    }

    #[test]
    fn frontmatter_tags_reject_values_that_are_not_tag_shaped() {
        let note = parse("---\ntags:\n  - two words\n  - ok-tag\n  - \"\"\n---\n");
        assert_eq!(note.tags, vec!["ok-tag"]);
    }

    #[test]
    fn frontmatter_is_only_recognised_at_the_top_of_the_file() {
        // A horizontal rule mid-document must not open a frontmatter block.
        let note = parse("# Sample\n\n---\n\ntags:\n  - notatag\n");
        assert!(note.tags.is_empty(), "{:?}", note.tags);
    }

    #[test]
    fn body_hash_tags_are_collected_but_headings_are_not() {
        let note = parse("# Heading\n\nSome text #alpha and #beta-two.\n\n## Another\n");
        assert_eq!(note.tags, vec!["alpha", "beta-two"]);
    }

    #[test]
    fn tags_are_lowercased_sorted_and_deduplicated() {
        let note = parse("---\ntags:\n  - Work\n---\n\nBody #work #Work #ALPHA\n");
        assert_eq!(note.tags, vec!["alpha", "work"]);
    }

    #[test]
    fn the_title_comes_from_the_first_heading_then_the_file_name() {
        assert_eq!(parse("# Real Title\n").title, "Real Title");
        assert_eq!(parse("no heading here\n").title, "Sample");
        // Frontmatter must not be mistaken for the heading.
        assert_eq!(parse("---\ntags:\n  - x\n---\n\n# After\n").title, "After");
    }

    #[test]
    fn tasks_record_their_line_and_checked_state() {
        let note = parse("# T\n\n- [ ] open item\n- [x] done item\n- plain bullet\n");
        assert_eq!(note.tasks.len(), 2);
        assert!(!note.tasks[0].checked);
        assert_eq!(note.tasks[0].text, "open item");
        assert!(note.tasks[1].checked);
        assert_eq!(note.tasks[1].line, 4);
    }

    #[test]
    fn wiki_and_markdown_links_are_collected() {
        let note = parse("# L\n\nSee [[Other Note]] and [Local](Notes/Local.md).\n");
        let targets: Vec<_> = note.links.iter().map(|link| link.target.as_str()).collect();
        assert!(targets.contains(&"Other Note"), "{targets:?}");
        assert!(targets.contains(&"Notes/Local.md"), "{targets:?}");
    }

    #[test]
    fn unicode_content_is_preserved() {
        let note = parse("# Ünïcode ✓\n\nText with #café and 日本語.\n");
        assert_eq!(note.title, "Ünïcode ✓");
        assert!(note.tags.contains(&"café".to_string()), "{:?}", note.tags);
    }

    #[test]
    fn an_unterminated_frontmatter_block_does_not_swallow_the_document() {
        let note = parse("---\ntags:\n  - work\n");
        assert_eq!(note.tags, vec!["work"]);
    }
}
