//! Port of `tests/ingest/test_feed_ingestor.py`. Every test installs `conftest.mock_dns`
//! first.
//!
//! `test_ingest_feed_errors` failed in the baseline run: it patches `requests.request`, which
//! the SSRF-guarded path never calls, so the request reached the pinned adapter and failed
//! closed on the reference machine's proxy (a `ValidationError`, not the expected
//! `ProcessingError`). The port reproduces that path.
#![allow(non_snake_case)]

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::Ordering;

use chrono::Datelike;
use common::*;
use sem_ingest::feed_ingestor::{
    ADD_FEED, CHECK_UPDATES, FeedData, FeedIngestor, FeedItem, FeedParser, START_MONITORING,
    THREAD_START,
};
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use sem_ingest::support::pytime;
use sem_utils::SemanticaError;
use serde_json::{Value, json};

fn complex_atom_xml() -> &'static str {
    r#"
    <feed xmlns="http://www.w3.org/2005/Atom">
        <title>Atom</title>
        <link href="http://example.com" rel="self"/>
        <subtitle>Subtitle</subtitle>
        <updated>2025-01-01T00:00:00Z</updated>
        <entry>
            <title>Entry 1</title>
            <link href="http://example.com/1" rel="alternate"/>
            <summary>Summary</summary>
            <content>Full Content</content>
            <id>uuid:123</id>
            <published>2025-01-01T00:00:00Z</published>
            <updated>2025-01-02T00:00:00Z</updated>
            <category term="tech"/>
            <category term="news"/>
        </entry>
    </feed>
    "#
}

fn feed_ingestor() -> FeedIngestor {
    FeedIngestor::new(Kwargs::new()).unwrap()
}

#[test]
fn test_parse_atom_complex() {
    let _dns = mock_dns();
    let parser = FeedParser::default();
    let data = parser.parse_feed(complex_atom_xml(), None).unwrap();
    let item = &data.items[0];
    assert_eq!(data.title, "Atom");
    assert_eq!(data.items.len(), 1);
    assert_eq!(item.description, "Summary");
    assert_eq!(item.content, "Full Content");
    assert!(item.categories.contains(&"tech".to_string()));
    assert!(item.categories.contains(&"news".to_string()));
    assert_eq!(item.published.as_ref().unwrap().wall().year(), 2025);
}

#[test]
fn test_parse_rss_dates() {
    let _dns = mock_dns();
    let xml = r#"
    <rss version="2.0">
        <channel>
            <title>T</title>
            <link>http://l.com</link>
            <item>
                <title>T</title>
                <pubDate>Mon, 27 Jan 2025 12:00:00 GMT</pubDate>
            </item>
        </channel>
    </rss>
    "#;
    let parser = FeedParser::default();
    let data = parser.parse_feed(xml, None).unwrap();
    assert_eq!(data.items[0].published.as_ref().unwrap().wall().year(), 2025);
}

#[test]
fn test_ingest_feed_errors() {
    let _dns = mock_dns();
    let mut ingestor = feed_ingestor();
    assert!(ingestor.ingest_feed("not_a_url", None, true, &Kwargs::new()).is_err());

    let _resolver = dns_returning(&["93.184.216.34"]);
    // `patch("requests.request", side_effect=RequestException("Fail"))` is never reached.
    let _proxy = baseline_proxies();
    let err = ingestor
        .ingest_feed("http://valid.com", None, true, &Kwargs::new())
        .unwrap_err();
    assert_proxy_refused(&err);
}

#[test]
fn test_monitor_loop_lifecycle() {
    let _dns = mock_dns();
    let mut ingestor = feed_ingestor();
    ingestor.monitor.add_feed("http://test.com", &Kwargs::new()).unwrap();
    let monitoring = ingestor.monitor.monitoring.clone();
    let _sleep = pytime::SLEEP.set(Box::new(move |_seconds| monitoring.store(false, Ordering::SeqCst)));
    let _check = CHECK_UPDATES.set(Box::new(|_url: &str| {
        Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "Exception".to_string(),
            message: "Check Fail".to_string(),
        })
    }));
    ingestor.monitor.monitoring.store(true, Ordering::SeqCst);
    ingestor.monitor._monitoring_loop();
    assert!(!ingestor.monitor.monitoring.load(Ordering::SeqCst));
}

#[test]
fn test_monitor_threading() {
    let _dns = mock_dns();
    let mut ingestor = feed_ingestor();
    ingestor.monitor.add_feed("http://test.com", &Kwargs::new()).unwrap();
    {
        let starts = Rc::new(Cell::new(0));
        let counter = starts.clone();
        let _thread = THREAD_START.set(Box::new(move |_target: Box<dyn FnOnce() + Send>| {
            counter.set(counter.get() + 1);
            None
        }));
        ingestor.monitor.start_monitoring();
        assert_eq!(starts.get(), 1);
    }
    // Test double start and stop
    ingestor.monitor.start_monitoring();
    ingestor.monitor.stop_monitoring();
    assert!(!ingestor.monitor.monitoring.load(Ordering::SeqCst));
}

#[test]
fn test_extract_content_helper() {
    let _dns = mock_dns();
    let ingestor = feed_ingestor();
    let mut item = FeedItem::new("T", "L");
    item.description = "D".to_string();
    item.content = "C".to_string();
    item.categories = vec!["cat".to_string()];
    let res = ingestor.extract_content(&item);
    assert_eq!(res["content"], "C");
    assert_eq!(res["categories"], json!(["cat"]));
}

#[test]
fn test_extract_content_missing_fields() {
    let _dns = mock_dns();
    let ingestor = feed_ingestor();
    let mut item = FeedItem::new("T", "L");
    item.description = "D".to_string();
    let res = ingestor.extract_content(&item);
    assert_eq!(res["content"], "D");
    assert_eq!(res["published"], Value::Null);
    assert_eq!(res["updated"], Value::Null);
}

#[test]
fn test_parse_date_formats() {
    let _dns = mock_dns();
    let parser = FeedParser::default();
    let d1 = parser._parse_date("Mon, 27 Jan 2025 12:00:00 GMT").unwrap().unwrap();
    let d2 = parser._parse_date("2025-01-27T12:00:00Z").unwrap().unwrap();
    let d3 = parser._parse_date("2025-01-27").unwrap().unwrap();
    assert_eq!(d1.wall().year(), 2025);
    assert_eq!(d2.wall().year(), 2025);
    assert_eq!(d3.wall().year(), 2025);
    let err = parser._parse_date("Not a date").unwrap_err();
    assert!(err.is_value_error(), "{err:?}");
}

#[test]
fn test_validate_feed() {
    let _dns = mock_dns();
    let parser = FeedParser::default();
    let mock_item = || FeedItem::new("t", "");
    let f1 = FeedData::new("T", "http://e.com", vec![mock_item()]);
    let f2 = FeedData::new("", "http://e.com", vec![mock_item()]);
    let f3 = FeedData::new("T", "http://e.com", vec![]);
    assert!(parser.validate_feed(&f1));
    assert!(!parser.validate_feed(&f2));
    assert!(!parser.validate_feed(&f3));
}

#[test]
fn test_discover_feeds_empty() {
    let _dns = mock_dns();
    let ingestor = feed_ingestor();
    let html = "<html><body>No feeds here</body></html>";
    let _resolver = dns_returning(&["93.184.216.34"]);
    let fake = FakeTransport::handler(move |call| {
        if call.url == "http://site.com" {
            Ok(Response::new(200).with_text(html))
        } else {
            Err(request_exception("not found"))
        }
    });
    let _session = patch_session_request(&fake);
    let feeds = ingestor.discover_feeds("http://site.com");
    assert_eq!(feeds.len(), 0);
}

#[test]
fn test_discover_feeds_found() {
    let _dns = mock_dns();
    let ingestor = feed_ingestor();
    let html = r#"
    <html>
    <head>
        <link rel="alternate" type="application/rss+xml" href="/rss.xml">
    </head>
    <body>
        <a href="/feed">RSS</a>
    </body>
    </html>
    "#;
    let _resolver = dns_returning(&["93.184.216.34"]);
    let fake = FakeTransport::handler(move |_call| {
        Ok(Response::new(200)
            .with_header("Content-Type", "application/rss+xml")
            .with_text(html))
    });
    let _session = patch_session_request(&fake);
    let feeds = ingestor.discover_feeds("http://site.com");
    assert!(feeds.contains(&"http://site.com/rss.xml".to_string()));
    assert!(feeds.contains(&"http://site.com/feed".to_string()));
}

#[test]
fn test_feed_monitor_options() {
    let _dns = mock_dns();
    let mut ingestor = feed_ingestor();
    let added: Rc<RefCell<Vec<(String, Kwargs)>>> = Rc::new(RefCell::new(Vec::new()));
    let log = added.clone();
    let _add = ADD_FEED.set(Box::new(move |url: &str, options: &Kwargs| {
        log.borrow_mut().push((url.to_string(), options.clone()))
    }));
    let starts = Rc::new(Cell::new(0));
    let counter = starts.clone();
    let _start = START_MONITORING.set(Box::new(move || counter.set(counter.get() + 1)));
    let options = json!({"interval": 60, "start": true}).as_object().cloned().unwrap();
    ingestor.monitor_feeds(&["http://f.com"], &options, None).unwrap();
    let calls = added.borrow();
    let (url, called_with) = calls.last().unwrap();
    assert_eq!(url, "http://f.com");
    assert_eq!(Value::Object(called_with.clone()), json!({"interval": 60, "start": true}));
    assert_eq!(starts.get(), 1);
}
