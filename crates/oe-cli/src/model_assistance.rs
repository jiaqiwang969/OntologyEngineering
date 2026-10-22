//! Explicit direct-model entry. The Decisions judgment interface and its budget remain separate.
use anyhow::Result;
use clap::{Args, Subcommand};
use oe_core::{SkillRoot, write_new};
use oe_engineering::model_assistance::{self, transport::HttpModelTransport};
use serde_json::json;
use std::{path::PathBuf, time::Duration};

#[derive(Args)]
pub struct Inputs {
    /// Agent-maintained current Project and complete historical input bindings.
    #[arg(long)]
    input: PathBuf,
    /// Explicit private service profile; no ambient Codex authentication.
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
}

#[derive(Subcommand)]
pub enum ModelAssistanceCommand {
    /// Inspect current identity, complete input and independent service budget.
    Prepare(Inputs),
    /// Inspect explicitly bound selected PDF pages without model dispatch.
    PreparePages(Inputs),
    /// Actual registered6 OCR preparation, native/OCR separately marked; no model.
    PrepareOcr(Inputs),
    /// Recreate real OCR capability and reuse the existing model dispatcher.
    RunOcr {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(long)]
        live: bool,
    },
    /// Rebuild from sources. Live performs at most one direct HTTP dispatch.
    Run {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(long)]
        live: bool,
    },
    /// Selected-page version through the same single direct-model dispatcher.
    RunPages {
        #[command(flatten)]
        inputs: Inputs,
        #[arg(long)]
        live: bool,
    },
}

pub fn execute(root: &SkillRoot, command: ModelAssistanceCommand) -> Result<u8> {
    let binding = crate::v2::require_execution(root, "model_assistance")?;
    let command = match command {
        ModelAssistanceCommand::PrepareOcr(inputs) => {
            return execute_ocr7(root, inputs, false, true, binding);
        }
        ModelAssistanceCommand::RunOcr { inputs, live } => {
            return execute_ocr7(root, inputs, live, false, binding);
        }
        command => command,
    };
    let (inputs, live, prepare_only, pages) = match command {
        ModelAssistanceCommand::Prepare(inputs) => (inputs, false, true, false),
        ModelAssistanceCommand::Run { inputs, live } => (inputs, live, false, false),
        ModelAssistanceCommand::PreparePages(inputs) => (inputs, false, true, true),
        ModelAssistanceCommand::RunPages { inputs, live } => (inputs, live, false, true),
        ModelAssistanceCommand::PrepareOcr(_) | ModelAssistanceCommand::RunOcr { .. } => {
            unreachable!("handled explicit source7 above")
        }
    };
    let tools = oe_knowledge::poppler::PopplerTools::default();
    let prepared = if pages {
        Preparation::Pages(oe_engineering::model_page_support::prepare_with_pages(
            root,
            &inputs.input,
            &inputs.profile,
            &tools,
        )?)
    } else {
        Preparation::Original(model_assistance::prepare(
            root,
            &inputs.input,
            &inputs.profile,
        )?)
    };
    let directory = root.private_path(&inputs.output_dir)?;
    if prepare_only {
        let admitted = prepared.admitted();
        let result = match prepared {
            Preparation::Original(value) => {
                model_assistance::write_prepared(root, &value, &directory.join("prepared"))?
            }
            Preparation::Pages(value) => model_assistance::write_page_prepared(
                root,
                value,
                &tools,
                &directory.join("prepared"),
            )?,
        };
        let receipt = json!({"schema":if pages {"oev2.model-page-assistance-cli/2"} else {"oev2.model-assistance-cli/1"}, "action":"prepare", "result":result,
            "execution_binding":binding,"model_execution":"not_run", "engineering_adoption":"not_established"});
        write_new(&directory.join("entry.json"), &receipt)?;
        println!(
            "{}",
            json!({"status":result["status"],"model_execution":"not_run","engineering_adoption":"not_established"})
        );
        return Ok(if admitted { 0 } else { 1 });
    }
    let mut adapter_error = None;
    let transport = if live && prepared.admitted() {
        let credential_file = root.expand(&PathBuf::from(&prepared.profile().credential_file))?;
        match HttpModelTransport::new(
            &prepared.profile().endpoint,
            &credential_file,
            Duration::from_secs(prepared.profile().timeout_seconds),
            prepared.profile().maximum_response_bytes,
        ) {
            Ok(transport) => Some(transport),
            Err(error) => {
                adapter_error = Some(serde_json::to_value(error)?);
                None
            }
        }
    } else {
        None
    };
    let selected_transport = transport
        .as_ref()
        .map(|value| value as &dyn model_assistance::transport::ModelTransport);
    let result = if pages {
        model_assistance::run_pages(
            root,
            &inputs.input,
            &inputs.profile,
            &directory.join("execution"),
            &tools,
            selected_transport,
        )?
    } else {
        model_assistance::run(
            root,
            &inputs.input,
            &inputs.profile,
            &directory.join("execution"),
            selected_transport,
        )?
    };
    let status = if adapter_error.is_some() {
        "not_run"
    } else {
        result["status"].as_str().unwrap_or("unknown")
    };
    let transport_observation = transport.as_ref().map(HttpModelTransport::observation);
    // Separate local timing artifact: the historical /1 entry and transport
    // observation keep their original shapes and meanings.
    if let Some(transport) = &transport {
        write_new(
            &directory.join("transport-wait-observation.json"),
            &serde_json::to_value(transport.wait_observation())?,
        )?;
    }
    let receipt = json!({"schema":if pages {"oev2.model-page-assistance-cli/2"} else {"oev2.model-assistance-cli/1"}, "action":"run", "live_requested":live,
        "adapter":"explicit_responses_http/1", "adapter_constructed":transport.is_some(), "adapter_error":adapter_error,
        "transport_observation":transport_observation,
        "execution_binding":binding,"result":result,"status":status,
        "engineering_adoption":"not_established","codex_execution":false,"automatic_judgment_execution":false});
    write_new(&directory.join("entry.json"), &receipt)?;
    println!(
        "{}",
        json!({"status":status,"model_execution":result["model_execution"],
        "adopted":false,"usage_status":result["usage"]["status"],"adapter_error":adapter_error})
    );
    Ok(if status == "completed" && adapter_error.is_none() {
        0
    } else {
        1
    })
}

// CLI selection only. Model dispatch and archival remain in one owning module.
enum Preparation {
    Original(model_assistance::Prepared),
    Pages(oe_engineering::model_page_support::PageAssistedPrepared),
}
impl Preparation {
    fn admitted(&self) -> bool {
        match self {
            Self::Original(value) => value.admitted(),
            Self::Pages(value) => value.admitted(),
        }
    }
    fn profile(&self) -> &oe_contracts::ModelServiceProfile {
        match self {
            Self::Original(value) => &value.profile,
            Self::Pages(value) => value.service_profile(),
        }
    }
}

// New selection facade; original modes and dispatcher retain their exact guards.
fn execute_ocr7(
    root: &SkillRoot,
    inputs: Inputs,
    live: bool,
    prepare_only: bool,
    binding: serde_json::Value,
) -> Result<u8> {
    use oe_engineering::model_registered_ocr7 as channel;
    use oe_knowledge::registered_ocr6;
    use std::time::Instant;
    // One caller clock before input/source reads, no per-page resets or retries.
    let deadline = Instant::now() + Duration::from_secs(120);
    let directory = root.new_private_dir(&inputs.output_dir)?;
    let selectors = channel::read_input7(root, &inputs.input)?;
    let tools = oe_knowledge::poppler::PopplerTools::default();
    let mut actual = Vec::new();
    for (ordinal, selection) in selectors.ocr_pages.iter().enumerate() {
        let leaf = format!(
            "model7-{}-{ordinal}",
            oe_core::sha256(directory.as_os_str().as_encoded_bytes())
        );
        actual.push(registered_ocr6::prepare_registered_page_ocr6(
            root,
            &selection.request,
            &tools,
            deadline,
            &leaf,
        )?);
    }
    let sources: Vec<_> = actual.iter().collect();
    let prepared = channel::prepare_with_registered_ocr7(
        root,
        &inputs.input,
        &inputs.profile,
        &tools,
        &sources,
    )?;
    let admitted = prepared.admitted();
    if prepare_only {
        let result = model_assistance::write_registered_ocr_prepared7(
            root,
            prepared,
            &tools,
            &directory.join("prepared"),
        )?;
        write_new(
            &directory.join("entry.json"),
            &json!({"schema":"oev2.model-registered-ocr-cli/7","action":"prepare","execution_binding":binding,
            "actual_ocr_calls":actual.len(),"result":result,"model_execution":"not_run","engineering_adoption":"not_established","audit_restores_authority":false}),
        )?;
        println!(
            "{}",
            json!({"status":result["status"],"model_execution":"not_run","engineering_adoption":"not_established"})
        );
        return Ok(if admitted { 0 } else { 1 });
    }
    let mut adapter_error = None;
    let transport = if live && admitted {
        let profile = prepared.service_profile();
        let credential_file = root.expand(&PathBuf::from(&profile.credential_file))?;
        match HttpModelTransport::new(
            &profile.endpoint,
            &credential_file,
            Duration::from_secs(profile.timeout_seconds),
            profile.maximum_response_bytes,
        ) {
            Ok(t) => Some(t),
            Err(e) => {
                adapter_error = Some(serde_json::to_value(e)?);
                None
            }
        }
    } else {
        None
    };
    let selected = transport
        .as_ref()
        .map(|t| t as &dyn model_assistance::transport::ModelTransport);
    let result = model_assistance::run_registered_ocr7(
        root,
        prepared,
        &directory.join("execution"),
        &tools,
        selected,
    )?;
    if let Some(t) = &transport {
        write_new(
            &directory.join("transport-wait-observation.json"),
            &serde_json::to_value(t.wait_observation())?,
        )?;
    }
    let status = if adapter_error.is_some() {
        "not_run"
    } else {
        result["status"].as_str().unwrap_or("unknown")
    };
    write_new(
        &directory.join("entry.json"),
        &json!({"schema":"oev2.model-registered-ocr-cli/7","action":"run","execution_binding":binding,
        "live_requested":live,"adapter_constructed":transport.is_some(),"adapter_error":adapter_error,
        "transport_observation":transport.as_ref().map(HttpModelTransport::observation),"actual_ocr_calls":actual.len(),
        "result":result,"status":status,"engineering_adoption":"not_established","audit_restores_authority":false}),
    )?;
    println!(
        "{}",
        json!({"status":status,"model_execution":result["model_execution"],"adopted":false})
    );
    Ok(if status == "completed" && adapter_error.is_none() {
        0
    } else {
        1
    })
}
