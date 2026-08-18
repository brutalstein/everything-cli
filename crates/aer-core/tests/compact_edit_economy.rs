use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use aer_core::edit_abi::{
    CompactEditPlan, EditLimits, EditOperation, apply_edit_plan, sha256,
};

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aer-compact-edit-economy-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("src")).expect("temp root");
    root
}

#[test]
fn sparse_edit_output_is_materially_smaller_than_whole_file_replacement() {
    let root = temp_root();
    let mut base = String::new();
    for line in 1..=2_000 {
        base.push_str(&format!("const VALUE_{line:04}: usize = {line};\n"));
    }
    fs::write(root.join("src/generated.rs"), base.as_bytes()).expect("base");

    let target_line = 1_337_u32;
    let old_line = format!("const VALUE_{target_line:04}: usize = {target_line};\n");
    let replacement = format!("const VALUE_{target_line:04}: usize = 424242;\n");
    let plan = CompactEditPlan {
        summary: "change one generated constant".to_owned(),
        operations: vec![EditOperation::ReplaceRange {
            path: "src/generated.rs".to_owned(),
            base_file_sha256: sha256(base.as_bytes()),
            start_line: target_line,
            end_line: target_line,
            expected_segment_sha256: sha256(old_line.as_bytes()),
            replacement: replacement.as_bytes().to_vec(),
        }],
    };

    let compact_wire = serde_json::json!({
        "summary":"change one generated constant",
        "operations":[{
            "op":"replace_range",
            "path":"src/generated.rs",
            "base_file_sha256":sha256(base.as_bytes()),
            "start_line":target_line,
            "end_line":target_line,
            "expected_segment_sha256":sha256(old_line.as_bytes()),
            "replacement":replacement,
        }]
    })
    .to_string();
    let whole_file_wire = serde_json::json!({
        "summary":"change one generated constant",
        "edits":[{"path":"src/generated.rs","content":base}]
    })
    .to_string();

    let receipt = apply_edit_plan(&root, &plan, EditLimits::default()).expect("compact apply");
    assert_eq!(receipt.operation_count, 1);
    assert_eq!(receipt.changed_output_bytes, replacement.len());
    assert!(
        compact_wire.len().saturating_mul(20) < whole_file_wire.len(),
        "sparse compact payload should be at least 20x smaller: compact={} whole={}",
        compact_wire.len(),
        whole_file_wire.len()
    );
    assert!(receipt.changed_output_bytes.saturating_mul(500) < base.len());

    let result = fs::read_to_string(root.join("src/generated.rs")).expect("result");
    assert!(result.contains("const VALUE_1337: usize = 424242;"));
    fs::remove_dir_all(root).expect("cleanup");
}
