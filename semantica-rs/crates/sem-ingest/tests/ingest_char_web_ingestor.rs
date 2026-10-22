//! Characterisation tests for `semantica/ingest/web_ingestor.py` (expected values from the
//! original run in the oracle environment with `requests.Session.request` and DNS patched).

mod common;

use common::*;
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use sem_ingest::web_ingestor::{SitemapCrawler, WebIngestor, WebIngestorOptions};
use serde_json::json;

#[test]
fn extract_content_builds_web_content() {
    let _dns = mock_dns();
    let html = r#"<html><head><title> Hello </title><meta name="Description" content="D"><meta property="og:title" content="OG"></head><body><p>Para one.</p>  <a href="/x">X</a><a href="https://other.org/y">Y</a><a href="mailto:a@b">M</a><script>var a=1;</script></body></html>"#;
    let ingestor = WebIngestor::new(WebIngestorOptions {
        respect_robots: false,
        ..WebIngestorOptions::default()
    })
    .unwrap();
    let c = ingestor
        .extract_content(html, Some("https://example.com/dir/page"))
        .unwrap();
    assert_eq!(c.title, "Hello");
    assert_eq!(c.text, "Hello Para one. XYM");
    assert_eq!(c.links, vec!["https://example.com/x", "https://other.org/y"]);
    assert_eq!(
        serde_json::Value::Object(c.metadata),
        json!({"url": "https://example.com/dir/page", "title": "Hello", "description": "D", "og:title": "OG", "og": {"title": "OG"}})
    );
    assert_eq!(c.url, "https://example.com/dir/page");
    assert_eq!(c.status_code, None);
}

#[test]
fn crawl_sitemap_index_follows_child_sitemaps() {
    let _dns = mock_dns();
    let index_xml = r#"<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><sitemap><loc> http://ex.com/s1.xml </loc></sitemap><sitemap><loc>http://ex.com/s2.xml</loc></sitemap></sitemapindex>"#;
    let child1 = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>http://ex.com/page1</loc></url><url><loc>http://ex.com/page2</loc></url></urlset>"#;
    let child2 = r#"<urlset><url><loc>http://ex.com/page3</loc></url></urlset>"#;
    let fake = FakeTransport::sequence(
        [index_xml, child1, child2]
            .iter()
            .map(|x| Response::new(200).with_content(x.as_bytes().to_vec()))
            .collect(),
    );
    let _session = patch_session_request(&fake);
    let urls = SitemapCrawler::new(Kwargs::new())
        .unwrap()
        .crawl_sitemap_index("http://ex.com/index.xml")
        .unwrap();
    assert_eq!(urls, vec!["http://ex.com/page1", "http://ex.com/page2", "http://ex.com/page3"]);
    let calls: Vec<(String, String)> = fake.calls().into_iter().map(|c| (c.method, c.url)).collect();
    assert_eq!(
        calls,
        vec![
            ("GET".to_string(), "http://ex.com/index.xml".to_string()),
            ("GET".to_string(), "http://ex.com/s1.xml".to_string()),
            ("GET".to_string(), "http://ex.com/s2.xml".to_string()),
        ]
    );
}

#[test]
fn ingest_url_fetches_and_extracts() {
    let _dns = mock_dns();
    let page = r#"<html><head><title>Home</title></head><body><a href="/about">About</a></body></html>"#;
    let fake = FakeTransport::returning(Response::new(200).with_text(page));
    let _session = patch_session_request(&fake);
    let mut ingestor = WebIngestor::new(WebIngestorOptions {
        respect_robots: false,
        delay: 0.0,
        ..WebIngestorOptions::default()
    })
    .unwrap();
    let r = ingestor.ingest_url("https://example.com/", None).unwrap();
    assert_eq!(r.url, "https://example.com/");
    assert_eq!(r.title, "Home");
    assert_eq!(r.text, "HomeAbout");
    assert_eq!(r.links, vec!["https://example.com/about"]);
    assert_eq!(r.status_code, Some(200));
    assert_eq!(
        serde_json::Value::Object(r.metadata),
        json!({"url": "https://example.com/", "title": "Home"})
    );
    let call = fake.last_call();
    assert_eq!((call.method.as_str(), call.url.as_str()), ("GET", "https://example.com/"));
    assert_eq!(call.options.timeout, Some(30.0));
    assert_eq!(call.options.allow_redirects, Some(false));
}
