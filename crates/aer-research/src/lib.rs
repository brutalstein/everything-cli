//! Bounded, claim-oriented research artifacts.
//!
//! Acquisition transports live outside this crate. This boundary accepts source-
//! backed external evidence, validates the executable contract, enforces hard
//! budgets/cross references, and deliberately exposes no API that can promote an
//! external claim directly into an accepted requirement or system decision.

use std::{collections::BTreeSet, error::Error, fmt};

use aer_contracts::embedded::EmbeddedContractRegistry;
use aer_domain::{
    contracts::CoreContract,
    spec::{ResearchClaimStatus, ResearchFinding},
};
use serde_json::Value;

pub const MAX_RESEARCH_ARTIFACT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RESEARCH_SOURCES: usize = 32;
pub const MAX_RESEARCH_CLAIMS: usize = 128;
pub const MAX_SOURCE_URI_BYTES: usize = 4096;
pub const MAX_CLAIM_STATEMENT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedResearchArtifact {
    value: Value,
    research_id: String,
    question: String,
    findings: Vec<ResearchFinding>,
    source_count: usize,
}

impl ValidatedResearchArtifact {
    /// Validates an externally supplied artifact as untrusted evidence.
    ///
    /// Non-empty `promoted_refs` are rejected here because promotion is a local
    /// authority decision, never an instruction accepted from external content.
    pub fn ingest_untrusted(value: Value) -> Result<Self, ResearchError> {
        let bytes = serde_json::to_vec(&value)?;
        if bytes.len() > MAX_RESEARCH_ARTIFACT_BYTES {
            return Err(ResearchError::Budget(format!(
                "artifact exceeds {MAX_RESEARCH_ARTIFACT_BYTES} bytes"
            )));
        }

        let registry = EmbeddedContractRegistry::load()
            .map_err(|error| ResearchError::Contract(error.to_string()))?;
        registry
            .validate_current(CoreContract::ResearchArtifact, &value)
            .map_err(|error| ResearchError::Contract(error.to_string()))?;

        if value
            .get("promoted_refs")
            .and_then(Value::as_array)
            .is_some_and(|refs| !refs.is_empty())
        {
            return Err(ResearchError::Authority(
                "untrusted research artifact cannot arrive with promoted_refs".to_owned(),
            ));
        }

        let research_id = value
            .get("research_id")
            .and_then(Value::as_str)
            .expect("schema validated research_id")
            .to_owned();
        let question = value
            .get("question")
            .and_then(Value::as_str)
            .expect("schema validated question")
            .to_owned();
        let sources = value
            .get("sources")
            .and_then(Value::as_array)
            .expect("schema validated sources");
        let claims = value
            .get("claims")
            .and_then(Value::as_array)
            .expect("schema validated claims");

        if sources.len() > MAX_RESEARCH_SOURCES {
            return Err(ResearchError::Budget(format!(
                "source count exceeds {MAX_RESEARCH_SOURCES}"
            )));
        }
        if claims.len() > MAX_RESEARCH_CLAIMS {
            return Err(ResearchError::Budget(format!(
                "claim count exceeds {MAX_RESEARCH_CLAIMS}"
            )));
        }

        let mut source_ids = BTreeSet::new();
        for source in sources {
            let source_id = source
                .get("source_id")
                .and_then(Value::as_str)
                .expect("schema validated source_id");
            if !source_ids.insert(source_id.to_owned()) {
                return Err(ResearchError::Integrity(format!(
                    "duplicate source_id: {source_id}"
                )));
            }
            let uri = source
                .get("uri")
                .and_then(Value::as_str)
                .expect("schema validated uri");
            if uri.len() > MAX_SOURCE_URI_BYTES {
                return Err(ResearchError::Budget(format!(
                    "source URI exceeds {MAX_SOURCE_URI_BYTES} bytes: {source_id}"
                )));
            }
            let content_hash = source
                .get("content_hash")
                .and_then(Value::as_str)
                .expect("schema validated content_hash");
            if content_hash.trim().is_empty() {
                return Err(ResearchError::Integrity(format!(
                    "source content_hash is empty: {source_id}"
                )));
            }
        }

        let mut claim_ids = BTreeSet::new();
        let mut findings = Vec::with_capacity(claims.len());
        for claim in claims {
            let claim_id = claim
                .get("claim_id")
                .and_then(Value::as_str)
                .expect("schema validated claim_id");
            if !claim_ids.insert(claim_id.to_owned()) {
                return Err(ResearchError::Integrity(format!(
                    "duplicate claim_id: {claim_id}"
                )));
            }
            let statement = claim
                .get("statement")
                .and_then(Value::as_str)
                .expect("schema validated claim statement");
            if statement.len() > MAX_CLAIM_STATEMENT_BYTES {
                return Err(ResearchError::Budget(format!(
                    "claim statement exceeds {MAX_CLAIM_STATEMENT_BYTES} bytes: {claim_id}"
                )));
            }
            let refs = claim
                .get("source_refs")
                .and_then(Value::as_array)
                .expect("schema validated source_refs")
                .iter()
                .map(|source| {
                    source
                        .as_str()
                        .expect("schema validated source ref")
                        .to_owned()
                })
                .collect::<Vec<_>>();
            if refs.is_empty() {
                return Err(ResearchError::Integrity(format!(
                    "claim has no source refs: {claim_id}"
                )));
            }
            for source_ref in claim
                .get("counterevidence_refs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .chain(
                    claim
                        .get("source_refs")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten(),
                )
            {
                let source_ref = source_ref.as_str().expect("schema validated source ref");
                if !source_ids.contains(source_ref) {
                    return Err(ResearchError::Integrity(format!(
                        "claim {claim_id} references unknown source {source_ref}"
                    )));
                }
            }

            let confidence = claim
                .get("confidence")
                .and_then(Value::as_f64)
                .expect("schema validated confidence");
            let confidence_milli = (confidence * 1000.0).round().clamp(0.0, 1000.0) as u16;
            let status = match claim
                .get("status")
                .and_then(Value::as_str)
                .expect("schema validated claim status")
            {
                "supported" => ResearchClaimStatus::Supported,
                "contested" => ResearchClaimStatus::Contested,
                "insufficient" => ResearchClaimStatus::Insufficient,
                "superseded" => ResearchClaimStatus::Superseded,
                _ => unreachable!("schema validates claim status"),
            };
            findings.push(ResearchFinding {
                research_id: research_id.clone(),
                claim_id: claim_id.to_owned(),
                statement: statement.to_owned(),
                status,
                confidence_milli,
                source_refs: refs,
            });
        }

        let source_count = sources.len();
        Ok(Self {
            value,
            research_id,
            question,
            findings,
            source_count,
        })
    }

    #[must_use]
    pub fn research_id(&self) -> &str {
        &self.research_id
    }

    #[must_use]
    pub fn question(&self) -> &str {
        &self.question
    }

    #[must_use]
    pub fn source_count(&self) -> usize {
        self.source_count
    }

    #[must_use]
    pub fn findings(&self) -> &[ResearchFinding] {
        &self.findings
    }

    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ResearchError> {
        serde_json::to_vec(&self.value).map_err(ResearchError::from)
    }
}

#[derive(Debug)]
pub enum ResearchError {
    Contract(String),
    Budget(String),
    Integrity(String),
    Authority(String),
    Json(serde_json::Error),
}

impl fmt::Display for ResearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(message) => write!(formatter, "research contract: {message}"),
            Self::Budget(message) => write!(formatter, "research budget: {message}"),
            Self::Integrity(message) => write!(formatter, "research integrity: {message}"),
            Self::Authority(message) => write!(formatter, "research authority: {message}"),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl Error for ResearchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Contract(_) | Self::Budget(_) | Self::Integrity(_) | Self::Authority(_) => None,
        }
    }
}

impl From<serde_json::Error> for ResearchError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ValidatedResearchArtifact;

    fn artifact() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "research_id": "RES-1",
            "question": "Which external claim is supported?",
            "observed_at": "2026-08-16T00:00:00Z",
            "sources": [{
                "source_id": "SRC-1",
                "uri": "https://example.invalid/spec",
                "source_class": "official",
                "retrieved_at": "2026-08-16T00:00:00Z",
                "content_hash": "sha256:abc"
            }],
            "claims": [{
                "claim_id": "CLM-1",
                "statement": "A source-backed external claim.",
                "source_refs": ["SRC-1"],
                "confidence": 0.9,
                "status": "supported"
            }]
        })
    }

    #[test]
    fn accepts_bounded_source_backed_artifact() {
        let artifact = ValidatedResearchArtifact::ingest_untrusted(artifact()).expect("artifact");
        assert_eq!(artifact.research_id(), "RES-1");
        assert_eq!(artifact.source_count(), 1);
        assert_eq!(artifact.findings().len(), 1);
        assert_eq!(artifact.findings()[0].confidence_milli, 900);
    }

    #[test]
    fn external_artifact_cannot_self_promote() {
        let mut value = artifact();
        value["promoted_refs"] = json!(["DEC-1"]);
        let error = ValidatedResearchArtifact::ingest_untrusted(value)
            .expect_err("promotion must be rejected");
        assert!(
            error
                .to_string()
                .contains("cannot arrive with promoted_refs")
        );
    }

    #[test]
    fn dangling_claim_source_is_rejected() {
        let mut value = artifact();
        value["claims"][0]["source_refs"] = json!(["SRC-MISSING"]);
        assert!(ValidatedResearchArtifact::ingest_untrusted(value).is_err());
    }
}
