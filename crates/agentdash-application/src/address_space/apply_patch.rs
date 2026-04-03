use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use agentdash_spi::MountEditCapabilities;
use async_trait::async_trait;

const BEGIN_PATCH_MARKER: &str = "*** Begin Patch";
const END_PATCH_MARKER: &str = "*** End Patch";
const ADD_FILE_MARKER: &str = "*** Add File: ";
const DELETE_FILE_MARKER: &str = "*** Delete File: ";
const UPDATE_FILE_MARKER: &str = "*** Update File: ";
const MOVE_TO_MARKER: &str = "*** Move to: ";
const EOF_MARKER: &str = "*** End of File";
const CHANGE_CONTEXT_MARKER: &str = "@@ ";
const EMPTY_CHANGE_CONTEXT_MARKER: &str = "@@";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedPaths {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ApplyPatchError {
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("patch 路径非法: {0}")]
    InvalidPath(String),
    #[error("patch 能力不足: {0}")]
    Capabilities(String),
    #[error("patch 应用失败: {0}")]
    Apply(String),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum ParseError {
    #[error("invalid patch: {0}")]
    InvalidPatchError(String),
    #[error("invalid hunk at line {line_number}: {message}")]
    InvalidHunkError { message: String, line_number: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchEntry {
    AddFile {
        path: PathBuf,
        contents: String,
    },
    DeleteFile {
        path: PathBuf,
    },
    UpdateFile {
        path: PathBuf,
        move_path: Option<PathBuf>,
        chunks: Vec<UpdateFileChunk>,
    },
}

impl PatchEntry {
    /// Returns the primary file path of this entry.
    pub fn path(&self) -> &Path {
        match self {
            PatchEntry::AddFile { path, .. } => path,
            PatchEntry::DeleteFile { path } => path,
            PatchEntry::UpdateFile { path, .. } => path,
        }
    }

    /// Sets the primary file path (used when stripping mount prefixes).
    pub fn set_path(&mut self, new_path: PathBuf) {
        match self {
            PatchEntry::AddFile { path, .. } => *path = new_path,
            PatchEntry::DeleteFile { path } => *path = new_path,
            PatchEntry::UpdateFile { path, .. } => *path = new_path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFileChunk {
    change_context: Option<String>,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    is_end_of_file: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct RequiredEditCapabilities {
    create: bool,
    delete: bool,
    move_path: bool,
}

#[async_trait]
pub trait ApplyPatchTarget: Send + Sync {
    fn edit_capabilities(&self) -> MountEditCapabilities;

    async fn read_text(&self, path: &str) -> Result<String, ApplyPatchError>;

    async fn write_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError>;

    async fn delete_text(&self, path: &str) -> Result<(), ApplyPatchError>;

    async fn rename_text(&self, from_path: &str, to_path: &str) -> Result<(), ApplyPatchError>;
}

pub fn apply_patch_to_fs(
    workspace_root: &Path,
    patch: &str,
) -> Result<AffectedPaths, ApplyPatchError> {
    let workspace_root = std::fs::canonicalize(workspace_root)
        .map_err(|e| ApplyPatchError::Apply(format!("解析 workspace_root 失败: {e}")))?;
    let entries = parse_patch(patch)?;
    if entries.is_empty() {
        return Err(ApplyPatchError::Apply("没有检测到任何文件改动".to_string()));
    }

    let mut affected = AffectedPaths {
        added: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
    };

    for entry in entries {
        match entry {
            PatchEntry::AddFile { path, contents } => {
                let target = resolve_path_for_write(&workspace_root, &path)?;
                if target.exists() {
                    return Err(ApplyPatchError::Apply(format!(
                        "目标文件已存在: {}",
                        display_relative(&path)
                    )));
                }
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| ApplyPatchError::Apply(format!("创建目录失败: {e}")))?;
                }
                std::fs::write(&target, contents)
                    .map_err(|e| ApplyPatchError::Apply(format!("写入文件失败: {e}")))?;
                affected.added.push(display_relative(&path));
            }
            PatchEntry::DeleteFile { path } => {
                let target = resolve_existing_path(&workspace_root, &path)?;
                std::fs::remove_file(&target)
                    .map_err(|e| ApplyPatchError::Apply(format!("删除文件失败: {e}")))?;
                affected.deleted.push(display_relative(&path));
            }
            PatchEntry::UpdateFile {
                path,
                move_path,
                chunks,
            } => {
                let source = resolve_existing_path(&workspace_root, &path)?;
                let original_contents = std::fs::read_to_string(&source)
                    .map_err(|e| ApplyPatchError::Apply(format!("读取待更新文件失败: {e}")))?;
                let source_label = display_relative(&path);
                let new_contents =
                    derive_new_contents_from_text(&source_label, &original_contents, &chunks)?;

                if let Some(dest_rel) = move_path {
                    let destination = resolve_path_for_write(&workspace_root, &dest_rel)?;
                    if destination != source && destination.exists() {
                        return Err(ApplyPatchError::Apply(format!(
                            "目标文件已存在: {}",
                            display_relative(&dest_rel)
                        )));
                    }
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| ApplyPatchError::Apply(format!("创建目录失败: {e}")))?;
                    }
                    std::fs::write(&destination, new_contents)
                        .map_err(|e| ApplyPatchError::Apply(format!("写入文件失败: {e}")))?;
                    if destination != source {
                        std::fs::remove_file(&source)
                            .map_err(|e| ApplyPatchError::Apply(format!("移除源文件失败: {e}")))?;
                    }
                    affected.modified.push(display_relative(&dest_rel));
                } else {
                    std::fs::write(&source, new_contents)
                        .map_err(|e| ApplyPatchError::Apply(format!("写入文件失败: {e}")))?;
                    affected.modified.push(source_label);
                }
            }
        }
    }

    Ok(affected)
}

pub fn apply_patch_to_inline_files(
    files: &mut BTreeMap<String, String>,
    patch: &str,
) -> Result<AffectedPaths, ApplyPatchError> {
    let entries = parse_patch(patch)?;
    if entries.is_empty() {
        return Err(ApplyPatchError::Apply("没有检测到任何文件改动".to_string()));
    }

    let mut affected = AffectedPaths {
        added: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
    };

    for entry in entries {
        match entry {
            PatchEntry::AddFile { path, contents } => {
                let normalized = normalize_relative_path(&path)?;
                let key = display_relative(&normalized);
                if files.contains_key(&key) {
                    return Err(ApplyPatchError::Apply(format!("目标文件已存在: {key}")));
                }
                files.insert(key.clone(), contents);
                affected.added.push(key);
            }
            PatchEntry::DeleteFile { path } => {
                let normalized = normalize_relative_path(&path)?;
                let key = display_relative(&normalized);
                if files.remove(&key).is_none() {
                    return Err(ApplyPatchError::Apply(format!("目标文件不存在: {key}")));
                }
                affected.deleted.push(key);
            }
            PatchEntry::UpdateFile {
                path,
                move_path,
                chunks,
            } => {
                let normalized = normalize_relative_path(&path)?;
                let key = display_relative(&normalized);
                let original_contents = files
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| ApplyPatchError::Apply(format!("目标文件不存在: {key}")))?;
                let new_contents =
                    derive_new_contents_from_text(&key, &original_contents, &chunks)?;

                if let Some(dest_rel) = move_path {
                    let dest_normalized = normalize_relative_path(&dest_rel)?;
                    let dest_key = display_relative(&dest_normalized);
                    if dest_key != key && files.contains_key(&dest_key) {
                        return Err(ApplyPatchError::Apply(format!(
                            "目标文件已存在: {dest_key}"
                        )));
                    }
                    files.insert(dest_key.clone(), new_contents);
                    if dest_key != key {
                        files.remove(&key);
                    }
                    affected.modified.push(dest_key);
                } else {
                    files.insert(key.clone(), new_contents);
                    affected.modified.push(key);
                }
            }
        }
    }

    Ok(affected)
}

pub async fn apply_patch_to_target<T: ApplyPatchTarget>(
    target: &T,
    patch: &str,
) -> Result<AffectedPaths, ApplyPatchError> {
    let entries = parse_patch(patch)?;
    apply_entries_to_target(target, &entries).await
}

pub async fn apply_entries_to_target<T: ApplyPatchTarget>(
    target: &T,
    entries: &[PatchEntry],
) -> Result<AffectedPaths, ApplyPatchError> {
    if entries.is_empty() {
        return Err(ApplyPatchError::Apply("没有检测到任何文件改动".to_string()));
    }

    ensure_edit_capabilities(
        target.edit_capabilities(),
        required_edit_capabilities(entries),
    )?;

    let mut affected = AffectedPaths {
        added: Vec::new(),
        modified: Vec::new(),
        deleted: Vec::new(),
    };

    for entry in entries {
        match entry {
            PatchEntry::AddFile { path, contents } => {
                let normalized = normalize_relative_path(path)?;
                let path_label = display_relative(&normalized);
                if target.read_text(&path_label).await.is_ok() {
                    return Err(ApplyPatchError::Apply(format!(
                        "目标文件已存在: {path_label}"
                    )));
                }
                target.write_text(&path_label, contents).await?;
                affected.added.push(path_label);
            }
            PatchEntry::DeleteFile { path } => {
                let normalized = normalize_relative_path(path)?;
                let path_label = display_relative(&normalized);
                target.delete_text(&path_label).await?;
                affected.deleted.push(path_label);
            }
            PatchEntry::UpdateFile {
                path,
                move_path,
                chunks,
            } => {
                let normalized = normalize_relative_path(path)?;
                let source_label = display_relative(&normalized);
                let original_contents = target.read_text(&source_label).await?;
                let new_contents =
                    derive_new_contents_from_text(&source_label, &original_contents, chunks)?;

                if let Some(dest_rel) = move_path {
                    let dest_normalized = normalize_relative_path(dest_rel)?;
                    let dest_label = display_relative(&dest_normalized);
                    let edit_caps = target.edit_capabilities();

                    if dest_label != source_label && target.read_text(&dest_label).await.is_ok() {
                        return Err(ApplyPatchError::Apply(format!(
                            "目标文件已存在: {dest_label}"
                        )));
                    }

                    if dest_label == source_label {
                        target.write_text(&source_label, &new_contents).await?;
                    } else if edit_caps.rename {
                        target.rename_text(&source_label, &dest_label).await?;
                        target.write_text(&dest_label, &new_contents).await?;
                    } else {
                        target.write_text(&dest_label, &new_contents).await?;
                        target.delete_text(&source_label).await?;
                    }

                    affected.modified.push(dest_label);
                } else {
                    target.write_text(&source_label, &new_contents).await?;
                    affected.modified.push(source_label);
                }
            }
        }
    }

    Ok(affected)
}

fn required_edit_capabilities(entries: &[PatchEntry]) -> RequiredEditCapabilities {
    let mut required = RequiredEditCapabilities::default();
    for entry in entries {
        match entry {
            PatchEntry::AddFile { .. } => required.create = true,
            PatchEntry::DeleteFile { .. } => required.delete = true,
            PatchEntry::UpdateFile { move_path, .. } => {
                if move_path.is_some() {
                    required.move_path = true;
                }
            }
        }
    }
    required
}

fn ensure_edit_capabilities(
    actual: MountEditCapabilities,
    required: RequiredEditCapabilities,
) -> Result<(), ApplyPatchError> {
    let mut missing = Vec::new();
    if required.create && !actual.create {
        missing.push("create");
    }
    if required.delete && !actual.delete {
        missing.push("delete");
    }
    if required.move_path && !actual.supports_move() {
        missing.push("rename 或 create+delete");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(ApplyPatchError::Capabilities(format!(
            "当前目标缺少以下编辑能力: {}",
            missing.join(", ")
        )))
    }
}

pub fn parse_patch(patch: &str) -> Result<Vec<PatchEntry>, ParseError> {
    let lines: Vec<&str> = patch.trim().lines().collect();
    let lines = check_patch_boundaries(&lines)?;
    let last_line_index = lines.len().saturating_sub(1);
    let mut remaining = &lines[1..last_line_index];
    let mut line_number = 2;
    let mut entries = Vec::new();

    while !remaining.is_empty() {
        let (entry, consumed) = parse_one_entry(remaining, line_number)?;
        entries.push(entry);
        remaining = &remaining[consumed..];
        line_number += consumed;
    }

    Ok(entries)
}

fn check_patch_boundaries<'a>(lines: &'a [&'a str]) -> Result<&'a [&'a str], ParseError> {
    if lines.is_empty() {
        return Err(ParseError::InvalidPatchError(
            "The first line of the patch must be '*** Begin Patch'".to_string(),
        ));
    }

    let first = lines.first().map(|line| line.trim());
    let last = lines.last().map(|line| line.trim());
    match (first, last) {
        (Some(BEGIN_PATCH_MARKER), Some(END_PATCH_MARKER)) => Ok(lines),
        (Some(first), _) if first != BEGIN_PATCH_MARKER => {
            if (first == "<<EOF" || first == "<<'EOF'" || first == "<<\"EOF\"")
                && lines
                    .last()
                    .is_some_and(|line| line.trim_end().ends_with("EOF"))
                && lines.len() >= 4
            {
                let inner = &lines[1..lines.len() - 1];
                let inner_first = inner.first().map(|line| line.trim());
                let inner_last = inner.last().map(|line| line.trim());
                match (inner_first, inner_last) {
                    (Some(BEGIN_PATCH_MARKER), Some(END_PATCH_MARKER)) => Ok(inner),
                    (_, Some(_)) => Err(ParseError::InvalidPatchError(
                        "The first line of the patch must be '*** Begin Patch'".to_string(),
                    )),
                    _ => Err(ParseError::InvalidPatchError(
                        "The last line of the patch must be '*** End Patch'".to_string(),
                    )),
                }
            } else {
                Err(ParseError::InvalidPatchError(
                    "The first line of the patch must be '*** Begin Patch'".to_string(),
                ))
            }
        }
        _ => Err(ParseError::InvalidPatchError(
            "The last line of the patch must be '*** End Patch'".to_string(),
        )),
    }
}

fn parse_one_entry(lines: &[&str], line_number: usize) -> Result<(PatchEntry, usize), ParseError> {
    let first_line = lines[0].trim();
    if let Some(path) = first_line.strip_prefix(ADD_FILE_MARKER) {
        let mut contents = String::new();
        let mut consumed = 1;
        for line in &lines[1..] {
            if let Some(content_line) = line.strip_prefix('+') {
                contents.push_str(content_line);
                contents.push('\n');
                consumed += 1;
            } else {
                break;
            }
        }
        return Ok((
            PatchEntry::AddFile {
                path: PathBuf::from(path),
                contents,
            },
            consumed,
        ));
    }

    if let Some(path) = first_line.strip_prefix(DELETE_FILE_MARKER) {
        return Ok((
            PatchEntry::DeleteFile {
                path: PathBuf::from(path),
            },
            1,
        ));
    }

    if let Some(path) = first_line.strip_prefix(UPDATE_FILE_MARKER) {
        let mut remaining = &lines[1..];
        let mut consumed = 1;
        let move_path = remaining
            .first()
            .and_then(|line| line.trim().strip_prefix(MOVE_TO_MARKER))
            .map(PathBuf::from);

        if move_path.is_some() {
            remaining = &remaining[1..];
            consumed += 1;
        }

        let mut chunks = Vec::new();
        while !remaining.is_empty() {
            if remaining[0].trim().is_empty() {
                remaining = &remaining[1..];
                consumed += 1;
                continue;
            }
            if remaining[0].starts_with("***") {
                break;
            }
            let (chunk, chunk_lines) =
                parse_update_file_chunk(remaining, line_number + consumed, chunks.is_empty())?;
            chunks.push(chunk);
            remaining = &remaining[chunk_lines..];
            consumed += chunk_lines;
        }

        if chunks.is_empty() {
            return Err(ParseError::InvalidHunkError {
                message: format!("Update File entry for path '{path}' has no @@ chunks"),
                line_number,
            });
        }

        return Ok((
            PatchEntry::UpdateFile {
                path: PathBuf::from(path),
                move_path,
                chunks,
            },
            consumed,
        ));
    }

    Err(ParseError::InvalidHunkError {
        message: format!(
            "'{first_line}' is not a valid file entry header. Expected one of: '*** Add File: {{path}}', '*** Delete File: {{path}}', '*** Update File: {{path}}'"
        ),
        line_number,
    })
}

fn parse_update_file_chunk(
    lines: &[&str],
    line_number: usize,
    allow_missing_context: bool,
) -> Result<(UpdateFileChunk, usize), ParseError> {
    if lines.is_empty() {
        return Err(ParseError::InvalidHunkError {
            message: "Update hunk does not contain any lines".to_string(),
            line_number,
        });
    }

    let (change_context, start_index) = if lines[0] == EMPTY_CHANGE_CONTEXT_MARKER {
        (None, 1)
    } else if let Some(context) = lines[0].strip_prefix(CHANGE_CONTEXT_MARKER) {
        (Some(context.to_string()), 1)
    } else if allow_missing_context {
        (None, 0)
    } else {
        return Err(ParseError::InvalidHunkError {
            message: format!(
                "Expected update hunk to start with a @@ context marker, got: '{}'",
                lines[0]
            ),
            line_number,
        });
    };

    if start_index >= lines.len() {
        return Err(ParseError::InvalidHunkError {
            message: "Update hunk does not contain any lines".to_string(),
            line_number: line_number + 1,
        });
    }

    let mut chunk = UpdateFileChunk {
        change_context,
        old_lines: Vec::new(),
        new_lines: Vec::new(),
        is_end_of_file: false,
    };
    let mut consumed = 0;

    for line in &lines[start_index..] {
        if *line == EOF_MARKER {
            if consumed == 0 {
                return Err(ParseError::InvalidHunkError {
                    message: "Update hunk does not contain any lines".to_string(),
                    line_number: line_number + 1,
                });
            }
            chunk.is_end_of_file = true;
            consumed += 1;
            break;
        }

        match line.chars().next() {
            None => {
                chunk.old_lines.push(String::new());
                chunk.new_lines.push(String::new());
            }
            Some(' ') => {
                chunk.old_lines.push(line[1..].to_string());
                chunk.new_lines.push(line[1..].to_string());
            }
            Some('+') => {
                chunk.new_lines.push(line[1..].to_string());
            }
            Some('-') => {
                chunk.old_lines.push(line[1..].to_string());
            }
            _ if consumed == 0 => {
                return Err(ParseError::InvalidHunkError {
                    message: format!(
                        "Unexpected line found in update hunk: '{line}'. Every line should start with ' ' (context line), '+' (added line), or '-' (removed line)"
                    ),
                    line_number: line_number + 1,
                });
            }
            _ => break,
        }
        consumed += 1;
    }

    Ok((chunk, consumed + start_index))
}

fn derive_new_contents_from_text(
    path_label: &str,
    original_contents: &str,
    chunks: &[UpdateFileChunk],
) -> Result<String, ApplyPatchError> {
    let mut original_lines: Vec<String> = original_contents.split('\n').map(String::from).collect();
    if original_lines.last().is_some_and(String::is_empty) {
        original_lines.pop();
    }

    let replacements = compute_replacements(&original_lines, path_label, chunks)?;
    let mut new_lines = apply_replacements(original_lines, &replacements);
    if !new_lines.last().is_some_and(String::is_empty) {
        new_lines.push(String::new());
    }
    Ok(new_lines.join("\n"))
}

fn compute_replacements(
    original_lines: &[String],
    path_label: &str,
    chunks: &[UpdateFileChunk],
) -> Result<Vec<(usize, usize, Vec<String>)>, ApplyPatchError> {
    let mut replacements = Vec::new();
    let mut line_index = 0usize;

    for chunk in chunks {
        if let Some(ctx_line) = &chunk.change_context {
            if let Some(index) = seek_sequence(
                original_lines,
                std::slice::from_ref(ctx_line),
                line_index,
                false,
            ) {
                line_index = index + 1;
            } else {
                return Err(ApplyPatchError::Apply(format!(
                    "无法在 {path_label} 中定位上下文 '{ctx_line}'"
                )));
            }
        }

        if chunk.old_lines.is_empty() {
            let insertion_index = if original_lines.last().is_some_and(String::is_empty) {
                original_lines.len().saturating_sub(1)
            } else {
                original_lines.len()
            };
            replacements.push((insertion_index, 0, chunk.new_lines.clone()));
            continue;
        }

        let mut pattern: &[String] = &chunk.old_lines;
        let mut new_slice: &[String] = &chunk.new_lines;
        let mut found = seek_sequence(original_lines, pattern, line_index, chunk.is_end_of_file);

        if found.is_none() && pattern.last().is_some_and(String::is_empty) {
            pattern = &pattern[..pattern.len() - 1];
            if new_slice.last().is_some_and(String::is_empty) {
                new_slice = &new_slice[..new_slice.len() - 1];
            }
            found = seek_sequence(original_lines, pattern, line_index, chunk.is_end_of_file);
        }

        if let Some(start_idx) = found {
            replacements.push((start_idx, pattern.len(), new_slice.to_vec()));
            line_index = start_idx + pattern.len();
        } else {
            return Err(ApplyPatchError::Apply(format!(
                "无法在 {path_label} 中定位预期旧内容:\n{}",
                chunk.old_lines.join("\n")
            )));
        }
    }

    replacements.sort_by_key(|(index, _, _)| *index);
    Ok(replacements)
}

fn apply_replacements(
    mut lines: Vec<String>,
    replacements: &[(usize, usize, Vec<String>)],
) -> Vec<String> {
    for (start_idx, old_len, new_segment) in replacements.iter().rev() {
        for _ in 0..*old_len {
            if *start_idx < lines.len() {
                lines.remove(*start_idx);
            }
        }
        for (offset, line) in new_segment.iter().enumerate() {
            lines.insert(*start_idx + offset, line.clone());
        }
    }
    lines
}

fn seek_sequence(lines: &[String], pattern: &[String], start: usize, eof: bool) -> Option<usize> {
    if pattern.is_empty() {
        return Some(start);
    }
    if pattern.len() > lines.len() {
        return None;
    }

    let search_start = if eof && lines.len() >= pattern.len() {
        lines.len() - pattern.len()
    } else {
        start
    };

    for idx in search_start..=lines.len().saturating_sub(pattern.len()) {
        if lines[idx..idx + pattern.len()] == *pattern {
            return Some(idx);
        }
    }

    for idx in search_start..=lines.len().saturating_sub(pattern.len()) {
        let matched = pattern
            .iter()
            .enumerate()
            .all(|(offset, pat)| lines[idx + offset].trim_end() == pat.trim_end());
        if matched {
            return Some(idx);
        }
    }

    for idx in search_start..=lines.len().saturating_sub(pattern.len()) {
        let matched = pattern.iter().enumerate().all(|(offset, pat)| {
            normalize_for_match(&lines[idx + offset]) == normalize_for_match(pat)
        });
        if matched {
            return Some(idx);
        }
    }

    None
}

fn normalize_for_match(input: &str) -> String {
    input
        .trim()
        .chars()
        .map(|ch| match ch {
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
            | '\u{2212}' => '-',
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
            '\u{00A0}' | '\u{2002}' | '\u{2003}' | '\u{2004}' | '\u{2005}' | '\u{2006}'
            | '\u{2007}' | '\u{2008}' | '\u{2009}' | '\u{200A}' | '\u{202F}' | '\u{205F}'
            | '\u{3000}' => ' ',
            other => other,
        })
        .collect()
}

fn resolve_existing_path(
    workspace_root: &Path,
    relative: &Path,
) -> Result<PathBuf, ApplyPatchError> {
    let normalized = normalize_relative_path(relative)?;
    let candidate = workspace_root.join(&normalized);
    if !candidate.exists() {
        return Err(ApplyPatchError::Apply(format!(
            "目标文件不存在: {}",
            display_relative(&normalized)
        )));
    }
    let canonical = std::fs::canonicalize(&candidate)
        .map_err(|e| ApplyPatchError::Apply(format!("解析目标路径失败: {e}")))?;
    if !canonical.starts_with(workspace_root) {
        return Err(ApplyPatchError::InvalidPath(display_relative(&normalized)));
    }
    Ok(canonical)
}

fn resolve_path_for_write(
    workspace_root: &Path,
    relative: &Path,
) -> Result<PathBuf, ApplyPatchError> {
    let normalized = normalize_relative_path(relative)?;
    if normalized.as_os_str().is_empty() {
        return Err(ApplyPatchError::InvalidPath(relative.display().to_string()));
    }
    let candidate = workspace_root.join(&normalized);
    let parent = candidate
        .parent()
        .ok_or_else(|| ApplyPatchError::InvalidPath(display_relative(&normalized)))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| ApplyPatchError::Apply(format!("创建父目录失败: {e}")))?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|e| ApplyPatchError::Apply(format!("解析父目录失败: {e}")))?;
    if !canonical_parent.starts_with(workspace_root) {
        return Err(ApplyPatchError::InvalidPath(display_relative(&normalized)));
    }
    Ok(candidate)
}

fn normalize_relative_path(path: &Path) -> Result<PathBuf, ApplyPatchError> {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "." {
        return Ok(PathBuf::new());
    }
    if is_absolute_like(trimmed) {
        return Err(ApplyPatchError::InvalidPath(trimmed.to_string()));
    }

    let mut normalized = PathBuf::new();
    for part in trimmed.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if !normalized.pop() {
                return Err(ApplyPatchError::InvalidPath(trimmed.to_string()));
            }
            continue;
        }
        normalized.push(part);
    }
    Ok(normalized)
}

fn is_absolute_like(raw: &str) -> bool {
    raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.starts_with("//")
        || raw.starts_with("\\\\")
        || raw
            .as_bytes()
            .get(1)
            .zip(raw.as_bytes().get(2))
            .is_some_and(|(second, third)| *second == b':' && (*third == b'\\' || *third == b'/'))
}

fn display_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap_patch(body: &str) -> String {
        format!("*** Begin Patch\n{body}\n*** End Patch")
    }

    #[test]
    fn apply_patch_to_fs_updates_multiple_chunks() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("multi.txt");
        std::fs::write(&file, "foo\nbar\nbaz\nqux\n").expect("write file");

        let patch = wrap_patch(
            r#"*** Update File: multi.txt
@@
 foo
-bar
+BAR
@@
 baz
-qux
+QUX"#,
        );

        let affected = apply_patch_to_fs(temp.path(), &patch).expect("patch should apply");
        assert_eq!(affected.modified, vec!["multi.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(file).expect("read file"),
            "foo\nBAR\nbaz\nQUX\n"
        );
    }

    #[test]
    fn apply_patch_to_fs_supports_add_delete_and_move() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("obsolete.txt"), "old\n").expect("write obsolete");
        std::fs::write(temp.path().join("rename-me.txt"), "line\n").expect("write source");

        let patch = wrap_patch(
            r#"*** Add File: nested/new.txt
+created
*** Delete File: obsolete.txt
*** Update File: rename-me.txt
*** Move to: renamed/after.txt
@@
-line
+line2"#,
        );

        let affected = apply_patch_to_fs(temp.path(), &patch).expect("patch should apply");
        assert_eq!(affected.added, vec!["nested/new.txt".to_string()]);
        assert_eq!(affected.deleted, vec!["obsolete.txt".to_string()]);
        assert_eq!(affected.modified, vec!["renamed/after.txt".to_string()]);
        assert!(!temp.path().join("rename-me.txt").exists());
        assert_eq!(
            std::fs::read_to_string(temp.path().join("renamed/after.txt")).expect("read moved"),
            "line2\n"
        );
    }

    #[test]
    fn apply_patch_to_fs_rejects_path_escape() {
        let temp = tempfile::tempdir().expect("tempdir");
        let patch = wrap_patch(
            r#"*** Add File: ../escape.txt
+nope"#,
        );

        let error = apply_patch_to_fs(temp.path(), &patch).expect_err("escape should be rejected");
        assert!(matches!(error, ApplyPatchError::InvalidPath(_)));
    }

    #[test]
    fn apply_patch_to_fs_accepts_heredoc_wrapped_patch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let patch = r#"<<'EOF'
*** Begin Patch
*** Add File: hello.txt
+hi
*** End Patch
EOF"#;

        let affected = apply_patch_to_fs(temp.path(), patch).expect("patch should apply");
        assert_eq!(affected.added, vec!["hello.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("hello.txt")).expect("read file"),
            "hi\n"
        );
    }

    #[test]
    fn apply_patch_to_inline_files_supports_move_and_delete() {
        let mut files = BTreeMap::from([
            ("a.txt".to_string(), "one\n".to_string()),
            ("dir/b.txt".to_string(), "two\n".to_string()),
        ]);

        let patch = wrap_patch(
            r#"*** Update File: a.txt
*** Move to: moved/a2.txt
@@
-one
+ONE
*** Delete File: dir/b.txt
*** Add File: new.txt
+fresh"#,
        );

        let affected = apply_patch_to_inline_files(&mut files, &patch).expect("patch should apply");
        assert_eq!(affected.modified, vec!["moved/a2.txt".to_string()]);
        assert_eq!(affected.deleted, vec!["dir/b.txt".to_string()]);
        assert_eq!(affected.added, vec!["new.txt".to_string()]);
        assert_eq!(files.get("moved/a2.txt").map(String::as_str), Some("ONE\n"));
        assert!(!files.contains_key("a.txt"));
        assert!(!files.contains_key("dir/b.txt"));
        assert_eq!(files.get("new.txt").map(String::as_str), Some("fresh\n"));
    }

    #[derive(Default)]
    struct MemoryTarget {
        files: tokio::sync::RwLock<BTreeMap<String, String>>,
        capabilities: MountEditCapabilities,
    }

    #[async_trait]
    impl ApplyPatchTarget for MemoryTarget {
        fn edit_capabilities(&self) -> MountEditCapabilities {
            self.capabilities
        }

        async fn read_text(&self, path: &str) -> Result<String, ApplyPatchError> {
            self.files
                .read()
                .await
                .get(path)
                .cloned()
                .ok_or_else(|| ApplyPatchError::Apply(format!("目标文件不存在: {path}")))
        }

        async fn write_text(&self, path: &str, content: &str) -> Result<(), ApplyPatchError> {
            self.files
                .write()
                .await
                .insert(path.to_string(), content.to_string());
            Ok(())
        }

        async fn delete_text(&self, path: &str) -> Result<(), ApplyPatchError> {
            self.files
                .write()
                .await
                .remove(path)
                .ok_or_else(|| ApplyPatchError::Apply(format!("目标文件不存在: {path}")))?;
            Ok(())
        }

        async fn rename_text(&self, from_path: &str, to_path: &str) -> Result<(), ApplyPatchError> {
            let mut files = self.files.write().await;
            let content = files
                .remove(from_path)
                .ok_or_else(|| ApplyPatchError::Apply(format!("目标文件不存在: {from_path}")))?;
            files.insert(to_path.to_string(), content);
            Ok(())
        }
    }

    #[tokio::test]
    async fn apply_patch_to_target_reports_missing_capabilities() {
        let target = MemoryTarget {
            files: Default::default(),
            capabilities: MountEditCapabilities {
                create: false,
                delete: false,
                rename: false,
            },
        };
        let patch = wrap_patch(
            r#"*** Add File: hello.txt
+hi"#,
        );

        let error = apply_patch_to_target(&target, &patch)
            .await
            .expect_err("capability check should fail");
        assert!(matches!(error, ApplyPatchError::Capabilities(_)));
    }

    #[tokio::test]
    async fn apply_patch_to_target_supports_move_via_create_delete_fallback() {
        let target = MemoryTarget {
            files: tokio::sync::RwLock::new(BTreeMap::from([(
                "src.txt".to_string(),
                "hello\n".to_string(),
            )])),
            capabilities: MountEditCapabilities {
                create: true,
                delete: true,
                rename: false,
            },
        };
        let patch = wrap_patch(
            r#"*** Update File: src.txt
*** Move to: dst.txt
@@
-hello
+HELLO"#,
        );

        let affected = apply_patch_to_target(&target, &patch)
            .await
            .expect("patch should apply");
        assert_eq!(affected.modified, vec!["dst.txt".to_string()]);
        let files = target.files.read().await;
        assert_eq!(files.get("dst.txt").map(String::as_str), Some("HELLO\n"));
        assert!(!files.contains_key("src.txt"));
    }
}
