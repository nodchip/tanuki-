use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROFILE_SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ArchiveKind {
    Zip,
    SevenZip,
    TarXz,
    Lzh,
    Csa,
    Kif,
}

impl ArchiveKind {
    pub fn from_relative_path(path: &str) -> Result<Self, ProfileError> {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".tar.xz") {
            return Ok(Self::TarXz);
        }
        match Path::new(&lower)
            .extension()
            .and_then(|value| value.to_str())
        {
            Some("zip") => Ok(Self::Zip),
            Some("7z") => Ok(Self::SevenZip),
            Some("lzh") | Some("lha") => Ok(Self::Lzh),
            Some("csa") => Ok(Self::Csa),
            Some("kif") | Some("kifu") => Ok(Self::Kif),
            _ => Err(ProfileError::UnsupportedSource(path.to_owned())),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestSource {
    pub site: String,
    pub event: String,
    pub year: i32,
    pub retrieved_at: i64,
    pub url: String,
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    pub manifest_version: u32,
    pub created_at_utc: String,
    pub sources: Vec<ManifestSource>,
}

impl CorpusManifest {
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let text = read_text(path)?;
        let manifest: Self = parse_json(path, &text)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), ProfileError> {
        if self.manifest_version != MANIFEST_VERSION {
            return Err(ProfileError::UnsupportedManifestVersion(
                self.manifest_version,
            ));
        }
        let mut paths = HashSet::new();
        for source in &self.sources {
            ensure_safe_relative_path(&source.relative_path)?;
            ArchiveKind::from_relative_path(&source.relative_path)?;
            if !paths.insert(source.relative_path.clone()) {
                return Err(ProfileError::DuplicateManifestPath(
                    source.relative_path.clone(),
                ));
            }
            if source.site.trim().is_empty()
                || source.event.trim().is_empty()
                || source.year <= 0
                || source.retrieved_at <= 0
                || !(source.url.starts_with("https://") || source.url.starts_with("http://"))
                || source.size == 0
                || source.sha256.len() != 64
                || !source.sha256.bytes().all(|value| value.is_ascii_hexdigit())
            {
                return Err(ProfileError::InvalidManifestSource(
                    source.relative_path.clone(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusIngest {
    pub site: String,
    pub event: String,
    pub year: i32,
    pub retrieved_at: i64,
    pub inputs: Vec<String>,
    pub member_pattern: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCorpusBuildProfile {
    schema_version: u32,
    name: String,
    priority_reference_year: i32,
    manifest: String,
    ingests: Vec<CorpusIngest>,
    ranking_files: Vec<String>,
    #[serde(default)]
    alias_files: Vec<String>,
    rating_file: Option<String>,
    rating_config: Option<String>,
    input_book: Option<String>,
    coverage_snapshot_id: String,
}

#[derive(Clone, Debug)]
pub struct CorpusBuildProfile {
    pub schema_version: u32,
    pub name: String,
    pub priority_reference_year: i32,
    pub manifest: PathBuf,
    pub ingests: Vec<CorpusIngest>,
    pub ranking_files: Vec<PathBuf>,
    pub alias_files: Vec<PathBuf>,
    pub rating_file: Option<PathBuf>,
    pub rating_config: Option<PathBuf>,
    pub input_book: Option<PathBuf>,
    pub coverage_snapshot_id: String,
}

impl CorpusBuildProfile {
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let text = read_text(path)?;
        let raw: RawCorpusBuildProfile = parse_json(path, &text)?;
        if raw.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ProfileError::UnsupportedProfileVersion(raw.schema_version));
        }
        if raw.name.trim().is_empty() || raw.coverage_snapshot_id.trim().is_empty() {
            return Err(ProfileError::InvalidProfile(
                "name and coverage snapshot are required",
            ));
        }
        if raw.priority_reference_year <= 0 {
            return Err(ProfileError::InvalidProfile(
                "priority_reference_year must be positive",
            ));
        }
        if raw.rating_file.is_some() != raw.rating_config.is_some() {
            return Err(ProfileError::InvalidProfile(
                "rating_file and rating_config must both be paths or null",
            ));
        }
        ensure_safe_relative_path(&raw.manifest)?;
        let base = path.parent().unwrap_or_else(|| Path::new(""));
        let resolve_checked = |value: String| -> Result<PathBuf, ProfileError> {
            ensure_safe_relative_path(&value)?;
            Ok(base.join(value))
        };
        let manifest_path = resolve_checked(raw.manifest)?;
        let manifest = CorpusManifest::load(&manifest_path)?;

        let mut ingest_keys = HashSet::new();
        let mut referenced_inputs = HashSet::new();
        for ingest in &raw.ingests {
            let key = (ingest.site.clone(), ingest.event.clone(), ingest.year);
            if !ingest_keys.insert(key) {
                return Err(ProfileError::DuplicateIngest(format!(
                    "{}/{}/{}",
                    ingest.site, ingest.event, ingest.year
                )));
            }
            if ingest.site.trim().is_empty()
                || ingest.event.trim().is_empty()
                || ingest.year <= 0
                || ingest.retrieved_at <= 0
            {
                return Err(ProfileError::InvalidProfile("ingest metadata is invalid"));
            }
            if let Some(pattern) = ingest.member_pattern.as_deref() {
                Regex::new(pattern).map_err(|source| ProfileError::InvalidMemberPattern {
                    pattern: pattern.to_owned(),
                    source,
                })?;
            }
            if ingest.inputs.is_empty() {
                return Err(ProfileError::InvalidProfile(
                    "ingest inputs cannot be empty",
                ));
            }
            for input in &ingest.inputs {
                ensure_safe_relative_path(input)?;
                if !referenced_inputs.insert(input.clone()) {
                    return Err(ProfileError::DuplicateIngestInput(input.clone()));
                }
                let count = manifest
                    .sources
                    .iter()
                    .filter(|source| {
                        source.relative_path == *input
                            && source.site == ingest.site
                            && source.event == ingest.event
                            && source.year == ingest.year
                            && source.retrieved_at == ingest.retrieved_at
                    })
                    .count();
                if count != 1 {
                    return Err(ProfileError::IngestManifestMismatch(input.clone()));
                }
            }
        }
        if manifest
            .sources
            .iter()
            .any(|source| !referenced_inputs.contains(&source.relative_path))
        {
            return Err(ProfileError::InvalidProfile(
                "every manifest source must be referenced by one ingest",
            ));
        }

        let ranking_files = raw
            .ranking_files
            .into_iter()
            .map(resolve_checked)
            .collect::<Result<Vec<_>, _>>()?;
        let alias_files = raw
            .alias_files
            .into_iter()
            .map(resolve_checked)
            .collect::<Result<Vec<_>, _>>()?;
        let rating_file = raw.rating_file.map(resolve_checked).transpose()?;
        let rating_config = raw.rating_config.map(resolve_checked).transpose()?;
        let input_book = raw.input_book.map(resolve_checked).transpose()?;
        for path in ranking_files
            .iter()
            .chain(alias_files.iter())
            .chain(rating_file.iter())
            .chain(rating_config.iter())
            .chain(input_book.iter())
        {
            if !path.is_file() {
                return Err(ProfileError::MissingMetadataFile(path.clone()));
            }
        }
        Ok(Self {
            schema_version: raw.schema_version,
            name: raw.name,
            priority_reference_year: raw.priority_reference_year,
            manifest: manifest_path,
            ingests: raw.ingests,
            ranking_files,
            alias_files,
            rating_file,
            rating_config,
            input_book,
            coverage_snapshot_id: raw.coverage_snapshot_id,
        })
    }
}

fn read_text(path: &Path) -> Result<String, ProfileError> {
    fs::read_to_string(path).map_err(|source| ProfileError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_json<T: for<'de> Deserialize<'de>>(path: &Path, text: &str) -> Result<T, ProfileError> {
    serde_json::from_str(text).map_err(|source| ProfileError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn ensure_safe_relative_path(value: &str) -> Result<(), ProfileError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ProfileError::UnsafeRelativePath(value.to_owned()));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("invalid member_pattern {pattern:?}: {source}")]
    InvalidMemberPattern {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("metadata file does not exist: {0}")]
    MissingMetadataFile(PathBuf),
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported corpus source: {0}")]
    UnsupportedSource(String),
    #[error("unsupported profile schema version: {0}")]
    UnsupportedProfileVersion(u32),
    #[error("unsupported manifest version: {0}")]
    UnsupportedManifestVersion(u32),
    #[error("duplicate manifest path: {0}")]
    DuplicateManifestPath(String),
    #[error("duplicate ingest: {0}")]
    DuplicateIngest(String),
    #[error("duplicate ingest input: {0}")]
    DuplicateIngestInput(String),
    #[error("unsafe relative path: {0}")]
    UnsafeRelativePath(String),
    #[error("ingest input does not match manifest: {0}")]
    IngestManifestMismatch(String),
    #[error("invalid manifest source: {0}")]
    InvalidManifestSource(String),
    #[error("invalid profile: {0}")]
    InvalidProfile(&'static str),
}
