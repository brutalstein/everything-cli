from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
path = ROOT / "crates/aer-context/src/lib.rs"
text = path.read_text(encoding="utf-8")
model_path = ROOT / "crates/aer-context/src/model.rs"
model = model_path.read_text(encoding="utf-8")


def replace_once(source: str, old: str, new: str, label: str) -> str:
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one anchor, found {count}")
    return source.replace(old, new, 1)


text = replace_once(
    text,
    '''        let pack = self.select(
            workspace_root,
            index,
            &snapshot_id,
            request,
            &demands,
            retrieval_trace,
            ranked,
        )?;
''',
    '''        let pack = self.select(
            workspace_root,
            index,
            SelectionInputs {
                snapshot_id: &snapshot_id,
                request,
                demands: &demands,
                retrieval_trace,
                ranked,
            },
        )?;
''',
    "selection call",
)

text = replace_once(
    text,
    '''    fn select(
        &self,
        workspace_root: &Path,
        index: &RepositoryIndex,
        snapshot_id: &str,
        request: &ContextRequest,
        demands: &[EvidenceDemand],
        mut retrieval_trace: RetrievalTrace,
        ranked: Vec<Candidate>,
    ) -> Result<ContextPack, ContextError> {
        let available = request.input_token_budget;
''',
    '''    fn select(
        &self,
        workspace_root: &Path,
        index: &RepositoryIndex,
        inputs: SelectionInputs<'_>,
    ) -> Result<ContextPack, ContextError> {
        let SelectionInputs {
            snapshot_id,
            request,
            demands,
            mut retrieval_trace,
            ranked,
        } = inputs;
        let available = request.input_token_budget;
''',
    "selection signature",
)

text = replace_once(
    text,
    '''#[derive(Clone, Debug)]
struct Candidate {
''',
    '''struct SelectionInputs<'a> {
    snapshot_id: &'a str,
    request: &'a ContextRequest,
    demands: &'a [EvidenceDemand],
    retrieval_trace: RetrievalTrace,
    ranked: Vec<Candidate>,
}

#[derive(Clone, Debug)]
struct Candidate {
''',
    "selection input type",
)

model = replace_once(
    model,
    '''    RequirementContext,
    RuntimeEvidence,
''',
    '''    RequirementContext,
    ImplementationContext,
    RuntimeEvidence,
''',
    "implementation demand kind",
)

text = replace_once(
    text,
    '''    for (index, semantic_id) in request.required_semantic_ids.iter().enumerate() {
        demands.push(EvidenceDemand {
            demand_id: format!("requirement-context:{index}:{semantic_id}"),
            kind: EvidenceDemandKind::RequirementContext,
            target: EvidenceDemandTarget::SemanticId(semantic_id.clone()),
            minimum_tier: ContextTier::Structural,
            required_provenance: EvidenceProvenance::IndexedSource,
            minimum_coverage: 1,
            expansion_policy: EvidenceExpansionPolicy::Never,
            importance_milli: 950,
            verification_critical: true,
        });
    }
''',
    '''    let implementation_task = objective_requires_implementation(&request.objective);
    for (index, semantic_id) in request.required_semantic_ids.iter().enumerate() {
        let kind = if implementation_task {
            EvidenceDemandKind::ImplementationContext
        } else {
            EvidenceDemandKind::RequirementContext
        };
        demands.push(EvidenceDemand {
            demand_id: format!("requirement-context:{index}:{semantic_id}"),
            kind,
            target: EvidenceDemandTarget::SemanticId(semantic_id.clone()),
            minimum_tier: if implementation_task {
                ContextTier::SourceSpan
            } else {
                ContextTier::Structural
            },
            required_provenance: if implementation_task {
                EvidenceProvenance::ExactSource
            } else {
                EvidenceProvenance::IndexedSource
            },
            minimum_coverage: 1,
            expansion_policy: if implementation_task {
                EvidenceExpansionPolicy::BoundedSourceSpan
            } else {
                EvidenceExpansionPolicy::Never
            },
            importance_milli: 950,
            verification_critical: true,
        });
    }
''',
    "semantic implementation demand",
)

text = replace_once(
    text,
    '''        EvidenceDemandTarget::SemanticId(semantic_id) => {
            candidate.required_semantic_ids.contains(semantic_id)
        }
''',
    '''        EvidenceDemandTarget::SemanticId(semantic_id) => {
            candidate.required_semantic_ids.contains(semantic_id)
                && (demand.kind != EvidenceDemandKind::ImplementationContext
                    || !is_test_source_path(&candidate.path))
        }
''',
    "implementation semantic coverage",
)

text = replace_once(
    text,
    '''fn demand_needs_lexical(demand: &EvidenceDemand) -> bool {
''',
    '''fn objective_requires_implementation(objective: &str) -> bool {
    objective
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .any(|token| {
            matches!(
                token.to_ascii_lowercase().as_str(),
                "fix"
                    | "implement"
                    | "change"
                    | "modify"
                    | "update"
                    | "repair"
                    | "refactor"
                    | "add"
                    | "remove"
                    | "delete"
                    | "create"
            )
        })
}

fn demand_needs_lexical(demand: &EvidenceDemand) -> bool {
''',
    "implementation objective classifier",
)

path.write_text(text, encoding="utf-8")
model_path.write_text(model, encoding="utf-8")
print("Context selection inputs consolidated; mutating semantic tasks retain implementation evidence")
