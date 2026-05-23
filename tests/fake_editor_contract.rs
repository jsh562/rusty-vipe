//! T114: assert the `fake-editor` source documents every supported transform
//! (FR-026). Reads the source file via `include_str!` and verifies the
//! docstring table mentions each transform name.

#[test]
fn fake_editor_documents_every_transform() {
    const SOURCE: &str = include_str!("bin/fake_editor.rs");
    for transform in &[
        "delete-line:",
        "replace:",
        "passthrough",
        "exit-nonzero:",
        "noop",
        "report-argv",
        "report-filename",
        "report-stdio",
    ] {
        assert!(
            SOURCE.contains(transform),
            "FR-026: fake-editor source must mention transform {transform:?}"
        );
    }
}
