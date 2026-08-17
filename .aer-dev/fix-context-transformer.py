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
    "mid_start = '''        for semantic_id in &request.required_semantic_ids {\\n'''\n",
    "mid_start = '''        for semantic_id in &request.required_semantic_ids {\\n            let candidate = materialized\\n'''\n",
    "semantic selection anchor",
)

replace_once(
    '''    EditTarget,\n    SupportingContext,\n''',
    '''    EditTarget,\n    TestContext,\n    SupportingContext,\n''',
    "test demand enum",
)

replace_once(
    '''        let support_coverage = if objective_lower.contains(\"test\") { 2 } else { 1 };\n''',
    '''        let wants_test_context = objective_lower.contains(\"test\");\n''',
    "test intent flag",
)

replace_once(
    '''        demands.push(EvidenceDemand {\n            demand_id: \"objective:supporting-context\".to_owned(),\n            kind: EvidenceDemandKind::SupportingContext,\n            target: EvidenceDemandTarget::Objective,\n            minimum_tier: ContextTier::Structural,\n            required_provenance: EvidenceProvenance::IndexedSource,\n            minimum_coverage: support_coverage,\n            expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,\n            importance_milli: 700,\n            verification_critical: false,\n        });\n''',
    '''        if wants_test_context {\n            demands.push(EvidenceDemand {\n                demand_id: \"objective:test-context\".to_owned(),\n                kind: EvidenceDemandKind::TestContext,\n                target: EvidenceDemandTarget::Objective,\n                minimum_tier: ContextTier::SourceSpan,\n                required_provenance: EvidenceProvenance::IndexedSource,\n                minimum_coverage: 1,\n                expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,\n                importance_milli: 850,\n                verification_critical: false,\n            });\n        } else {\n            demands.push(EvidenceDemand {\n                demand_id: \"objective:supporting-context\".to_owned(),\n                kind: EvidenceDemandKind::SupportingContext,\n                target: EvidenceDemandTarget::Objective,\n                minimum_tier: ContextTier::Structural,\n                required_provenance: EvidenceProvenance::IndexedSource,\n                minimum_coverage: 1,\n                expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,\n                importance_milli: 700,\n                verification_critical: false,\n            });\n        }\n''',
    "test/support demand compilation",
)

replace_once(
    '''        EvidenceDemandKind::ChangeImpact | EvidenceDemandKind::SupportingContext\n''',
    '''        EvidenceDemandKind::ChangeImpact\n            | EvidenceDemandKind::TestContext\n            | EvidenceDemandKind::SupportingContext\n''',
    "structural demand kinds",
)

replace_once(
    '''        EvidenceDemandTarget::Objective => {\n            candidate.lexical_rank.is_some() || candidate.structural_rank.is_some()\n        }\n''',
    '''        EvidenceDemandTarget::Objective => {\n            if demand.kind == EvidenceDemandKind::TestContext {\n                is_test_source_path(&candidate.path)\n            } else {\n                candidate.lexical_rank.is_some() || candidate.structural_rank.is_some()\n            }\n        }\n''',
    "objective demand coverage",
)

replace_once(
    '''fn demand_satisfied(\n''',
    '''fn is_test_source_path(path: &str) -> bool {\n    let path = path.to_ascii_lowercase();\n    path.starts_with(\"tests/\")\n        || path.contains(\"/tests/\")\n        || path.contains(\"/test/\")\n        || path.ends_with(\"_test.rs\")\n        || path.ends_with(\"_test.py\")\n        || path.ends_with(\".test.ts\")\n        || path.ends_with(\".test.tsx\")\n        || path.ends_with(\".test.js\")\n        || path.ends_with(\".spec.ts\")\n        || path.ends_with(\".spec.js\")\n}\n\nfn demand_satisfied(\n''',
    "generic test path classifier",
)

replace_once(
    '''                demand.verification_critical && candidate_covers_demand(candidate, demand)\n''',
    '''                demand.verification_critical\n                    && !matches!(demand.target, EvidenceDemandTarget::Objective)\n                    && candidate_covers_demand(candidate, demand)\n''',
    "mandatory materialization scope",
)

path.write_text(text, encoding="utf-8")
print("Context transformer repaired: precise selection anchor + explicit test-context demand")
