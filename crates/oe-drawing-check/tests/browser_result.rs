//! The browser result is read back from the page Chrome dumps; a failed, pending or malformed result is an
//! error, never a pass. The harness substitutes only the page location, mode, focus label and region.
use oe_drawing_check::browser::{MODES, file_url, harness, read_result};
use serde_json::json;
use std::path::Path;

fn dom(text: &str) -> String {
    format!(
        "<html><head></head><body><pre id=\"checks\">{text}</pre><iframe id=\"f\"></iframe></body></html>"
    )
}

/// Text-node escaping as an HTML serializer writes it.
fn escaped(value: &serde_json::Value) -> String {
    serde_json::to_string(value)
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\u{a0}', "&nbsp;")
}

#[test]
fn the_browser_result_is_read_from_the_dumped_page_and_a_failure_is_kept() {
    let passed = json!({"status":"passed","count":2,"checks":["Typed expression graph loaded","Kind a<b> & c"],"facts":{"label":"Ø20\u{a0}h7"},"mode":"kind"});
    assert_eq!(read_result(&dom(&escaped(&passed))).unwrap(), passed);
    let failed = json!({"status":"failed","error":"Error: Default view draws every structural line once as one element"});
    let error = read_result(&dom(&escaped(&failed)))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Default view draws every structural line once"),
        "{error}"
    );
    for broken in [
        dom("pending"),
        "<html><body>no result</body></html>".into(),
        "<pre id=\"checks\">{".into(),
    ] {
        assert!(read_result(&broken).is_err(), "{broken}");
    }
    let miscounted = json!({"status":"passed","count":3,"checks":["one"]});
    assert!(read_result(&dom(&escaped(&miscounted))).is_err());
}

#[test]
fn the_harness_carries_the_mode_focus_and_region_without_answers() {
    for mode in MODES {
        let text = harness(
            "file:///tmp/a%20b/page.html",
            mode,
            Some("Ø20"),
            Some(&[0.0, 1.0, 2.0, 3.0]),
        )
        .unwrap();
        assert!(text.contains(&format!("const mode='{mode}'")), "{mode}");
        assert!(text.contains("\"Ø20\"") && text.contains("[0.0,1.0,2.0,3.0]"));
        assert!(text.contains("<iframe id=\"f\" src=\"file:///tmp/a%20b/page.html\"></iframe>"));
        // The same page once more with scripting disabled: the drawing as embedded.
        assert!(text.contains(
            "<iframe id=\"p\" sandbox=\"allow-same-origin\" src=\"file:///tmp/a%20b/page.html\" style=\"display:none\"></iframe>"
        ));
        assert!(
            !text.contains("__MODE__") && !text.contains("__FOCUS__") && !text.contains("__BOX__")
        );
        // Every mode checks that masks keep the paper colour and the drawing over them its own.
        assert!(text.contains("'Masks keep the paper colour'"), "{mode}");
        assert!(text.contains("'Drawing over masks keeps its own colour'"), "{mode}");
        // Every mode checks the line widths, what a text background covers, the original view, the
        // empty notice and the display by member; none of these labels depends on the mode.
        for label in [
            "'Thick and thin line widths are 2:1'",
            "'d is one sixth of the median dimension terminator'",
            "'Every drawn stroke has the width of its member rule'",
            "'No mask hides ink of its own annotation except ink crossing its text'",
            "'Original view keeps the converter stroke widths'",
            "'Original view keeps the converted drawing order'",
            "'The empty notice is hidden whenever something is drawn'",
            "'Only the visible outline selected: no empty notice over drawn structural lines'",
            "'Groups with member identity are shown exactly when a claiming member passes'",
            "'Every data-member names a native member of its root'",
            "'Content that never plots is drawn faint in the classified views and nothing else is'",
            "'Expression details name every referenced view by its label'",
        ] {
            assert!(text.contains(label), "{mode}: {label}");
        }
    }
    let plain = harness("file:///p.html", "kind", None, None).unwrap();
    assert!(plain.contains("const fl=\"\"") && plain.contains("const box=null"));
    assert!(harness("file:///p.html", "unknown-mode", None, None).is_err());
    assert!(harness("file:///p.html", "kind", None, Some(&[0.0, 1.0, 2.0])).is_err());
    assert_eq!(
        file_url(Path::new("/a b/图.html")),
        "file:///a%20b/%E5%9B%BE.html"
    );
}
