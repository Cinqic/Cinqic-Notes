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
            continue;
        }
        if in_frontmatter {
            if let Some(value) = trimmed.strip_prefix("type:") {
                project = value.trim().eq_ignore_ascii_case("project");
            }
            if trimmed == "tags:" {
                continue;
            }
            if let Some(value) = trimmed.strip_prefix('-') {
                let value = value.trim().trim_matches(['"', '\'']);
                if !value.is_empty() && !value.contains(' ') {
                    tags.push(value.to_ascii_lowercase());
                }
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
