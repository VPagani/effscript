//! The parser always terminates, including when it tries a construct and backtracks.

use std::sync::mpsc;
use std::time::Duration;

fn parses_in_time(src: &'static str) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let parsed = efs_syntax::parse(src);
        tx.send(parsed.diagnostics.len()).ok();
    });
    rx.recv_timeout(Duration::from_secs(5)).expect("the parser did not terminate");
}

#[test]
fn markup_in_an_arrow_after_an_object_argument() {
    parses_in_time("const v = g({ a: 1 }, (i) =>\n  <d\n    x={[\n      1,\n    ]}\n  >\n    {i}\n  </d>\n);\n");
}
