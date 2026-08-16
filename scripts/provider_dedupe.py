from pathlib import Path

path = Path("crates/aer-provider/src/delegated.rs")
text = path.read_text(encoding="utf-8")
block = '''    #[test]
    fn bounded_capture_distinguishes_exact_limit_from_overflow() {
        let exact =
            super::capture_bounded(std::io::Cursor::new(b"abcd"), 4).expect("exact capture");
        assert_eq!(exact.bytes, b"abcd");
        assert!(!exact.truncated);

        let overflow =
            super::capture_bounded(std::io::Cursor::new(b"abcde"), 4).expect("overflow capture");
        assert_eq!(overflow.bytes, b"abcd");
        assert!(overflow.truncated);
    }
'''
if text.count(block) != 2:
    raise SystemExit(f"expected exactly two duplicate capture tests, found {text.count(block)}")
first = text.find(block)
second = text.find(block, first + len(block))
text = text[:second] + text[second + len(block):]
path.write_text(text, encoding="utf-8")
