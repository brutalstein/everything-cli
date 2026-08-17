include!("../../../../tools/aer-provider-acceptance/acceptance.rs");

// Keep the diagnostic source self-contained while ensuring items used by its
// test-only assertions remain checked by normal all-target builds as well.
const _: &str = CURRENT_INSTRUCTION;
const _: usize = std::mem::size_of::<BTreeSet<&'static str>>();
