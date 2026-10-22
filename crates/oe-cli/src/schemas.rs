//! Pure schema consumer boundary. Candidate construction, field validation and
//! engineering adoption stay distinct; no model or external services run here.
use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_core::{SkillRoot, read_regular_bytes};
use semantica_contracts::{SemanticSchemaValidationPort, SemanticSchemaValidationRequest};
use semantica_runtime::schema::RustSchemaRuntime;
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum SchemaCommand {
    /// List the complete pinned original model identities and source schemas.
    List,
    /// Print one original model's exact JSON Schema; does not validate any data.
    Show {
        #[arg(long)]
        model_id: String,
    },
    /// Execute a source-bound generated schema request. model_construct remains unchecked.
    Evaluate {
        #[arg(long)]
        input: PathBuf,
    },
}
pub fn execute(root: &SkillRoot, command: SchemaCommand) -> Result<u8> {
    super::v2::require_execution(root, "schemas")?;
    let runtime = RustSchemaRuntime::new().map_err(|x| anyhow::anyhow!(x.code))?;
    match command {
        SchemaCommand::List => {
            println!(
                "{}",
                serde_json::json!({"schema":"semantica.model-catalog-response/1","models":runtime.model_ids().collect::<Vec<_>>(),"engineering_adoption":"not_established","model_execution":"not_run"})
            );
            Ok(0)
        }
        SchemaCommand::Show { model_id } => {
            println!(
                "{}",
                runtime
                    .model_json_schema(&model_id)
                    .map_err(|x| anyhow::anyhow!(x.code))?
            );
            Ok(0)
        }
        SchemaCommand::Evaluate { input } => {
            let raw = read_regular_bytes(&root.expand(&input)?, 2 * 1024 * 1024)?;
            let request: SemanticSchemaValidationRequest = serde_json::from_slice(&raw)?;
            let result = runtime
                .validate_schema(&request)
                .map_err(|x| anyhow::anyhow!(x.code))?;
            ensure!(result.validate().is_ok(), "schema_output_contract_invalid");
            let code = if result.valid || (!result.validation_executed && result.issues.is_empty())
            {
                0
            } else {
                2
            };
            println!(
                "{}",
                serde_json::json!({"schema":"semantica.schema-cli-response/1","result":result,"engineering_adoption":"not_established","model_execution":"not_run"})
            );
            Ok(code)
        }
    }
}
