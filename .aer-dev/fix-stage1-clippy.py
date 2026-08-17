from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
path = ROOT / "crates/aer-context/src/lib.rs"
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one anchor, found {count}")
    text = text.replace(old, new, 1)


replace_once(
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

replace_once(
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

replace_once(
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

path.write_text(text, encoding="utf-8")
print("Context selection inputs consolidated")
