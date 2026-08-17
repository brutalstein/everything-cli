//! Compact, deterministic edit protocol for coding workers.
//!
//! The provider emits only changed text. Every mutation is bound to exact base
//! evidence and is fully preflighted before the owned worktree is changed.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const DEFAULT_MAX_OPERATIONS: usize = 32;
pub const DEFAULT_MAX_OPERATION_BYTES: usize = 1024 * 1024;
pub const DEFAULT_MAX_PLAN_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditLimits {
    pub max_operations: usize,
    pub max_operation_bytes: usize,
    pub max_plan_bytes: usize,
}

impl Default for EditLimits {
    fn default() -> Self {
        Self {
            max_operations: DEFAULT_MAX_OPERATIONS,
            max_operation_bytes: DEFAULT_MAX_OPERATION_BYTES,
            max_plan_bytes: DEFAULT_MAX_PLAN_BYTES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactEditPlan {
    pub summary: String,
    pub operations: Vec<EditOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditOperation {
    ReplaceRange {
        path: String,
        base_file_sha256: String,
        start_line: u32,
        end_line: u32,
        expected_segment_sha256: String,
        replacement: Vec<u8>,
    },
    CreateFile {
        path: String,
        content: Vec<u8>,
    },
    DeleteFile {
        path: String,
        base_file_sha256: String,
    },
}

impl EditOperation {
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::ReplaceRange { path, .. }
            | Self::CreateFile { path, .. }
            | Self::DeleteFile { path, .. } => path,
        }
    }

    #[must_use]
    pub fn changed_bytes(&self) -> usize {
        match self {
            Self::ReplaceRange { replacement, .. } => replacement.len(),
            Self::CreateFile { content, .. } => content.len(),
            Self::DeleteFile { .. } => 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditResult {
    pub path: String,
    pub previous_sha256: Option<String>,
    pub resulting_sha256: Option<String>,
    pub changed_output_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditReceipt {
    pub results: Vec<EditResult>,
    pub operation_count: usize,
    pub changed_output_bytes: usize,
}

#[must_use]
pub fn edit_plan_schema(max_operations: usize) -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["summary","operations"],
        "properties":{
            "summary":{"type":"string"},
            "operations":{
                "type":"array",
                "minItems":1,
                "maxItems":max_operations,
                "items":{
                    "oneOf":[
                        {
                            "type":"object",
                            "additionalProperties":false,
                            "required":["op","path","base_file_sha256","start_line","end_line","expected_segment_sha256","replacement"],
                            "properties":{
                                "op":{"const":"replace_range"},
                                "path":{"type":"string"},
                                "base_file_sha256":{"type":"string"},
                                "start_line":{"type":"integer","minimum":1},
                                "end_line":{"type":"integer","minimum":1},
                                "expected_segment_sha256":{"type":"string"},
                                "replacement":{"type":"string"}
                            }
                        },
                        {
                            "type":"object",
                            "additionalProperties":false,
                            "required":["op","path","content"],
                            "properties":{
                                "op":{"const":"create_file"},
                                "path":{"type":"string"},
                                "content":{"type":"string"}
                            }
                        },
                        {
                            "type":"object",
                            "additionalProperties":false,
                            "required":["op","path","base_file_sha256"],
                            "properties":{
                                "op":{"const":"delete_file"},
                                "path":{"type":"string"},
                                "base_file_sha256":{"type":"string"}
                            }
                        }
                    ]
                }
            }
        }
    })
}

pub fn parse_edit_plan(text: &str, limits: EditLimits) -> Result<CompactEditPlan, EditAbiError> {
    validate_limits(limits)?;
    if text.len() > limits.max_plan_bytes {
        return Err(EditAbiError::PlanTooLarge(text.len()));
    }
    let value: Value = serde_json::from_str(text).map_err(EditAbiError::Json)?;
    let object = value
        .as_object()
        .ok_or_else(|| EditAbiError::InvalidPlan("plan must be an object".to_owned()))?;
    require_keys(object.keys().map(String::as_str), &["operations", "summary"])?;
    let summary = object
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| EditAbiError::InvalidPlan("summary must be a string".to_owned()))?
        .to_owned();
    let raw_operations = object
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(|| EditAbiError::InvalidPlan("operations must be an array".to_owned()))?;
    if raw_operations.is_empty() || raw_operations.len() > limits.max_operations {
        return Err(EditAbiError::InvalidPlan(format!(
            "operation count must be between 1 and {}",
            limits.max_operations
        )));
    }

    let mut operations = Vec::with_capacity(raw_operations.len());
    let mut changed_bytes = 0_usize;
    for raw in raw_operations {
        let object = raw
            .as_object()
            .ok_or_else(|| EditAbiError::InvalidPlan("operation must be an object".to_owned()))?;
        let op = object
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| EditAbiError::InvalidPlan("operation op must be a string".to_owned()))?;
        let parsed = match op {
            "replace_range" => {
                require_keys(
                    object.keys().map(String::as_str),
                    &[
                        "base_file_sha256",
                        "end_line",
                        "expected_segment_sha256",
                        "op",
                        "path",
                        "replacement",
                        "start_line",
                    ],
                )?;
                let path = required_string(object, "path")?;
                validate_relative_path(&path)?;
                let base_file_sha256 = required_hash(object, "base_file_sha256")?;
                let expected_segment_sha256 = required_hash(object, "expected_segment_sha256")?;
                let start_line = required_u32(object, "start_line")?;
                let end_line = required_u32(object, "end_line")?;
                if start_line == 0 || end_line < start_line {
                    return Err(EditAbiError::InvalidRange {
                        path,
                        start_line,
                        end_line,
                    });
                }
                let replacement = required_string(object, "replacement")?.into_bytes();
                if replacement.len() > limits.max_operation_bytes {
                    return Err(EditAbiError::OperationTooLarge {
                        path,
                        bytes: replacement.len(),
                    });
                }
                EditOperation::ReplaceRange {
                    path,
                    base_file_sha256,
                    start_line,
                    end_line,
                    expected_segment_sha256,
                    replacement,
                }
            }
            "create_file" => {
                require_keys(
                    object.keys().map(String::as_str),
                    &["content", "op", "path"],
                )?;
                let path = required_string(object, "path")?;
                validate_relative_path(&path)?;
                let content = required_string(object, "content")?.into_bytes();
                if content.len() > limits.max_operation_bytes {
                    return Err(EditAbiError::OperationTooLarge {
                        path,
                        bytes: content.len(),
                    });
                }
                EditOperation::CreateFile { path, content }
            }
            "delete_file" => {
                require_keys(
                    object.keys().map(String::as_str),
                    &["base_file_sha256", "op", "path"],
                )?;
                let path = required_string(object, "path")?;
                validate_relative_path(&path)?;
                EditOperation::DeleteFile {
                    path,
                    base_file_sha256: required_hash(object, "base_file_sha256")?,
                }
            }
            other => return Err(EditAbiError::UnknownOperation(other.to_owned())),
        };
        changed_bytes = changed_bytes
            .checked_add(parsed.changed_bytes())
            .ok_or(EditAbiError::ArithmeticOverflow)?;
        if changed_bytes > limits.max_plan_bytes {
            return Err(EditAbiError::PlanTooLarge(changed_bytes));
        }
        operations.push(parsed);
    }

    validate_operation_conflicts(&operations)?;
    Ok(CompactEditPlan {
        summary,
        operations,
    })
}

pub fn apply_edit_plan(
    worktree_root: &Path,
    plan: &CompactEditPlan,
    limits: EditLimits,
) -> Result<EditReceipt, EditAbiError> {
    validate_limits(limits)?;
    if plan.operations.is_empty() || plan.operations.len() > limits.max_operations {
        return Err(EditAbiError::InvalidPlan(
            "operation count violates configured limits".to_owned(),
        ));
    }
    validate_operation_conflicts(&plan.operations)?;
    let canonical_root = worktree_root.canonicalize().map_err(EditAbiError::Io)?;

    let mut by_path: BTreeMap<String, Vec<&EditOperation>> = BTreeMap::new();
    let mut changed_output_bytes = 0_usize;
    for operation in &plan.operations {
        validate_relative_path(operation.path())?;
        if operation.changed_bytes() > limits.max_operation_bytes {
            return Err(EditAbiError::OperationTooLarge {
                path: operation.path().to_owned(),
                bytes: operation.changed_bytes(),
            });
        }
        changed_output_bytes = changed_output_bytes
            .checked_add(operation.changed_bytes())
            .ok_or(EditAbiError::ArithmeticOverflow)?;
        if changed_output_bytes > limits.max_plan_bytes {
            return Err(EditAbiError::PlanTooLarge(changed_output_bytes));
        }
        by_path
            .entry(operation.path().to_owned())
            .or_default()
            .push(operation);
    }

    let mut prepared = Vec::with_capacity(by_path.len());
    for (path, operations) in by_path {
        prepared.push(preflight_path(&canonical_root, path, operations)?);
    }

    let mut mutated = Vec::<PreparedPath>::new();
    for item in prepared {
        let result = apply_prepared(&item);
        if let Err(error) = result {
            if let Err(rollback_error) = rollback(&mutated) {
                return Err(EditAbiError::RollbackFailed {
                    mutation_error: error.to_string(),
                    rollback_error: rollback_error.to_string(),
                });
            }
            return Err(error);
        }
        mutated.push(item);
    }

    let mut results = Vec::with_capacity(mutated.len());
    for item in &mutated {
        results.push(EditResult {
            path: item.relative_path.clone(),
            previous_sha256: item.original.as_ref().map(|bytes| sha256(bytes)),
            resulting_sha256: item.resulting.as_ref().map(|bytes| sha256(bytes)),
            changed_output_bytes: item.changed_output_bytes,
        });
    }
    Ok(EditReceipt {
        results,
        operation_count: plan.operations.len(),
        changed_output_bytes,
    })
}

#[derive(Clone, Debug)]
struct PreparedPath {
    relative_path: String,
    target: PathBuf,
    original: Option<Vec<u8>>,
    resulting: Option<Vec<u8>>,
    changed_output_bytes: usize,
}

fn preflight_path(
    canonical_root: &Path,
    relative_path: String,
    operations: Vec<&EditOperation>,
) -> Result<PreparedPath, EditAbiError> {
    let target = safe_target(canonical_root, &relative_path)?;
    let metadata = fs::symlink_metadata(&target);
    let original = match metadata {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(EditAbiError::SymlinkTarget(relative_path));
            }
            if !metadata.file_type().is_file() {
                return Err(EditAbiError::NotRegularFile(relative_path));
            }
            Some(fs::read(&target).map_err(EditAbiError::Io)?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(EditAbiError::Io(error)),
    };

    if operations.len() == 1 {
        match operations[0] {
            EditOperation::CreateFile { content, .. } => {
                if original.is_some() {
                    return Err(EditAbiError::CreateTargetExists(relative_path));
                }
                return Ok(PreparedPath {
                    relative_path,
                    target,
                    original,
                    resulting: Some(content.clone()),
                    changed_output_bytes: content.len(),
                });
            }
            EditOperation::DeleteFile {
                base_file_sha256, ..
            } => {
                let bytes = original
                    .as_ref()
                    .ok_or_else(|| EditAbiError::MissingBase(relative_path.clone()))?;
                require_base_hash(&relative_path, bytes, base_file_sha256)?;
                return Ok(PreparedPath {
                    relative_path,
                    target,
                    original,
                    resulting: None,
                    changed_output_bytes: 0,
                });
            }
            EditOperation::ReplaceRange { .. } => {}
        }
    }

    let bytes = original
        .as_ref()
        .ok_or_else(|| EditAbiError::MissingBase(relative_path.clone()))?;
    let mut replacements = Vec::<PreparedReplacement>::new();
    let mut changed_output_bytes = 0_usize;
    let mut common_base: Option<&str> = None;
    for operation in operations {
        let EditOperation::ReplaceRange {
            base_file_sha256,
            start_line,
            end_line,
            expected_segment_sha256,
            replacement,
            ..
        } = operation
        else {
            return Err(EditAbiError::ConflictingOperations(relative_path));
        };
        if let Some(expected) = common_base {
            if expected != base_file_sha256 {
                return Err(EditAbiError::ConflictingBaseHashes(relative_path));
            }
        } else {
            common_base = Some(base_file_sha256);
        }
        require_base_hash(&relative_path, bytes, base_file_sha256)?;
        let (start, end) = line_byte_range(bytes, *start_line, *end_line).ok_or_else(|| {
            EditAbiError::InvalidRange {
                path: relative_path.clone(),
                start_line: *start_line,
                end_line: *end_line,
            }
        })?;
        let actual_segment = sha256(&bytes[start..end]);
        if actual_segment != *expected_segment_sha256 {
            return Err(EditAbiError::StaleRange {
                path: relative_path.clone(),
                start_line: *start_line,
                end_line: *end_line,
                expected: expected_segment_sha256.clone(),
                actual: actual_segment,
            });
        }
        changed_output_bytes = changed_output_bytes
            .checked_add(replacement.len())
            .ok_or(EditAbiError::ArithmeticOverflow)?;
        replacements.push(PreparedReplacement {
            start_line: *start_line,
            end_line: *end_line,
            start,
            end,
            replacement: replacement.clone(),
        });
    }
    replacements.sort_by_key(|replacement| (replacement.start_line, replacement.end_line));
    for pair in replacements.windows(2) {
        if pair[1].start_line <= pair[0].end_line {
            return Err(EditAbiError::OverlappingRanges {
                path: relative_path,
                first: (pair[0].start_line, pair[0].end_line),
                second: (pair[1].start_line, pair[1].end_line),
            });
        }
    }

    let mut resulting = bytes.clone();
    for replacement in replacements.iter().rev() {
        resulting.splice(
            replacement.start..replacement.end,
            replacement.replacement.iter().copied(),
        );
    }
    Ok(PreparedPath {
        relative_path,
        target,
        original,
        resulting: Some(resulting),
        changed_output_bytes,
    })
}

#[derive(Clone, Debug)]
struct PreparedReplacement {
    start_line: u32,
    end_line: u32,
    start: usize,
    end: usize,
    replacement: Vec<u8>,
}

fn apply_prepared(item: &PreparedPath) -> Result<(), EditAbiError> {
    match &item.resulting {
        Some(bytes) => fs::write(&item.target, bytes).map_err(EditAbiError::Io),
        None => fs::remove_file(&item.target).map_err(EditAbiError::Io),
    }
}

fn rollback(mutated: &[PreparedPath]) -> Result<(), EditAbiError> {
    for item in mutated.iter().rev() {
        match &item.original {
            Some(bytes) => fs::write(&item.target, bytes).map_err(EditAbiError::Io)?,
            None => match fs::remove_file(&item.target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(EditAbiError::Io(error)),
            },
        }
    }
    Ok(())
}

fn safe_target(canonical_root: &Path, relative_path: &str) -> Result<PathBuf, EditAbiError> {
    validate_relative_path(relative_path)?;
    let target = canonical_root.join(relative_path);
    let parent = target
        .parent()
        .ok_or_else(|| EditAbiError::InvalidPath(relative_path.to_owned()))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| EditAbiError::ParentUnavailable(relative_path.to_owned()))?;
    if !canonical_parent.starts_with(canonical_root) {
        return Err(EditAbiError::PathEscape(relative_path.to_owned()));
    }
    Ok(target)
}

pub fn validate_relative_path(value: &str) -> Result<(), EditAbiError> {
    if value.trim().is_empty() {
        return Err(EditAbiError::InvalidPath(value.to_owned()));
    }
    if value.contains('\\') || value.contains(':') || value.contains('\0') {
        return Err(EditAbiError::InvalidPath(value.to_owned()));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(EditAbiError::InvalidPath(value.to_owned()));
    }
    for segment in value.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(EditAbiError::InvalidPath(value.to_owned()));
        }
        if segment.eq_ignore_ascii_case(".git") || segment.eq_ignore_ascii_case(".aer") {
            return Err(EditAbiError::ProtectedPath(value.to_owned()));
        }
        if segment.chars().any(char::is_control) {
            return Err(EditAbiError::InvalidPath(value.to_owned()));
        }
    }
    Ok(())
}

fn validate_operation_conflicts(operations: &[EditOperation]) -> Result<(), EditAbiError> {
    let mut kinds: BTreeMap<&str, BTreeSet<&'static str>> = BTreeMap::new();
    for operation in operations {
        let kind = match operation {
            EditOperation::ReplaceRange { .. } => "replace_range",
            EditOperation::CreateFile { .. } => "create_file",
            EditOperation::DeleteFile { .. } => "delete_file",
        };
        kinds.entry(operation.path()).or_default().insert(kind);
    }
    for (path, path_kinds) in kinds {
        if path_kinds.len() > 1
            || (path_kinds.contains("create_file")
                && operations.iter().filter(|op| op.path() == path).count() > 1)
            || (path_kinds.contains("delete_file")
                && operations.iter().filter(|op| op.path() == path).count() > 1)
        {
            return Err(EditAbiError::ConflictingOperations(path.to_owned()));
        }
    }
    Ok(())
}

fn validate_limits(limits: EditLimits) -> Result<(), EditAbiError> {
    if limits.max_operations == 0 || limits.max_operation_bytes == 0 || limits.max_plan_bytes == 0 {
        return Err(EditAbiError::InvalidLimits);
    }
    Ok(())
}

fn line_byte_range(bytes: &[u8], start_line: u32, end_line: u32) -> Option<(usize, usize)> {
    if start_line == 0 || end_line < start_line {
        return None;
    }
    let mut line = 1_u32;
    let mut start = (start_line == 1).then_some(0_usize);
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            if line == end_line {
                return start.map(|start| (start, index + 1));
            }
            line = line.checked_add(1)?;
            if line == start_line {
                start = Some(index + 1);
            }
        }
    }
    if line == end_line {
        return start.map(|start| (start, bytes.len()));
    }
    None
}

fn require_base_hash(path: &str, bytes: &[u8], expected: &str) -> Result<(), EditAbiError> {
    let actual = sha256(bytes);
    if actual != expected {
        return Err(EditAbiError::StaleBase {
            path: path.to_owned(),
            expected: expected.to_owned(),
            actual,
        });
    }
    Ok(())
}

fn required_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, EditAbiError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| EditAbiError::InvalidPlan(format!("{field} must be a string")))
}

fn required_hash(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, EditAbiError> {
    let hash = required_string(object, field)?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(EditAbiError::InvalidPlan(format!(
            "{field} must be a lowercase 64-character SHA-256"
        )));
    }
    Ok(hash)
}

fn required_u32(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<u32, EditAbiError> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| EditAbiError::InvalidPlan(format!("{field} must be a u32 integer")))
}

fn require_keys<'a>(
    actual: impl Iterator<Item = &'a str>,
    expected: &[&str],
) -> Result<(), EditAbiError> {
    let actual = actual.collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(EditAbiError::InvalidPlan(
            "object contains missing or unknown fields".to_owned(),
        ));
    }
    Ok(())
}

#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[derive(Debug)]
pub enum EditAbiError {
    InvalidLimits,
    PlanTooLarge(usize),
    OperationTooLarge { path: String, bytes: usize },
    InvalidPlan(String),
    UnknownOperation(String),
    InvalidPath(String),
    ProtectedPath(String),
    ParentUnavailable(String),
    PathEscape(String),
    SymlinkTarget(String),
    NotRegularFile(String),
    MissingBase(String),
    CreateTargetExists(String),
    StaleBase { path: String, expected: String, actual: String },
    StaleRange {
        path: String,
        start_line: u32,
        end_line: u32,
        expected: String,
        actual: String,
    },
    InvalidRange { path: String, start_line: u32, end_line: u32 },
    OverlappingRanges {
        path: String,
        first: (u32, u32),
        second: (u32, u32),
    },
    ConflictingOperations(String),
    ConflictingBaseHashes(String),
    ArithmeticOverflow,
    RollbackFailed { mutation_error: String, rollback_error: String },
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for EditAbiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => write!(formatter, "invalid compact edit limits"),
            Self::PlanTooLarge(bytes) => write!(formatter, "compact edit plan exceeds byte limit: {bytes}"),
            Self::OperationTooLarge { path, bytes } => write!(formatter, "compact edit operation exceeds byte limit: {path} ({bytes})"),
            Self::InvalidPlan(message) => write!(formatter, "invalid compact edit plan: {message}"),
            Self::UnknownOperation(op) => write!(formatter, "unknown compact edit operation: {op}"),
            Self::InvalidPath(path) => write!(formatter, "invalid compact edit path: {path}"),
            Self::ProtectedPath(path) => write!(formatter, "compact edit targets protected control-plane path: {path}"),
            Self::ParentUnavailable(path) => write!(formatter, "compact edit parent must exist inside worktree: {path}"),
            Self::PathEscape(path) => write!(formatter, "compact edit escapes owned worktree: {path}"),
            Self::SymlinkTarget(path) => write!(formatter, "compact edit refuses symlink target: {path}"),
            Self::NotRegularFile(path) => write!(formatter, "compact edit target is not a regular file: {path}"),
            Self::MissingBase(path) => write!(formatter, "compact edit base file is missing: {path}"),
            Self::CreateTargetExists(path) => write!(formatter, "compact create target already exists: {path}"),
            Self::StaleBase { path, expected, actual } => write!(formatter, "stale compact edit base for {path}: expected {expected}, actual {actual}"),
            Self::StaleRange { path, start_line, end_line, expected, actual } => write!(formatter, "stale compact edit range for {path}:{start_line}-{end_line}: expected {expected}, actual {actual}"),
            Self::InvalidRange { path, start_line, end_line } => write!(formatter, "invalid compact edit range for {path}:{start_line}-{end_line}"),
            Self::OverlappingRanges { path, first, second } => write!(formatter, "overlapping compact edit ranges for {path}: {}-{} and {}-{}", first.0, first.1, second.0, second.1),
            Self::ConflictingOperations(path) => write!(formatter, "conflicting compact edit operations for {path}"),
            Self::ConflictingBaseHashes(path) => write!(formatter, "conflicting compact edit base identities for {path}"),
            Self::ArithmeticOverflow => write!(formatter, "compact edit arithmetic overflow"),
            Self::RollbackFailed { mutation_error, rollback_error } => write!(formatter, "compact edit failed ({mutation_error}) and rollback failed ({rollback_error})"),
            Self::Io(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl Error for EditAbiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let root = std::env::temp_dir().join(format!("aer-edit-abi-{label}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("temp root");
        root
    }

    fn replace(path: &str, base: &[u8], start: u32, end: u32, segment: &[u8], replacement: &str) -> EditOperation {
        EditOperation::ReplaceRange {
            path: path.to_owned(),
            base_file_sha256: sha256(base),
            start_line: start,
            end_line: end,
            expected_segment_sha256: sha256(segment),
            replacement: replacement.as_bytes().to_vec(),
        }
    }

    #[test]
    fn one_line_edit_applies_only_replacement_payload() {
        let root = temp_root("sparse");
        let base = b"one\ntwo\nthree\nfour\n";
        fs::write(root.join("src/value.txt"), base).expect("base");
        let plan = CompactEditPlan {
            summary: "one line".to_owned(),
            operations: vec![replace("src/value.txt", base, 2, 2, b"two\n", "TWO\n")],
        };
        let receipt = apply_edit_plan(&root, &plan, EditLimits::default()).expect("apply");
        assert_eq!(fs::read(root.join("src/value.txt")).expect("read"), b"one\nTWO\nthree\nfour\n");
        assert_eq!(receipt.changed_output_bytes, 4);
        assert!(receipt.changed_output_bytes < base.len());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn multiple_non_overlapping_ranges_apply_in_original_coordinates() {
        let root = temp_root("multiple");
        let base = b"a\nb\nc\nd\n";
        fs::write(root.join("src/value.txt"), base).expect("base");
        let plan = CompactEditPlan {
            summary: "two ranges".to_owned(),
            operations: vec![
                replace("src/value.txt", base, 1, 1, b"a\n", "alpha\n"),
                replace("src/value.txt", base, 4, 4, b"d\n", "delta\n"),
            ],
        };
        apply_edit_plan(&root, &plan, EditLimits::default()).expect("apply");
        assert_eq!(fs::read(root.join("src/value.txt")).expect("read"), b"alpha\nb\nc\ndelta\n");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn overlaps_and_stale_evidence_fail_closed_before_mutation() {
        let root = temp_root("stale");
        let base = b"a\nb\nc\n";
        fs::write(root.join("src/value.txt"), base).expect("base");
        let overlap = CompactEditPlan {
            summary: "overlap".to_owned(),
            operations: vec![
                replace("src/value.txt", base, 1, 2, b"a\nb\n", "x\n"),
                replace("src/value.txt", base, 2, 3, b"b\nc\n", "y\n"),
            ],
        };
        assert!(matches!(apply_edit_plan(&root, &overlap, EditLimits::default()), Err(EditAbiError::OverlappingRanges { .. })));
        let wrong_base = CompactEditPlan {
            summary: "stale".to_owned(),
            operations: vec![EditOperation::ReplaceRange {
                path: "src/value.txt".to_owned(),
                base_file_sha256: sha256(b"wrong"),
                start_line: 1,
                end_line: 1,
                expected_segment_sha256: sha256(b"a\n"),
                replacement: b"x\n".to_vec(),
            }],
        };
        assert!(matches!(apply_edit_plan(&root, &wrong_base, EditLimits::default()), Err(EditAbiError::StaleBase { .. })));
        let wrong_range = CompactEditPlan {
            summary: "stale range".to_owned(),
            operations: vec![replace("src/value.txt", base, 1, 1, b"not-a\n", "x\n")],
        };
        assert!(matches!(apply_edit_plan(&root, &wrong_range, EditLimits::default()), Err(EditAbiError::StaleRange { .. })));
        assert_eq!(fs::read(root.join("src/value.txt")).expect("read"), base);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn create_delete_and_result_hashes_are_exact() {
        let root = temp_root("lifecycle");
        let old = b"remove me\n";
        fs::write(root.join("src/old.txt"), old).expect("old");
        let plan = CompactEditPlan {
            summary: "lifecycle".to_owned(),
            operations: vec![
                EditOperation::CreateFile { path: "src/new.txt".to_owned(), content: b"new\n".to_vec() },
                EditOperation::DeleteFile { path: "src/old.txt".to_owned(), base_file_sha256: sha256(old) },
            ],
        };
        let receipt = apply_edit_plan(&root, &plan, EditLimits::default()).expect("apply");
        assert_eq!(fs::read(root.join("src/new.txt")).expect("new"), b"new\n");
        assert!(!root.join("src/old.txt").exists());
        let new_result = receipt.results.iter().find(|item| item.path == "src/new.txt").expect("result");
        assert_eq!(new_result.resulting_sha256.as_deref(), Some(sha256(b"new\n").as_str()));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn parser_rejects_traversal_and_protected_paths() {
        for path in ["../escape", ".git/config", "nested/.AeR/state", "src\\x", "C:/x"] {
            let value = json!({"summary":"bad","operations":[{"op":"create_file","path":path,"content":"x"}]}).to_string();
            assert!(parse_edit_plan(&value, EditLimits::default()).is_err(), "accepted {path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_target_fails_closed() {
        use std::os::unix::fs::symlink;
        let root = temp_root("symlink");
        fs::write(root.join("outside.txt"), b"outside\n").expect("outside");
        symlink(root.join("outside.txt"), root.join("src/link.txt")).expect("symlink");
        let plan = CompactEditPlan {
            summary: "bad".to_owned(),
            operations: vec![EditOperation::DeleteFile { path: "src/link.txt".to_owned(), base_file_sha256: sha256(b"outside\n") }],
        };
        assert!(matches!(apply_edit_plan(&root, &plan, EditLimits::default()), Err(EditAbiError::SymlinkTarget(_))));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn identical_base_and_plan_produce_identical_receipts() {
        let root_a = temp_root("replay-a");
        let root_b = temp_root("replay-b");
        let base = b"before\n";
        fs::write(root_a.join("src/value.txt"), base).expect("a");
        fs::write(root_b.join("src/value.txt"), base).expect("b");
        let plan = CompactEditPlan {
            summary: "deterministic".to_owned(),
            operations: vec![replace("src/value.txt", base, 1, 1, base, "after\n")],
        };
        let a = apply_edit_plan(&root_a, &plan, EditLimits::default()).expect("a apply");
        let b = apply_edit_plan(&root_b, &plan, EditLimits::default()).expect("b apply");
        assert_eq!(a, b);
        fs::remove_dir_all(root_a).expect("cleanup a");
        fs::remove_dir_all(root_b).expect("cleanup b");
    }
}
