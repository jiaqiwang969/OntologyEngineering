use anyhow::Result;
use oe_contracts::{NxAssemblyObservation, NxAssemblyReferenceSets2};
use oe_nx_protocol::RelativePath;
use oe_nx_worker::geometry::{NativeApi, PartInfo, Workspace};
use oe_nx_worker::{assembly_reference_sets, geometry::Parameters};
use serde_json::Value;
use serde_json::json;
use std::path::{Path, PathBuf};

fn example() -> (NxAssemblyObservation, NxAssemblyReferenceSets2) {
    let pose = vec![
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let graph=serde_json::from_value(json!({"schema":"oev2.nx-native-assembly-observation/1",
        "root_prototype_id":"root.prt","prototypes":[],"complete":true,"source_role":"reference_observation",
        "engineering_adoption":"not_assessed","occurrences":[{"occurrence_id":"root/000000","parent_occurrence_id":null,
            "prototype_id":"pin.prt","part_occurrence_tag":10,"instance_tag":20,"name":"PIN","reference_set":"None",
            "raw_parent_origin":[0.,0.,0.],"raw_parent_csys":[1.,0.,0.,0.,1.,0.,0.,0.,1.],
            "raw_parent_transform":pose,"raw_root_transform":pose,"local_to_parent_m":pose,"local_to_root_m":pose}]})).unwrap();
    let observation=serde_json::from_value(json!({"schema":assembly_reference_sets::SCHEMA,"api":"UF_ASSEM_ask_component_data",
        "root_prototype_id":"root.prt","complete":true,"members":[{"occurrence_id":"root/000000",
            "part_occurrence_tag":10,"instance_tag":20,"instance_reference_set":"None","part_occurrence_reference_set":"BODY"}]})).unwrap();
    (graph, observation)
}
#[test]
fn reference_set_capture_binds_scope_inventory_and_explicit_params() {
    let (graph, mut observation) = example();
    assembly_reference_sets::validate(&observation, &graph).unwrap();
    assert!(assembly_reference_sets::require_entire(&observation, &graph).is_err());
    observation.members[0].part_occurrence_reference_set = "None".into();
    assembly_reference_sets::require_entire(&observation, &graph).unwrap();
    observation.members[0].part_occurrence_tag = 99;
    assert!(assembly_reference_sets::validate(&observation, &graph).is_err());
    let old = json!({"action":"inspect_assembly","schema":oe_nx_worker::assembly::PARAMS_SCHEMA,
        "root_part":"root.prt","source_role":"reference_observation",
        "budget":{"max_occurrences":10,"max_prototypes":10,"max_depth":10}});
    let parsed: Parameters = serde_json::from_value(old.clone()).unwrap();
    parsed.validate().unwrap();
    assert_eq!(
        parsed.parameter_schema(),
        oe_nx_worker::assembly::PARAMS_SCHEMA
    );
    assert!(matches!(
        parsed,
        Parameters::InspectAssembly {
            capture_reference_sets: None,
            ..
        }
    ));
    let mut forbidden = old.clone();
    forbidden["capture_reference_sets"] = json!(false);
    assert!(
        serde_json::from_value::<Parameters>(forbidden.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    forbidden["capture_reference_sets"] = json!(null);
    assert!(serde_json::from_value::<Parameters>(forbidden).is_err());
    let mut current = old.clone();
    current["capture_reference_sets"] = json!(true);
    assert!(
        serde_json::from_value::<Parameters>(current.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    current["schema"] = json!(assembly_reference_sets::PARAMS_SCHEMA);
    let parsed: Parameters = serde_json::from_value(current.clone()).unwrap();
    parsed.validate().unwrap();
    assert_eq!(
        parsed.parameter_schema(),
        assembly_reference_sets::PARAMS_SCHEMA
    );
    assert!(matches!(
        parsed,
        Parameters::InspectAssembly {
            capture_reference_sets: Some(true),
            ..
        }
    ));
}

struct Api {
    graph: NxAssemblyObservation,
    references: NxAssemblyReferenceSets2,
    reads: usize,
    reference_reads: usize,
    fault: Option<&'static str>,
}
impl NativeApi for Api {
    fn ensure_empty_session(&mut self) -> Result<()> {
        Ok(())
    }
    fn open_part(&mut self, _: &Path) -> Result<u32> {
        Ok(1)
    }
    fn full_assembly_info(
        &mut self,
        _: u32,
        _: &Path,
        _: &oe_contracts::NxAssemblyBudget,
        _: &str,
    ) -> Result<NxAssemblyObservation> {
        self.reads += 1;
        let mut value = self.graph.clone();
        if self.fault == Some("inventory") && self.reads == 2 {
            value.occurrences[0].name = "CHANGED".into();
        }
        Ok(value)
    }
    fn assembly_reference_sets(
        &mut self,
        _: &NxAssemblyObservation,
    ) -> Result<NxAssemblyReferenceSets2> {
        self.reference_reads += 1;
        let mut value = self.references.clone();
        if self.fault == Some("reference") && self.reference_reads == 2 {
            value.members[0].part_occurrence_reference_set = "CHANGED".into();
        }
        Ok(value)
    }
    fn create_mm_part(&mut self, _: &Path) -> Result<u32> {
        unreachable!()
    }
    fn create_mm_expression(&mut self, _: u32, _: &str) -> Result<u32> {
        unreachable!()
    }
    fn create_block(&mut self, _: [f64; 3], _: [String; 3]) -> Result<u32> {
        unreachable!()
    }
    fn edit_expression_update(&mut self, _: u32, _: u32, _: &str) -> Result<i32> {
        unreachable!()
    }
    fn prepare_controlled_representation(&mut self, _: u32) -> Result<Value> {
        unreachable!()
    }
    fn save_work_part(&mut self) -> Result<()> {
        unreachable!()
    }
    fn add_component(&mut self, _: u32, _: u32, _: &Path, _: &str, _: [f64; 3]) -> Result<()> {
        unreachable!()
    }
    fn part_info(&mut self, _: u32) -> Result<PartInfo> {
        unreachable!()
    }
    fn export_ap214(&mut self, _: &Path, _: &Path) -> Result<()> {
        unreachable!()
    }
    fn import_step_new_part(&mut self, _: &Path, _: &Path) -> Result<()> {
        unreachable!()
    }
    fn import_parasolid(&mut self, _: &Path, _: u32) -> Result<Vec<u32>> {
        unreachable!()
    }
}
#[derive(Default)]
struct Work {
    records: Vec<(String, Value)>,
}
impl Workspace for Work {
    fn input(&self, _: &RelativePath) -> Result<PathBuf> {
        Ok(PathBuf::from("/synthetic/root.prt"))
    }
    fn output(&self, _: &str) -> Result<PathBuf> {
        unreachable!()
    }
    fn require_regular_output(&self, _: &str) -> Result<Vec<u8>> {
        unreachable!()
    }
    fn optional_regular_output(&self, _: &str) -> Result<Option<Vec<u8>>> {
        unreachable!()
    }
    fn record(&mut self, phase: &str, value: Value) -> Result<()> {
        self.records.push((phase.into(), value));
        Ok(())
    }
}
#[test]
fn explicit_reference_set_capture_executes_and_rejects_mid_read_changes() {
    for capture in [false, true] {
        let (graph, references) = example();
        let mut api = Api {
            graph,
            references,
            reads: 0,
            reference_reads: 0,
            fault: None,
        };
        let mut workspace = Work::default();
        let mut params = json!({"action":"inspect_assembly","schema":oe_nx_worker::assembly::PARAMS_SCHEMA,
            "root_part":"root.prt","source_role":"reference_observation",
            "budget":{"max_occurrences":10,"max_prototypes":10,"max_depth":10}});
        if capture {
            params["schema"] = json!(assembly_reference_sets::PARAMS_SCHEMA);
            params["capture_reference_sets"] = json!(true);
        }
        let p: Parameters = serde_json::from_value(params.clone()).unwrap();
        let result = oe_nx_worker::geometry::execute(&mut api, &mut workspace, &p).unwrap();
        assert_eq!(api.reads, if capture { 2 } else { 1 });
        assert_eq!(api.reference_reads, if capture { 2 } else { 0 });
        assert_eq!(result.get("reference_sets").is_some(), capture);
        if capture {
            assert_eq!(
                result["reference_sets"]["members"][0]["instance_reference_set"],
                "None"
            );
            assert_eq!(
                result["reference_sets"]["members"][0]["part_occurrence_reference_set"],
                "BODY"
            );
            assert!(
                workspace
                    .records
                    .iter()
                    .any(|(phase, _)| phase == "full_assembly_reference_sets_read")
            );
            for fault in ["inventory", "reference"] {
                let (graph, references) = example();
                let mut api = Api {
                    graph,
                    references,
                    reads: 0,
                    reference_reads: 0,
                    fault: Some(fault),
                };
                let mut work = Work::default();
                let p: Parameters = serde_json::from_value(params.clone()).unwrap();
                let error = oe_nx_worker::geometry::execute(&mut api, &mut work, &p)
                    .unwrap_err()
                    .to_string();
                assert!(
                    error.contains(if fault == "inventory" {
                        "assembly_changed_during_reference_set_read"
                    } else {
                        "assembly_reference_sets_changed_during_read"
                    }),
                    "{error}"
                );
            }
        }
    }
}
