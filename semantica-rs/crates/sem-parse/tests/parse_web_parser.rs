//! Characterisation tests for `web_parser` (WebParser, HTMLContentParser,
//! JavaScriptRenderer) against the original (fixtures/markup, section "web"), plus the
//! static WebDriver the port uses in place of Selenium (no browser; expectations by
//! construction, with a fake HTTP fetcher - no network).

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{Value, json};

use sem_parse::methods::ParseOutput;
use sem_parse::web_parser::{HttpFetcher, StaticWebDriver, WebDriver};
use sem_parse::{HTMLContentParser, JavaScriptRenderer, Options, WebParser};
use sem_utils::SemanticaError;

static ORACLE: Lazy<Value> = Lazy::new(|| common::load_oracle("markup"));

fn fixtures() -> PathBuf {
    common::fixture("markup")
}

fn outcome<T: Serialize>(result: Result<T, SemanticaError>) -> Value {
    match result {
        Ok(v) => json!({ "ok": serde_json::to_value(v).expect("serialisable") }),
        Err(e) => common::err_json(&e),
    }
}

fn expected(key: &str) -> Value {
    let v = &ORACLE["web"][key];
    assert!(!v.is_null(), "oracle has no web case {key}");
    let text = serde_json::to_string(v).unwrap();
    let dir = fixtures().display().to_string();
    serde_json::from_str(&text.replace("@FIXTURES@", &dir)).unwrap()
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

fn page() -> PathBuf {
    fixtures().join("html").join("01_basic.html")
}

fn check(key: &str, actual: Value) {
    assert_eq!(actual, expected(key), "web case {key}");
}

#[test]
fn test_web_parser_matches_oracle() {
    common::quiet_progress();
    let wp = WebParser::default();
    let none = Options::new();
    let as_dataclass = |r: Result<ParseOutput, SemanticaError>| outcome(r.map(|o| o.to_value()));
    check(
        "parse_html",
        as_dataclass(wp.parse_web_content("<p>web <a href='/x'>x</a></p>", Some("html"), Some("http://e.com/a/"), &none)),
    );
    check("parse_xml", as_dataclass(wp.parse_web_content("<root><a>1</a></root>", Some("xml"), None, &none)));
    check("parse_other", as_dataclass(wp.parse_web_content("{}", Some("json"), None, &none)));
    check("text_clean", outcome(wp.extract_text("<p>Hello   <script>x</script>World</p>", &none)));
    check(
        "text_raw",
        outcome(wp.extract_text("<p>Hello   <script>x</script>World</p>", &opts(json!({"clean": false})))),
    );
    check(
        "text_render",
        outcome(wp.extract_text("<p>Rendered <b>text</b></p>", &opts(json!({"render_javascript": true})))),
    );
    check("text_file", outcome(wp.extract_text(page(), &none)));
    check(
        "links",
        outcome(wp.extract_links("<a href='a'>1</a><a href='/b' rel='x'>2</a>", Some("http://e.com/d/"))),
    );
    check("render_string", outcome(wp.render_javascript("<p>no js</p>", &none)));
    check("render_file", outcome(wp.render_javascript(page(), &none)));
    let wp2 = WebParser::new(None, opts(json!({"js": {"use_selenium": false}})));
    assert_eq!(json!({"js_use_selenium": wp2.js_renderer.use_selenium}), expected("web_parser_config"));
    assert_eq!(wp.logger.name(), "semantica.web_parser");
}

#[test]
fn test_javascript_renderer_without_webdriver_matches_oracle() {
    common::quiet_progress();
    let jr = JavaScriptRenderer::default();
    check("js_execute", outcome(jr.execute_script("return 1", &Options::new())));
    check("js_wait", json!({"ok": jr.wait_for_content(None, None)}));
    // The reference environment has no selenium: the import-failure branch.
    let unavailable = JavaScriptRenderer::without_webdriver(opts(json!({"use_selenium": true})));
    assert_eq!(json!(unavailable.use_selenium), expected("js_use_selenium"));
    check("js_selenium_render", outcome(unavailable.render_page("<p>x</p>", &Options::new())));
    assert!(!unavailable.has_driver());
}

#[test]
fn test_html_content_parser_matches_oracle() {
    common::quiet_progress();
    let hcp = HTMLContentParser::new(Options::new());
    let none = Options::new();
    check(
        "content_structured",
        outcome(hcp.extract_structured_content("<h1>T</h1><p>x</p><ul><li>a</li></ul>", &none)),
    );
    check("content_structured_file", outcome(hcp.extract_structured_content(page(), &none)));
    check("content_parse", outcome(hcp.parse("<p>x</p>", None, &none)));
    let docs = [
        "<html><head><script>var a = 1;</script><style>p{}</style></head><body><!-- note --><p>  a   b\n c </p><pre>  keep   this  </pre><code> x  y </code><noscript>ns</noscript></body></html>",
        "<p>Tom &amp; Jerry &lt;3 &quot;q&quot;</p><br><img src='a.png' alt=\"x>y\">",
        "<meta charset='ISO-8859-1'><meta http-equiv='Content-Type' content='text/html; charset=latin-1'><p class='a  b'>t</p>",
        "<!DOCTYPE html><div>\n\n   </div><![CDATA[x]]><?php echo 1 ?>",
        "<table><tr><td> 1 </td></tr></table><textarea>  t  </textarea>",
    ];
    let keep = opts(json!({"remove_scripts": false, "remove_comments": false, "normalize_whitespace": false}));
    for (i, doc) in docs.iter().enumerate() {
        check(&format!("clean_{i}"), outcome(hcp.clean_html(*doc, &none)));
        check(&format!("clean_{i}_keep"), outcome(hcp.clean_html(*doc, &keep)));
    }
    check("clean_file", outcome(hcp.clean_html(page(), &none)));
}

/// A fake HTTP fetcher (the test double for the network).
struct FakeFetcher;

impl HttpFetcher for FakeFetcher {
    fn fetch(&self, url: &str) -> Result<String, SemanticaError> {
        Ok(format!("<title>{url}</title><p id=x>remote"))
    }
}

#[test]
fn test_static_webdriver_rendering() {
    common::quiet_progress();
    let factory: sem_parse::web_parser::DriverFactory = Arc::new(|args: &[String]| {
        assert_eq!(args, ["--headless", "--no-sandbox", "--disable-dev-shm-usage"]);
        Ok(Box::new(StaticWebDriver::new(Arc::new(FakeFetcher))) as Box<dyn WebDriver>)
    });
    let jr = JavaScriptRenderer::with_driver_factory(opts(json!({"use_selenium": true})), factory);
    assert!(jr.use_selenium);
    assert!(!jr.has_driver());
    assert!(!jr.wait_for_content(None, Some(1)));
    // Content strings go through a temporary .html file; the page source is the HTML5 DOM.
    let rendered = jr
        .render_page("<p>one<p>two", &opts(json!({"wait_time": 1, "wait_for_selector": "p"})))
        .unwrap();
    assert_eq!(rendered, "<html><head></head><body><p>one</p><p>two</p></body></html>");
    assert!(jr.has_driver());
    assert!(jr.wait_for_content(None, Some(1)));
    assert_eq!(jr.execute_script("return document.readyState", &Options::new()).unwrap(), json!("complete"));
    let err = jr.execute_script("return 1", &Options::new()).unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
    assert!(err.to_string().starts_with("Failed to execute script: Message: javascript error"));
    // A selector that never appears times out (TimeoutException wrapped in ProcessingError).
    let err = jr
        .render_page(page(), &opts(json!({"wait_time": 0, "wait_for_selector": "#nothing-here"})))
        .unwrap_err();
    assert_eq!(err.to_string(), "Failed to render JavaScript: Message: \n");
    // http(s) URLs are fetched with the HTTP client.
    let mut driver = StaticWebDriver::new(Arc::new(FakeFetcher));
    driver.get("https://example.com/").unwrap();
    assert_eq!(
        driver.page_source().unwrap(),
        "<html><head><title>https://example.com/</title></head><body><p id=\"x\">remote</p></body></html>"
    );
    assert!(driver.has_css_element("p#x").unwrap());
    // WebParser.render_javascript goes through the renderer.
    let wp = WebParser::new(None, opts(json!({"js": {"use_selenium": false}})));
    assert_eq!(wp.render_javascript("<b>x</b>", &Options::new()).unwrap(), "<b>x</b>");
}
