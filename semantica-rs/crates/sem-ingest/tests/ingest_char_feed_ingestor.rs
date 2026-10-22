//! Characterisation tests for `semantica/ingest/feed_ingestor.py` (expected values from the
//! original run in the oracle environment with `requests.Session.request` and DNS patched).

mod common;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use common::*;
use sem_ingest::feed_ingestor::{CHECK_UPDATES, FeedIngestor, FeedItem, FeedMonitor, FeedParser};
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use sem_ingest::support::pytime;
use serde_json::{Value, json};

const RSS1: &str = r#"<rss version="2.0"><channel><title>News</title><link>http://n.com</link><description>D</description><language>en</language>
<item><title>A</title><link>http://n.com/a</link><guid>g1</guid><description>da</description><author>me</author><category>c1</category><pubDate>Mon, 27 Jan 2025 12:00:00 GMT</pubDate></item>
</channel></rss>"#;

fn rss2() -> String {
    RSS1.replace(
        "</channel>",
        "<item><title>B</title><link>http://n.com/b</link><guid>g2</guid></item></channel>",
    )
}

#[test]
fn detect_format_and_parse_errors() {
    let p = FeedParser::default();
    assert_eq!(p.detect_format("<rss version='2.0'><channel/></rss>"), "rss");
    assert_eq!(p.detect_format(r#"<feed xmlns="http://www.w3.org/2005/Atom"></feed>"#), "atom");
    assert_eq!(p.detect_format("<html/>"), "rss");
    assert_eq!(p.detect_format("not xml"), "rss");
    assert!(p.validate_feed(&p.parse_feed(RSS1, None).unwrap()));
    let err = p.parse_feed("<html/>", None).unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
    assert_eq!(err.to_string(), "Failed to parse feed: Invalid RSS feed: no channel element found");
    let err = p.parse_feed("not xml <", None).unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
    assert_eq!(err.to_string(), "Failed to parse feed: syntax error: line 1, column 0");
}

#[test]
fn ingest_feed_extract_content_and_check_updates() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![
        Response::new(200).with_text(RSS1),
        Response::new(200).with_text(rss2()),
        Response::new(200).with_text(rss2()),
    ]);
    let _session = patch_session_request(&fake);
    let mut ing = FeedIngestor::new(Kwargs::new()).unwrap();
    let d = ing
        .ingest_feed("http://n.com/feed.xml", None, true, &Kwargs::new())
        .unwrap();
    assert_eq!(
        (d.title.as_str(), d.link.as_str(), d.description.as_str(), d.language.as_str()),
        ("News", "http://n.com", "D", "en")
    );
    assert_eq!(d.updated, None);
    assert_eq!(d.items.len(), 1);
    assert!(d.metadata.is_empty());
    let it = &d.items[0];
    assert_eq!((it.title.as_str(), it.link.as_str(), it.description.as_str()), ("A", "http://n.com/a", "da"));
    assert_eq!(it.content, "");
    assert_eq!(it.author, "");
    assert_eq!(it.published.as_ref().unwrap().isoformat('T'), "2025-01-27T12:00:00");
    assert_eq!(it.updated, None);
    assert_eq!(it.guid, "g1");
    assert_eq!(it.categories, vec!["c1"]);
    assert_eq!(
        Value::Object(ing.extract_content(it)),
        json!({"title": "A", "description": "da", "content": "da", "link": "http://n.com/a", "author": "", "published": "2025-01-27T12:00:00", "updated": null, "categories": ["c1"], "guid": "g1", "metadata": {}})
    );

    let m = &mut ing.monitor;
    m.add_feed("http://n.com/feed.xml", &Kwargs::new()).unwrap();
    let first = m.check_updates("http://n.com/feed.xml").unwrap();
    let second = m.check_updates("http://n.com/feed.xml").unwrap();
    assert_eq!(first.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
    assert!(second.is_empty());
    let err = m.check_updates("http://other").unwrap_err();
    assert!(err.is_validation_error());
    assert_eq!(err.to_string(), "Feed http://other not in monitoring list");
}

#[test]
fn monitor_loop_calls_update_callback() {
    let mut m = FeedMonitor::new(json!({"check_interval": 5}).as_object().cloned().unwrap()).unwrap();
    m.add_feed("http://a.com/feed", &Kwargs::new()).unwrap();
    m.add_feed("http://b.com/feed", &Kwargs::new()).unwrap();
    let seen: Arc<Mutex<Vec<(String, Vec<String>)>>> = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    m.set_update_callback(move |url, items| {
        log.lock()
            .unwrap()
            .push((url.to_string(), items.iter().map(|i| i.title.clone()).collect()))
    });
    let _check = CHECK_UPDATES.set(Box::new(|url: &str| {
        Ok(if url.starts_with("http://a") {
            vec![FeedItem::new("x", url)]
        } else {
            vec![]
        })
    }));
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let sleep_log = sleeps.clone();
    let monitoring = m.monitoring.clone();
    let _sleep = pytime::SLEEP.set(Box::new(move |s| {
        sleep_log.lock().unwrap().push(s);
        monitoring.store(false, Ordering::SeqCst);
    }));
    m.monitoring.store(true, Ordering::SeqCst);
    m._monitoring_loop();
    assert_eq!(
        *seen.lock().unwrap(),
        vec![("http://a.com/feed".to_string(), vec!["x".to_string()])]
    );
    assert_eq!(*sleeps.lock().unwrap(), vec![5.0]);
    assert_eq!(m.check_interval, 5.0);

    let err = m.add_feed("", &Kwargs::new()).unwrap_err();
    assert!(err.is_validation_error());
    assert_eq!(err.to_string(), "Feed URL is required");

    let mut m2 = FeedMonitor::new(Kwargs::new()).unwrap();
    m2.start_monitoring();
    assert!(!m2.monitoring.load(Ordering::SeqCst));
    assert_eq!(m2.check_interval, 3600.0);
    m2.stop_monitoring();
    assert!(!m2.monitoring.load(Ordering::SeqCst));
}
