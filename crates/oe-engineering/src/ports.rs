//! Generated work boundary delegates to the existing typed pipeline, not another engine.
use crate::{artifacts, workflow};
use oe_contracts::{
    EngineeringWorkPort, EngineeringWorkRequest, EngineeringWorkResult, FailureRecord,
};
use oe_core::SkillRoot;
use oe_knowledge::poppler::PopplerTools;

pub struct WorkExecutor<'a> {
    pub root: &'a SkillRoot,
    pub tools: &'a PopplerTools,
}
impl EngineeringWorkPort for WorkExecutor<'_> {
    fn work(
        &self,
        input: &EngineeringWorkRequest,
    ) -> std::result::Result<EngineeringWorkResult, FailureRecord> {
        let run = || -> anyhow::Result<EngineeringWorkResult> {
            input
                .validate()
                .map_err(|_| anyhow::anyhow!("work_port_contract_invalid"))?;
            let work: workflow::WorkInput =
                serde_json::from_value(artifacts::json(self.root, &input.input)?)?;
            let result = workflow::run(self.root, &work, self.tools)?;
            Ok(EngineeringWorkResult {
                execution: "completed".into(),
                result,
            })
        };
        run().map_err(|e| artifacts::failure("engineering_work", e))
    }
}
