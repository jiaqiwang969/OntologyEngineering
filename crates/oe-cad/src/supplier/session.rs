//! Exact normal-page account checks. Credentials and account values never enter results.
use anyhow::{Context, Result, ensure};
use oe_core::{FileIdentity, no_symlinks};
use serde_json::{Map, Value, json};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub navigate_account: bool,
    pub max_seconds: u64,
}
impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            navigate_account: false,
            max_seconds: 40,
        }
    }
}

pub fn unix_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

/// Observe only normal account-page fields, optionally navigating once to its
/// fixed public URL. Never authenticate, submit a form or read browser storage.
pub fn check(
    root: &oe_core::SkillRoot,
    browser: &mut dyn oe_browser::Browser,
    identity_path: &Path,
    output: &Path,
    options: &SessionOptions,
) -> Result<Value> {
    ensure!(
        (1..=120).contains(&options.max_seconds),
        "session_check_budget_out_of_range"
    );
    let source = super::source::identity(root)?;
    let private_input = oe_core::private_file::read(identity_path, 65536)?;
    let identity = oe_core::json::strict_json(&private_input.bytes)?;
    validate_identity(&identity)?;
    let input = private_input.identity.clone();
    let bound_session = browser.identity().clone();
    oe_browser::backend::ensure_current_policy(bound_session.backend)?;
    ensure!(
        bound_session.origin == ORIGIN,
        "misumi_account_origin_required"
    );
    let directory = root.new_private_dir(output)?;
    let started = std::time::Instant::now();
    oe_core::write_new(
        &directory.join("intent.json"),
        &json!({"schema":"cad-agent.misumi-session-intent/v2",
        "session":bound_session,"source_identity":source,"identity_input":input,"navigate_account":options.navigate_account,
        "navigation_url":if options.navigate_account {Some(ACCOUNT_URL)} else {None},"automatic_login":false}),
    )?;
    let mut result = json!({"schema":"cad-agent.misumi-session-check/v2","status":"account_state_unresolved",
        "session":bound_session,"source_identity":source,"identity_input":input,"account_verified":false,
        "password_reauthentication_verified":false,"credential_values_recorded":false,"browser_storage_accessed":false,
        "scope":"Exact supplied fields on the normal account page at the recorded time; not password reauthentication or a future login guarantee."});
    let outcome = (|| -> Result<()> {
        if options.navigate_account {
            super::source::ensure_current(root, &source)?;
            let receipt = browser.navigate_account()?;
            oe_core::write_new(
                &directory.join("navigation-receipt.json"),
                &serde_json::to_value(&receipt)?,
            )?;
            result["navigation"] = serde_json::to_value(&receipt)?;
            if receipt.state != oe_browser::DispatchState::Confirmed {
                result["status"] = "account_navigation_unconfirmed".into();
                return Ok(());
            }
        }
        let mut index = 0_u64;
        loop {
            ensure!(
                *browser.identity() == bound_session,
                "account_session_identity_changed"
            );
            let ui = browser.account_observation()?;
            let classified = classify(&ui, &identity)?;
            index += 1;
            // Account values exist only in memory. Hash and field match results
            // preserve identity/provenance without persisting the supplied values.
            let safe = json!({"observation_sha256":oe_core::json::canonical_digest(&ui)?,"classification":classified});
            oe_core::write_new(
                &directory.join(format!("observation-{index:04}.json")),
                &safe,
            )?;
            result["observation_sha256"] = safe["observation_sha256"].clone();
            for (key, value) in classified
                .as_object()
                .context("account_classification_invalid")?
            {
                result[key] = value.clone();
            }
            if classified["status"] != "account_state_unresolved"
                || started.elapsed().as_secs() >= options.max_seconds
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        ensure!(
            *browser.identity() == bound_session,
            "account_session_identity_changed"
        );
        private_input.recheck(identity_path, 65536)?;
        super::source::ensure_current(root, &source)?;
        Ok(())
    })();
    if let Err(error) = outcome {
        result["status"] = "account_check_unconfirmed".into();
        result["account_verified"] = false.into();
        result["error_code"] = super::source::error_code(&error).into();
    }
    result["checked_at_unix_ms"] = unix_ms()?.into();
    result["elapsed_seconds"] = started.elapsed().as_secs_f64().into();
    oe_core::write_new(&directory.join("result.json"), &result)?;
    Ok(result)
}

pub fn validate_receipt(
    root: &oe_core::SkillRoot,
    path: &Path,
    session: &oe_browser::SessionIdentity,
    max_age_seconds: u64,
) -> Result<Value> {
    oe_browser::backend::ensure_current_policy(session.backend)?;
    ensure!(
        (1..=3600).contains(&max_age_seconds),
        "account_receipt_max_age_out_of_range"
    );
    let path = root.private_path(path)?;
    no_symlinks(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            std::fs::symlink_metadata(&path)?.permissions().mode() & 0o077 == 0,
            "account_receipt_private_permissions_required"
        );
    }
    let private_input = oe_core::private_file::read(&path, 2 * 1024 * 1024)?;
    let before = private_input.identity.clone();
    let value = oe_core::json::strict_json(&private_input.bytes)?;
    ensure!(
        value["schema"] == "cad-agent.misumi-session-check/v2"
            && value["account_verified"] == true
            && value["status"] == "session_ready",
        "verified_account_receipt_required"
    );
    ensure!(
        value["session"] == serde_json::to_value(session)?,
        "account_receipt_session_mismatch"
    );
    let checked = value["checked_at_unix_ms"]
        .as_u64()
        .context("account_receipt_time_required")?;
    let age = unix_ms()?
        .checked_sub(checked)
        .context("account_receipt_future_time")?;
    ensure!(age <= max_age_seconds * 1000, "account_receipt_expired");
    super::source::ensure_current(root, &value["source_identity"])?;
    private_input.recheck(&path, 2 * 1024 * 1024)?;
    Ok(
        json!({"receipt":before,"checked_at_unix_ms":checked,"age_ms":age,"session":session,
        "status":"prior_normal_page_check_bound","password_reauthentication_verified":false,
        "scope":"A private caller-provided receipt bound to this browser instance and tab; not fresh account-page reobservation."}),
    )
}

#[path = "unicode_casefold.rs"]
mod unicode_casefold;

pub const ORIGIN: &str = "https://www.misumi.com.cn";
pub const ACCOUNT_URL: &str = "https://www.misumi.com.cn/my/user/manage/";
pub const IDENTITY_SCHEMA: &str = "cad-agent.misumi-account-identity/v1";

pub fn validate_identity(identity: &Value) -> Result<&Map<String, Value>> {
    ensure!(
        identity.is_object() && identity["schema"] == IDENTITY_SCHEMA,
        "identity_schema_required"
    );
    ensure!(
        identity.as_object().is_some_and(|o| o.len() == 2),
        "identity_unknown_field"
    );
    let fields = identity["match_fields"]
        .as_object()
        .context("exact_account_match_fields_required")?;
    ensure!(
        !fields.is_empty()
            && fields.len() <= 2
            && fields
                .keys()
                .all(|k| ["username", "email"].contains(&k.as_str())),
        "exact_account_match_fields_required"
    );
    ensure!(
        fields
            .values()
            .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()
                && s.chars().count() <= 1000
                && !s.chars().any(char::is_control))),
        "nonempty_identity_values_required"
    );
    Ok(fields)
}

/// External private input is read only; retain its identity for a post-observation check.
pub fn read_identity(path: &Path) -> Result<(Value, FileIdentity)> {
    let input = oe_core::private_file::read(path, 65536)?;
    let value = oe_core::json::strict_json(&input.bytes)?;
    validate_identity(&value)?;
    Ok((value, input.identity))
}

pub fn classify(ui: &Value, identity: &Value) -> Result<Value> {
    let expected = validate_identity(identity)?;
    ensure!(
        ui.is_object() && serde_json::to_vec(ui)?.len() <= 65536,
        "account_observation_invalid"
    );
    let mut result = json!({"status":"account_state_unresolved","account_verified":false,
        "password_reauthentication_verified":false,"credential_values_recorded":false,
        "browser_storage_accessed":false,"recovery":"observe_current_page",
        "email_comparison_unicode_version":unicode_casefold::UNICODE_VERSION});
    if ui["access_denied"] == true {
        result["status"] = "site_access_denied".into();
        result["recovery"] = "diagnose_access_path".into();
        return Ok(result);
    }
    if ui["challenge_visible"] == true {
        result["status"] = "verification_challenge".into();
        result["recovery"] = "normal_site_verification_required".into();
        return Ok(result);
    }
    let url = ui["url"]
        .as_str()
        .and_then(|s| super::source::source_url(s).ok());
    let Some(url) = url.filter(|u| u.origin().ascii_serialization() == ORIGIN) else {
        result["status"] = "unexpected_origin".into();
        result["recovery"] = "reobserve_bound_tab".into();
        return Ok(result);
    };
    if url.path().trim_end_matches('/') == "/my/user/manage" && ui["title"] == "个人信息管理"
    {
        let observed = ui["account_fields"].as_object();
        if let Some(observed) = observed.filter(|o| {
            expected.keys().all(|k| {
                o.get(k)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
            })
        }) {
            let mut matches = Map::new();
            for (key, value) in expected {
                let left = observed[key].as_str().unwrap().trim();
                let right = value.as_str().unwrap().trim();
                let matched = if key == "email" {
                    unicode_casefold::casefold(left) == unicode_casefold::casefold(right)
                } else {
                    left == right
                };
                matches.insert(key.clone(), matched.into());
            }
            let verified = matches.values().all(|v| v == true);
            result["status"] = if verified {
                "session_ready"
            } else {
                "different_account"
            }
            .into();
            result["account_verified"] = verified.into();
            result["exact_field_matches"] = matches.into();
            result["recovery"] = if verified {
                "reuse_browser_session"
            } else {
                "resolve_account_identity"
            }
            .into();
            return Ok(result);
        }
    }
    if ui["login_form_visible"] == true {
        result["status"] = "login_required".into();
        result["autofill_ready"] = (ui["autofill_ready"] == true).into();
        result["recovery"] = if ui["autofill_ready"] == true {
            "inspect_and_submit_normal_login_form"
        } else {
            "use_authorized_local_credentials_or_browser_autofill"
        }
        .into();
    }
    Ok(result)
}
