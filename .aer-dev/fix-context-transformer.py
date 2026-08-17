from pathlib import Path

path = Path(__file__).with_name("apply-context-economy-v2.py")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one source fragment, found {count}")
    text = text.replace(old, new, 1)


replace_once(
    "mid_start = '''        for semantic_id in &request.required_semantic_ids {\n'''\n",
    "mid_start = '''        for semantic_id in &request.required_semantic_ids {\n            let candidate = materialized\n'''\n",
    "semantic selection anchor",
)

replace_once(
    "    EditTarget,\n    SupportingContext,\n",
    "    EditTarget,\n    TestContext,\n    SupportingContext,\n",
    "test demand enum",
)

replace_once(
    '        let support_coverage = if objective_lower.contains("test") { 2 } else { 1 };\n',
    '        let wants_test_context = objective_lower.contains("test");\n',
    "test intent flag",
)

replace_once(
    '''        demands.push(EvidenceDemand {
            demand_id: "objective:supporting-context".to_owned(),
            kind: EvidenceDemandKind::SupportingContext,
            target: EvidenceDemandTarget::Objective,
            minimum_tier: ContextTier::Structural,
            required_provenance: EvidenceProvenance::IndexedSource,
            minimum_coverage: support_coverage,
            expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
            importance_milli: 700,
            verification_critical: false,
        });
''',
    '''        if wants_test_context {
            demands.push(EvidenceDemand {
                demand_id: "objective:test-context".to_owned(),
                kind: EvidenceDemandKind::TestContext,
                target: EvidenceDemandTarget::Objective,
                minimum_tier: ContextTier::SourceSpan,
                required_provenance: EvidenceProvenance::IndexedSource,
                minimum_coverage: 1,
                expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
                importance_milli: 850,
                verification_critical: false,
            });
        } else {
            demands.push(EvidenceDemand {
                demand_id: "objective:supporting-context".to_owned(),
                kind: EvidenceDemandKind::SupportingContext,
                target: EvidenceDemandTarget::Objective,
                minimum_tier: ContextTier::Structural,
                required_provenance: EvidenceProvenance::IndexedSource,
                minimum_coverage: 1,
                expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
                importance_milli: 700,
                verification_critical: false,
            });
        }
''',
    "test/support demand compilation",
)

replace_once(
    '''    }
    demands
}

fn demand_needs_lexical''',
    '''    }
    if request.objective.to_ascii_lowercase().contains("test")
        && !demands
            .iter()
            .any(|demand| demand.kind == EvidenceDemandKind::TestContext)
    {
        demands.push(EvidenceDemand {
            demand_id: "objective:test-context".to_owned(),
            kind: EvidenceDemandKind::TestContext,
            target: EvidenceDemandTarget::Objective,
            minimum_tier: ContextTier::SourceSpan,
            required_provenance: EvidenceProvenance::IndexedSource,
            minimum_coverage: 1,
            expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
            importance_milli: 850,
            verification_critical: false,
        });
    }
    demands
}

fn demand_needs_lexical''',
    "test demand beside explicit evidence",
)

replace_once(
    "        EvidenceDemandKind::ChangeImpact | EvidenceDemandKind::SupportingContext\n",
    "        EvidenceDemandKind::ChangeImpact\n            | EvidenceDemandKind::TestContext\n            | EvidenceDemandKind::SupportingContext\n",
    "structural demand kinds",
)

replace_once(
    '''        EvidenceDemandTarget::Objective => {
            candidate.lexical_rank.is_some() || candidate.structural_rank.is_some()
        }
''',
    '''        EvidenceDemandTarget::Objective => {
            if demand.kind == EvidenceDemandKind::TestContext {
                is_test_source_path(&candidate.path)
            } else {
                candidate.lexical_rank.is_some() || candidate.structural_rank.is_some()
            }
        }
''',
    "objective demand coverage",
)

replace_once(
    "fn demand_satisfied(\n",
    '''fn is_test_source_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.ends_with("_test.rs")
        || path.ends_with("_test.py")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.tsx")
        || path.ends_with(".test.js")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.js")
}

fn demand_satisfied(
''',
    "generic test path classifier",
)

replace_once(
    "                demand.verification_critical && candidate_covers_demand(candidate, demand)\n",
    '''                demand.verification_critical
                    && !matches!(demand.target, EvidenceDemandTarget::Objective)
                    && candidate_covers_demand(candidate, demand)
''',
    "mandatory materialization scope",
)

path.write_text(text, encoding="utf-8")
print("Context transformer repaired: demand-aware selection + explicit test-context preservation")
