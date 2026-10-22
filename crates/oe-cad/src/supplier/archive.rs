//! Bounded downloads of observed public supplier URLs. No CAD generation or login.
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, create_new_private_dir, ensure_directory, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Component, Path};
use std::time::{Duration, Instant, SystemTime};

pub const STATIC_LIMIT: usize = 10 * 1024 * 1024;
const EXPANDED_LIMIT: usize = 100 * 1024 * 1024;

fn decode_path(value: &str) -> Result<String> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let pair = bytes
                .get(at + 1..at + 3)
                .context("invalid_supplier_URL_encoding")?;
            let h = (pair[0] as char)
                .to_digit(16)
                .context("invalid_supplier_URL_encoding")?;
            let l = (pair[1] as char)
                .to_digit(16)
                .context("invalid_supplier_URL_encoding")?;
            out.push((h * 16 + l) as u8);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).context("invalid_supplier_URL_encoding")
}

fn exact_url(url: &str, host: &str) -> Result<(reqwest::Url, String)> {
    let prefix = format!("https://{host}/");
    ensure!(
        url.starts_with(&prefix) && url.len() <= 16384 && !url.chars().any(char::is_control),
        "observed_public_supplier_URL_required"
    );
    let parsed = reqwest::Url::parse(url).context("invalid_supplier_URL")?;
    ensure!(
        parsed.scheme() == "https"
            && parsed.host_str() == Some(host)
            && parsed.port().is_none()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "observed_public_supplier_URL_required"
    );
    let raw_path = &url[format!("https://{host}").len()..];
    let path = decode_path(raw_path)?;
    ensure!(
        !path.contains('\\')
            && !path.chars().any(char::is_control)
            && !path.split('/').any(|c| matches!(c, ".." | ".")),
        "unsafe_supplier_URL_path"
    );
    Ok((parsed, path))
}

pub fn validate_static_url(url: &str) -> Result<reqwest::Url> {
    let (parsed, path) = exact_url(url, "www.misumi.com.cn")?;
    ensure!(
        path.starts_with("/linked/material/") && path.to_ascii_lowercase().ends_with(".zip"),
        "observed_static_2d_URL_required"
    );
    Ok(parsed)
}

pub fn validate_ap203_url(url: &str, part: &str) -> Result<reqwest::Url> {
    super::artifacts::validate_part(part)?;
    let (parsed, path) = exact_url(url, "rd-cn-caddata.misumi.com.cn")?;
    ensure!(
        path.starts_with("/prod/CadSupplyGC/")
            && path.ends_with(&format!("/STEP_AP203/{part}_STEP.zip")),
        "observed_supplier_file_URL_not_supported"
    );
    Ok(parsed)
}

fn member_path(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 1024
            && !name.contains(['\\', ':'])
            && !name.chars().any(char::is_control),
        "unsafe_archive_member"
    );
    ensure!(
        Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_))),
        "unsafe_archive_member"
    );
    for component in name.split('/') {
        ensure!(
            !component.is_empty() && !component.ends_with(['.', ' ']),
            "unsafe_archive_member"
        );
        let base = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        ensure!(
            ![
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9"
            ]
            .contains(&base.as_str()),
            "unsafe_archive_member"
        );
    }
    Ok(())
}

pub fn inspect_static_archive(blob: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    ensure!(
        !blob.is_empty() && blob.len() <= STATIC_LIMIT,
        "static_archive_size_limit"
    );
    let mut zip = zip::ZipArchive::new(Cursor::new(blob)).context("static_archive_invalid")?;
    ensure!(
        !zip.is_empty() && zip.len() <= 10_000,
        "static_archive_member_limit"
    );
    let mut names = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut declared = 0_u64;
    let mut actual = 0_usize;
    for index in 0..zip.len() {
        let mut member = zip
            .by_index(index)
            .context("static_archive_member_invalid")?;
        let name = member.name().to_owned();
        let checked = name.strip_suffix('/').unwrap_or(&name);
        member_path(checked)?;
        ensure!(
            names.insert(checked.to_lowercase()),
            "static_archive_duplicate_path"
        );
        let mode = member.unix_mode().unwrap_or(0) & 0o170000;
        ensure!(
            mode == 0 || mode == 0o100000 || (mode == 0o040000 && member.is_dir()),
            "static_archive_unsafe_member_type"
        );
        declared = declared
            .checked_add(member.size())
            .context("static_archive_size_overflow")?;
        ensure!(
            declared <= EXPANDED_LIMIT as u64,
            "static_archive_expansion_limit"
        );
        if member.is_dir() {
            continue;
        }
        let suffix = Path::new(&name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            ["dxf", "dwg", "pdf", "txt"].contains(&suffix.as_str()),
            "not_supported_static_2d_package"
        );
        let mut bytes = Vec::new();
        (&mut member)
            .take((EXPANDED_LIMIT - actual) as u64 + 1)
            .read_to_end(&mut bytes)
            .context("static_archive_CRC_or_read_failure")?;
        actual = actual
            .checked_add(bytes.len())
            .context("static_archive_size_overflow")?;
        ensure!(
            actual <= EXPANDED_LIMIT && bytes.len() as u64 == member.size(),
            "static_archive_expansion_limit"
        );
        if suffix != "txt" {
            let base = Path::new(&name)
                .file_name()
                .and_then(|s| s.to_str())
                .context("static_archive_filename_invalid")?;
            super::artifacts::inspect_file(
                base,
                &bytes,
                "static_archive",
                &suffix.to_ascii_uppercase(),
            )?;
        }
        files.insert(name, bytes);
    }
    ensure!(!files.is_empty(), "empty_static_archive");
    // A file must not also be an ancestor directory of another member.
    for name in files.keys() {
        let mut parent = Path::new(name).parent();
        while let Some(p) = parent {
            let s = p.to_str().context("static_archive_path_invalid")?;
            ensure!(
                !files.keys().any(|k| k.eq_ignore_ascii_case(s)),
                "static_archive_path_collision"
            );
            parent = p.parent();
        }
    }
    Ok(files)
}

fn read_http(response: reqwest::blocking::Response, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        response.status().as_u16() == 200,
        "supplier_file_HTTP_not_200"
    );
    ensure!(
        response.content_length().is_none_or(|n| n <= limit as u64),
        "supplier_download_too_large"
    );
    let mut bytes = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .context("supplier_file_transfer_unconfirmed")?;
    ensure!(
        bytes.len() <= limit && !bytes.is_empty(),
        "supplier_download_too_large_or_empty"
    );
    Ok(bytes)
}

pub fn download(
    root: &SkillRoot,
    url: &str,
    observed_from: &str,
    out: &Path,
    expected_sha256: Option<&str>,
) -> Result<Value> {
    let source = super::source::identity(root)?;
    let mut current = validate_static_url(url)?;
    ensure!(
        !observed_from.trim().is_empty() && observed_from.chars().count() <= 32000,
        "observed_source_required"
    );
    if let Some(hash) = expected_sha256 {
        ensure!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "expected_digest_invalid"
        );
    }
    let output = root.new_private_dir(out)?;
    write_new(
        &output.join("intent.json"),
        &json!({"schema":"cad-agent.supplier-public-download-intent/v1","url":url,"observed_from":observed_from,"expected_sha256":expected_sha256,"browser_actions":0,"ordering":"not_performed"}),
    )?;
    let started = Instant::now();
    let operation = (|| -> Result<Value> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(30))
            .build()
            .context("supplier_http_client_unavailable")?;
        let mut redirects = Vec::new();
        let blob = loop {
            ensure!(
                started.elapsed() < Duration::from_secs(30),
                "supplier_transfer_timeout"
            );
            let response = client
                .get(current.clone())
                .timeout(Duration::from_secs(30).saturating_sub(started.elapsed()))
                .send()
                .context("supplier_file_transfer_unconfirmed")?;
            if response.status().is_redirection() {
                ensure!(redirects.len() < 10, "supplier_redirect_limit");
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .context("supplier_redirect_location_missing")?;
                let target = current
                    .join(location)
                    .context("supplier_redirect_invalid")?;
                current = validate_static_url(target.as_str())?;
                redirects.push(current.to_string());
                continue;
            }
            break read_http(response, STATIC_LIMIT)?;
        };
        let hash = oe_core::sha256(&blob);
        write_new_bytes(&output.join("unverified-download.zip"), &blob)?;
        ensure!(
            expected_sha256.is_none_or(|expected| expected == hash),
            "supplier_expected_digest_mismatch"
        );
        let files = inspect_static_archive(&blob)?;
        write_new_bytes(&output.join("archive.zip"), &blob)?;
        let members = output.join("members");
        create_new_private_dir(&members)?;
        let mut rows = Vec::new();
        for (name, bytes) in files {
            let target = members.join(&name);
            ensure_directory(target.parent().context("supplier_member_parent_missing")?)?;
            write_new_bytes(&target, &bytes)?;
            rows.push(json!({"path":name,"bytes":bytes.len(),"sha256":oe_core::sha256(&bytes)}));
        }
        super::source::ensure_current(root, &source)?;
        Ok(
            json!({"schema":"cad-agent.misumi-public-archive/v2","status":"artifact_acquired","source_identity":source,"source_url":url,"final_url":current.as_str(),"redirects":redirects,
            "observed_from":observed_from,"retrieved_at":chrono::DateTime::<chrono::Utc>::from(SystemTime::now()).to_rfc3339(),"sha256":hash,"bytes":blob.len(),"files":rows,
            "claim":"Static 2D file intake only; exact SKU geometry, native import, engineering fitness and purchasing not verified","engineering_acceptance":"not_evaluated","semantic_authority":false}),
        )
    })();
    match operation {
        Ok(receipt) => {
            write_new(&output.join("receipt.json"), &receipt)?;
            Ok(receipt)
        }
        Err(error) => {
            let result = json!({"status":"download_unconfirmed","error_code":super::source::error_code(&error),"source_identity":source,"retained_partial_artifacts":true,"engineering_acceptance":"not_evaluated"});
            write_new(&output.join("result.json"), &result)?;
            Ok(result)
        }
    }
}

/// Historical, explicit observed AP203 transfer. Only idempotent HTTP is retried.
pub fn download_observed_ap203(
    root: &SkillRoot,
    url: &str,
    part: &str,
    out: &Path,
) -> Result<Value> {
    download_observed_ap203_with_budget(root, url, part, out, 45)
}

pub fn download_observed_ap203_with_budget(
    root: &SkillRoot,
    url: &str,
    part: &str,
    out: &Path,
    max_seconds: u64,
) -> Result<Value> {
    ensure!(
        (1..=45).contains(&max_seconds),
        "supplier_transfer_budget_out_of_range"
    );
    let source = super::source::identity(root)?;
    let endpoint = validate_ap203_url(url, part)?;
    let output = root.new_private_dir(out)?;
    write_new(
        &output.join("intent.json"),
        &json!({"source_url":url,"part":part,"scope":"historical_ap203_comparison","generation":"not_performed"}),
    )?;
    let operation = (|| -> Result<Value> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(max_seconds.min(15)))
            .build()?;
        let started = Instant::now();
        for attempt in 0..=2 {
            let remaining = Duration::from_secs(max_seconds).saturating_sub(started.elapsed());
            ensure!(!remaining.is_zero(), "supplier_transfer_timeout");
            let response = client
                .get(endpoint.clone())
                .timeout(remaining.min(Duration::from_secs(15)))
                .send();
            let transfer = response
                .map_err(anyhow::Error::from)
                .and_then(|r| read_http(r, 20_000_000));
            match transfer {
                Ok(blob) => {
                    write_new_bytes(&output.join("unverified-download.zip"), &blob)?;
                    let inspected = super::artifacts::inspect_ap203_archive(blob, part)?;
                    let mut receipt = inspected.receipt;
                    receipt["scope"] = "historical_ap203_comparison".into();
                    receipt["observed_download_url"] = url.into();
                    receipt["transfer_retries"] = attempt.into();
                    receipt["HTTP_status"] = 200.into();
                    receipt["transfer_seconds"] = started.elapsed().as_secs_f64().into();
                    receipt["transfer_budget_seconds"] = max_seconds.into();
                    receipt["source_identity"] = source.clone();
                    write_new_bytes(&output.join(format!("{part}_STEP.zip")), &inspected.blob)?;
                    for (name, bytes) in inspected.files {
                        write_new_bytes(&output.join(name), &bytes)?;
                    }
                    super::source::ensure_current(root, &source)?;
                    write_new(&output.join("receipt.json"), &receipt)?;
                    return Ok(receipt);
                }
                Err(_) => {
                    write_new(
                        &output.join(format!("attempt-{attempt}.json")),
                        &json!({"attempt":attempt,"status":"transfer_unconfirmed","generation_retried":false}),
                    )?;
                    if attempt < 2
                        && started.elapsed() + Duration::from_secs(1)
                            < Duration::from_secs(max_seconds)
                    {
                        std::thread::sleep(Duration::from_secs(1));
                    } else {
                        break;
                    }
                }
            }
        }
        anyhow::bail!("supplier_file_transfer_unconfirmed")
    })();
    match operation {
        Ok(receipt) => Ok(receipt),
        Err(error) => {
            let result = json!({"status":"download_unconfirmed","error_code":super::source::error_code(&error),"source_identity":source,"engineering_acceptance":"not_evaluated","generation_retried":false,"retained_partial_artifacts":true});
            write_new(&output.join("result.json"), &result)?;
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Real bounded HTTP parsing against only a task-owned loopback fixture;
    /// this is not a successful transfer from the public supplier service.
    fn response(status: &str, headers: &str, body: &[u8]) -> reqwest::blocking::Response {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let raw = [
            format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n").as_bytes(),
            body,
        ]
        .concat();
        let server = std::thread::spawn(move || {
            let started = Instant::now();
            loop {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut request = [0; 4096];
                        let _ = socket.read(&mut request);
                        socket.write_all(&raw).unwrap();
                        break;
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(2) =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("controlled HTTP fixture unavailable: {error}"),
                }
            }
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let response = client
            .get(format!("http://{address}/controlled-body"))
            .send()
            .unwrap();
        server.join().unwrap();
        response
    }

    #[test]
    fn native_http_reader_enforces_success_and_both_declared_and_actual_size() {
        assert_eq!(
            read_http(response("200 OK", "Content-Length: 5\r\n", b"bytes"), 5).unwrap(),
            b"bytes"
        );
        assert!(
            read_http(
                response("403 Forbidden", "Content-Length: 5\r\n", b"bytes"),
                5
            )
            .is_err()
        );
        assert!(read_http(response("200 OK", "Content-Length: 5\r\n", b"bytes"), 4).is_err());
        assert!(read_http(response("200 OK", "", b"bytes"), 4).is_err());
        assert!(read_http(response("200 OK", "Content-Length: 0\r\n", b""), 4).is_err());
    }
}
