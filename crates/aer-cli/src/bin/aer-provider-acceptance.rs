// Keep the diagnostic source self-contained while ensuring items used by its
// test-only assertions remain checked by normal all-target builds as well.
// Rust item resolution is order-independent, so these guards may precede the
// included implementation without changing runtime behavior.
const _: &str = CURRENT_INSTRUCTION;
const _: usize = std::mem::size_of::<BTreeSet<&'static str>>();

include!("../../../../tools/aer-provider-acceptance/acceptance.rs");
