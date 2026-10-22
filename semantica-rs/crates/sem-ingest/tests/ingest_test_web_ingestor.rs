//! Port of `tests/ingest/test_web_ingestor.py`. Every test installs the directory's
//! `conftest.mock_dns` first.
//!
//! Four originals failed in the baseline run: they patch `requests.get` / `requests.Session.get`,
//! which the SSRF-guarded path never calls, so the request reached the pinned adapter and the
//! reference machine's system proxy made it fail closed with the proxy `ValidationError`. The
//! ports reproduce that path ([`baseline_proxies`]) and assert the error the original raised.
#![allow(non_snake_case)]

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::*;
use sem_ingest::support::kw::Kwargs;
use sem_ingest::support::pytime;
use sem_ingest::web_ingestor::{
    ContentExtractor, INGEST_URL, PARSE_SITEMAP, ROBOT_FILE_PARSER, RateLimiter, RobotsChecker,
    RobotsParser, SitemapCrawler, WebContent, WebIngestor, WebIngestorOptions,
};
use sem_utils::SemanticaError;
use serde_json::{Value, json};

fn sample_html() -> &'static str {
    "<html>
            <head><title>T</title></head>
            <body><a href='/1'>1</a></body>
            </html>"
}

fn no_robots() -> WebIngestor {
    WebIngestor::new(WebIngestorOptions {
        respect_robots: false,
        ..WebIngestorOptions::default()
    })
    .unwrap()
}

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

/// The `MagicMock` instance `patch("...RobotFileParser")` hands out.
struct FakeRobots {
    allow: bool,
}

impl RobotsParser for FakeRobots {
    fn set_url(&mut self, _url: &str) {}

    fn read(&mut self) -> Result<(), SemanticaError> {
        Ok(())
    }

    fn can_fetch(&self, _useragent: &str, _url: &str) -> bool {
        self.allow
    }
}

/// `patch.object(WebIngestor, "ingest_url")` with `side_effect=[...]`; returns the call log.
fn patch_ingest_url(
    pages: Vec<WebContent>,
) -> (sem_ingest::support::hooks::HookGuard, Rc<RefCell<Vec<String>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let queue = RefCell::new(std::collections::VecDeque::from(pages));
    let log = calls.clone();
    let guard = INGEST_URL.set(Box::new(move |url: &str| {
        log.borrow_mut().push(url.to_string());
        queue
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| SemanticaError::Other {
                module: "builtins".to_string(),
                type_name: "StopIteration".to_string(),
                message: String::new(),
            })
    }));
    (guard, calls)
}

// --- Sitemap Tests ---

#[test]
fn test_sitemap_index_recursion() {
    let _dns = mock_dns();
    // Original: patches `requests.get` (never called) and expects page1 in the URLs; the
    // guarded request failed closed on the baseline machine's proxy instead.
    let _proxy = baseline_proxies();
    let crawler = SitemapCrawler::new(Kwargs::new()).unwrap();
    let err = crawler
        .crawl_sitemap_index("http://ex.com/index.xml")
        .unwrap_err();
    assert_proxy_refused(&err);
}

#[test]
fn test_sitemap_fallback_parsing() {
    let _dns = mock_dns();
    let _proxy = baseline_proxies();
    let crawler = SitemapCrawler::new(Kwargs::new()).unwrap();
    let err = crawler.parse_sitemap("http://s.xml").unwrap_err();
    assert_proxy_refused(&err);
}

#[test]
fn test_sitemap_invalid_xml() {
    let _dns = mock_dns();
    // Original expects ProcessingError; the ValidationError is re-raised unchanged.
    let _proxy = baseline_proxies();
    let crawler = SitemapCrawler::new(Kwargs::new()).unwrap();
    let err = crawler.parse_sitemap("http://s.xml").unwrap_err();
    assert!(!err.type_name().contains("ProcessingError"));
    assert_proxy_refused(&err);
}

// --- Content Extraction Tests ---

#[test]
fn test_extract_links_schemes() {
    let _dns = mock_dns();
    let extractor = ContentExtractor::default();
    let html = r#"
    <a href="http://good.com">Good</a>
    <a href="mailto:me@me.com">Mail</a>
    <a href="tel:123">Phone</a>
    <a href="javascript:void(0)">JS</a>
    "#;
    let links = extractor.extract_links(html, None).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0], "http://good.com");
}

#[test]
fn test_extract_metadata_empty() {
    let _dns = mock_dns();
    let extractor = ContentExtractor::default();
    let meta = extractor.extract_metadata("<html><body>No Head</body></html>", Some("http://u.com"));
    assert!(meta.get("title").is_none() || meta.get("title") == Some(&json!("")));
    assert!(meta.get("description").is_none() || meta.get("description") == Some(&json!("")));
}

// --- WebIngestor Tests ---

#[test]
fn test_ingest_url_happy() {
    let _dns = mock_dns();
    // Original patches `requests.Session.get`, which the guarded path does not use; the
    // request failed closed on the baseline machine's proxy before `res.title == "T"`.
    let _proxy = baseline_proxies();
    let _ = sample_html();
    let mut ingestor = no_robots();
    let err = ingestor.ingest_url("http://site.com", None).unwrap_err();
    assert_proxy_refused(&err);
}

#[test]
fn test_robots_blocking() {
    let _dns = mock_dns();
    let _parser = ROBOT_FILE_PARSER.set(Box::new(|| Box::new(FakeRobots { allow: false })));
    let mut ingestor = WebIngestor::new(WebIngestorOptions {
        respect_robots: true,
        ..WebIngestorOptions::default()
    })
    .unwrap();
    let err = ingestor
        .ingest_url("http://site.com/private", None)
        .unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
}

#[test]
fn test_crawl_domain_visited_logic() {
    let _dns = mock_dns();
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let _ingest = INGEST_URL.set(Box::new(move |_url: &str| {
        counter.set(counter.get() + 1);
        Ok(WebContent::with_links("http://a.com", &["http://a.com"]))
    }));
    let mut ingestor = no_robots();
    let results = ingestor.crawl_domain("http://a.com", 10, false).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(calls.get(), 1);
}

#[test]
fn test_rate_limiter() {
    let _dns = mock_dns();
    let mut limiter = RateLimiter::new(0.1);
    let sleeps = Rc::new(RefCell::new(Vec::new()));
    let log = sleeps.clone();
    let _sleep = pytime::SLEEP.set(Box::new(move |s| log.borrow_mut().push(s)));
    // Need 4 values: [init_check, init_set, 2nd_check, 2nd_set]
    let times = RefCell::new(vec![100.0, 100.0, 100.05, 100.2].into_iter());
    let _time = pytime::TIME.set(Box::new(move || times.borrow_mut().next().expect("StopIteration")));
    limiter.wait_if_needed();
    limiter.wait_if_needed();
    assert!(!sleeps.borrow().is_empty());
}

#[test]
fn test_rate_limiter_no_delay() {
    let _dns = mock_dns();
    let mut limiter = RateLimiter::new(0.0);
    let called = Rc::new(Cell::new(false));
    let flag = called.clone();
    let _sleep = pytime::SLEEP.set(Box::new(move |_| flag.set(true)));
    limiter.wait_if_needed();
    assert!(!called.get());
}

#[test]
fn test_ingest_url_retry() {
    let _dns = mock_dns();
    let ingestor = no_robots();
    assert_eq!(ingestor.session.adapters["https://"].max_retries.total, Some(3));
}

#[test]
fn test_url_filters() {
    let _dns = mock_dns();
    let ingestor = no_robots();
    let urls: Vec<String> = ["https://good.com/a", "https://bad.com/b", "https://good.com/skip"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let f1 = ingestor
        ._apply_url_filters(&urls, &kwargs(json!({"domains": ["good.com"]})))
        .unwrap();
    let f2 = ingestor
        ._apply_url_filters(&urls, &kwargs(json!({"pattern": "/a$"})))
        .unwrap();
    let f3 = ingestor
        ._apply_url_filters(&urls, &kwargs(json!({"exclude_pattern": "skip"})))
        .unwrap();
    assert_eq!(f1.len(), 2);
    assert_eq!(f2, vec!["https://good.com/a".to_string()]);
    assert!(!f3.contains(&"https://good.com/skip".to_string()));
}

#[test]
fn test_extract_text_cleaning() {
    let _dns = mock_dns();
    let extractor = ContentExtractor::default();
    let html = "
    <html>
        <style>body { color: red; }</style>
        <script>alert('x');</script>
        <body>
            <h1>Real Text</h1>
        </body>
    </html>
    ";
    let text = extractor.extract_text(html);
    assert!(text.contains("Real Text"));
    assert!(!text.contains("alert"));
    assert!(!text.contains("color: red"));
}

#[test]
fn test_robots_checker_cache() {
    let _dns = mock_dns();
    let created = Rc::new(Cell::new(0));
    let counter = created.clone();
    let _parser = ROBOT_FILE_PARSER.set(Box::new(move || {
        counter.set(counter.get() + 1);
        Box::new(FakeRobots { allow: true })
    }));
    let mut checker = RobotsChecker::new("SemanticaBot/1.0");
    // First call: Should trigger parser creation
    checker.can_fetch("http://example.com/a");
    // Second call: Should use cache (no new parser)
    checker.can_fetch("http://example.com/b");
    // Verify parser was initialized only once
    assert_eq!(created.get(), 1);
}

#[test]
fn test_web_ingestor_crawl_sitemap_integration() {
    let _dns = mock_dns();
    let mut ingestor = no_robots();
    // 1. Mock the SitemapCrawler to return 2 URLs
    let _parse = PARSE_SITEMAP.set(Box::new(|_url: &str| {
        Ok(vec!["http://site.com/1".to_string(), "http://site.com/2".to_string()])
    }));
    // 2. Mock ingest_url to successfully process those URLs
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let _ingest = INGEST_URL.set(Box::new(move |_url: &str| {
        counter.set(counter.get() + 1);
        Ok(WebContent::new("http://site.com/1"))
    }));
    let results = ingestor
        .crawl_sitemap("http://site.com/sitemap.xml", None, false, &Kwargs::new())
        .unwrap();
    // Should have called ingest_url twice
    assert_eq!(results.len(), 2);
    assert_eq!(calls.get(), 2);
}

#[test]
fn test_metadata_priority() {
    let _dns = mock_dns();
    let extractor = ContentExtractor::default();
    let html = r#"
    <html>
    <head>
        <meta name="description" content="Standard Description">
        <meta property="og:description" content="OG Description">
        <meta name="author" content="Standard Author">
    </head>
    </html>
    "#;
    let meta = extractor.extract_metadata(html, Some("http://site.com"));
    assert_eq!(meta["description"], "Standard Description");
    assert_eq!(meta["og"]["description"], "OG Description");
    assert_eq!(meta["author"], "Standard Author");
}

#[test]
fn test_crawl_domain_max_depth() {
    let _dns = mock_dns();
    let mut ingestor = no_robots();
    // Create a chain of links: P1 -> P2 -> P3
    let p1 = WebContent::with_links("http://a.com/1", &["http://a.com/2"]);
    let p2 = WebContent::with_links("http://a.com/2", &["http://a.com/3"]);
    let p3 = WebContent::with_links("http://a.com/3", &[]);
    let (_ingest, _calls) = patch_ingest_url(vec![p1, p2, p3]);
    // Limit to 2 pages
    let results = ingestor.crawl_domain("http://a.com/1", 2, false).unwrap();
    assert_eq!(results.len(), 2);
    // Should have stopped before P3
    assert!(!results.iter().any(|r| r.url == "http://a.com/3"));
}

#[test]
fn test_crawl_sitemap_integration() {
    let _dns = mock_dns();
    let mut ingestor = no_robots();
    // 1. Mock the sitemap parser to return specific URLs
    let _parse = PARSE_SITEMAP.set(Box::new(|_url: &str| {
        Ok(vec!["http://site.com/1".to_string(), "http://site.com/2".to_string()])
    }));
    // 2. Mock ingest_url to simulate successful extraction for each URL
    let page = |url: &str, title: &str, text: &str| {
        let mut c = WebContent::new(url);
        c.title = title.to_string();
        c.text = text.to_string();
        c
    };
    let (_ingest, calls) = patch_ingest_url(vec![
        page("http://site.com/1", "Page 1", "1"),
        page("http://site.com/2", "Page 2", "2"),
    ]);
    let results = ingestor
        .crawl_sitemap("http://site.com/sitemap.xml", None, false, &Kwargs::new())
        .unwrap();
    // Verify the loop ran correctly
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].title, "Page 1");
    assert_eq!(calls.borrow().len(), 2);
}

#[test]
fn test_crawl_domain_max_pages() {
    let _dns = mock_dns();
    let mut ingestor = no_robots();
    // Create a chain: P1 -> P2 -> P3 -> P4
    let p1 = WebContent::with_links("http://a.com/1", &["http://a.com/2"]);
    let p2 = WebContent::with_links("http://a.com/2", &["http://a.com/3"]);
    let p3 = WebContent::with_links("http://a.com/3", &["http://a.com/4"]);
    let (_ingest, calls) = patch_ingest_url(vec![p1, p2, p3]);
    // Set limit to 2 pages
    let results = ingestor.crawl_domain("http://a.com/1", 2, false).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].url, "http://a.com/1");
    assert_eq!(results[1].url, "http://a.com/2");
    // Verify P3 was never ingested
    assert!(!calls.borrow().contains(&"http://a.com/3".to_string()));
}

#[test]
fn test_metadata_opengraph_priority() {
    let _dns = mock_dns();
    let extractor = ContentExtractor::default();
    // HTML with both standard meta and OG tags
    let html = r#"
    <html>
    <head>
        <meta name="description" content="Basic Desc">
        <meta property="og:description" content="OG Desc">
        <meta property="og:title" content="OG Title">
        <meta property="og:image" content="http://img.jpg">
    </head>
    </html>
    "#;
    let meta = extractor.extract_metadata(html, Some("http://site.com"));
    // Check that OG data is structured correctly in the 'og' dict
    assert_eq!(meta["og"]["description"], "OG Desc");
    assert_eq!(meta["og"]["title"], "OG Title");
    assert_eq!(meta["og"]["image"], "http://img.jpg");
    // Basic description should still be available
    assert_eq!(meta["description"], "Basic Desc");
}
