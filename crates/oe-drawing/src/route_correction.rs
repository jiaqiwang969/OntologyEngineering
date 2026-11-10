//! Correction steps of the drawing route; the worker receives only the corrected bytes.
use super::*;

impl Run<'_> {
    pub(super) fn write_corrected(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let Some(path) = &self.correction_plan else {
            s.facts["reason"] =
                json!("no correction plan; no source deviation inferred by the writer");
            return Ok("not_applicable");
        };
        let path = PathBuf::from(path);
        let plan_file = self.file(&path)?;
        let plan: MdDrawingCorrectionPlan =
            serde_json::from_slice(&read_regular_bytes(&path, 16 * 1024 * 1024)?)?;
        self.given.push((path, plan_file.sha256.clone()));
        s.inputs.push(plan_file);
        let dxf = self.dxf.as_ref().context("drawing_route_dxf_missing")?;
        s.inputs.push(self.file(dxf)?);
        // Rendering is mandatory for an applied plan; don't create an incomplete corrected drawing.
        ensure!(
            self.program.is_some(),
            "corrected_render_requires_verified_cadcraft"
        );
        let receipt = crate::correction::write_new(dxf, &self.out.join("corrected.dxf"), &plan)?;
        oe_core::write_new(
            &self.out.join("correction-plan.json"),
            &serde_json::to_value(&plan)?,
        )?;
        oe_core::write_new(
            &self.out.join("correction-receipt.json"),
            &serde_json::to_value(&receipt)?,
        )?;
        for name in [
            "corrected.dxf",
            "correction-plan.json",
            "correction-receipt.json",
        ] {
            s.outputs.push(self.file(&self.out.join(name))?);
        }
        s.facts["applied"] = json!(receipt.applied.len());
        self.correction = Some((plan, receipt));
        Ok("passed")
    }

    pub(super) fn read_corrected(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        if self.correction.is_none() {
            return Ok("not_applicable");
        }
        let dxf = self.out.join("corrected.dxf");
        s.inputs.push(self.file(&dxf)?);
        let args = vec![
            "-p".into(),
            usage_discovery::worker_profile(&self.exe)?.into(),
            self.exe.clone().into_os_string(),
            review::WORKER_FLAG.into(),
        ];
        let output = self.call(
            s,
            "read-corrected",
            Path::new("/usr/bin/sandbox-exec"),
            args,
            &[],
            Path::new("/"),
            120,
            Some(&dxf),
            true,
        )?;
        let graph: MdEngineeringExpressionGraph = serde_json::from_slice(&output.stdout)?;
        ensure!(
            graph.schema == engineering_expression::SCHEMA
                && graph.source_sha256 == self.file(&dxf)?.sha256,
            "corrected_worker_source_binding"
        );
        engineering_expression::check(&graph)?;
        oe_core::write_new_bytes(&self.out.join("corrected-graph.json"), &output.stdout)?;
        s.outputs
            .push(self.file(&self.out.join("corrected-graph.json"))?);
        s.facts["worker_input"] =
            json!("corrected DXF bytes only; no plan, receipt, path or old graph");
        Ok("passed")
    }

    pub(super) fn render_corrected(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        if self.correction.is_none() {
            return Ok("not_applicable");
        }
        let program = self.program.clone().context("corrected_renderer_missing")?;
        let dxf = self.out.join("corrected.dxf");
        let svg = self.out.join("corrected.svg");
        let png = self.out.join("corrected-preview.png");
        let mut plot = json!({"path": png});
        if let Some(fonts) = self
            .runtime
            .as_ref()
            .and_then(|r| r.fonts_directory.as_ref())
        {
            plot["fontDirectory"] = json!(fonts);
        }
        s.inputs.push(self.file(&dxf)?);
        let vars = self.cadcraft_environment(true);
        self.call(
            s,
            "render-corrected",
            &program,
            vec![
                "run".into(),
                dxf.into_os_string(),
                "--cmd".into(),
                format!("reviewplot {}", serde_json::to_string(&plot)?).into(),
                "--export".into(),
                svg.clone().into_os_string(),
            ],
            &vars,
            &self.work,
            900,
            None,
            true,
        )?;
        page::rendering(&read_regular_bytes(&svg, 64 * 1024 * 1024)?)?;
        for p in [&svg, &png] {
            ensure!(std::fs::metadata(p)?.len() > 0, "corrected_render_empty");
            s.outputs.push(self.file(p)?);
        }
        Ok("passed")
    }

    pub(super) fn check_corrected(&mut self, s: &mut MdDrawingRouteStep) -> Result<&'static str> {
        let Some((plan, receipt)) = &self.correction else {
            return Ok("not_applicable");
        };
        let source = self.dxf.as_ref().context("drawing_route_dxf_missing")?;
        let corrected = self.out.join("corrected.dxf");
        crate::correction::verify(
            &read_regular_bytes(source, 64 * 1024 * 1024)?,
            &read_regular_bytes(&corrected, 64 * 1024 * 1024)?,
            plan,
            receipt,
        )?;
        let graph: Value = serde_json::from_slice(&read_regular_bytes(
            &self.out.join("corrected-graph.json"),
            64 * 1024 * 1024,
        )?)?;
        let shown = page::rendering(&read_regular_bytes(
            &self.out.join("corrected.svg"),
            64 * 1024 * 1024,
        )?)?;
        let roots: std::collections::BTreeSet<_> = graph["source"]["entities"]
            .as_array()
            .context("corrected_graph_roots")?
            .iter()
            .filter_map(|e| e["handle"].as_str())
            .collect();
        ensure!(
            shown.active.is_empty() && shown.entities.iter().all(|h| roots.contains(h.as_str())),
            "corrected_render_foreign_roots"
        );
        for op in plan.operations.iter().filter(|o| o.kind == "add_text") {
            let applied = receipt
                .applied
                .iter()
                .find(|a| a.operation_id == op.id)
                .context("corrected_applied_text_missing")?;
            ensure!(
                applied
                    .result_handles
                    .iter()
                    .all(|h| shown.entities.contains(h)),
                "corrected_added_text_not_rendered"
            );
        }
        for name in [
            "corrected.dxf",
            "corrected-graph.json",
            "corrected.svg",
            "correction-plan.json",
            "correction-receipt.json",
        ] {
            s.inputs.push(self.file(&self.out.join(name))?);
        }
        let report = json!({"status":"passed","source_sha256":receipt.source_sha256,"corrected_sha256":receipt.corrected_sha256,
            "checks":["recorded byte changes reconstruct saved drawing", "untargeted source records unchanged", "new handles unique", "operation postconditions", "bytes-only readback", "rendered roots belong to corrected source"],
            "scope":"correction execution and source preservation, not engineering acceptance"});
        oe_core::write_new(&self.out.join("correction-check.json"), &report)?;
        s.outputs
            .push(self.file(&self.out.join("correction-check.json"))?);
        s.facts = report;
        Ok("passed")
    }
}

#[cfg(all(test, target_os = "macos"))]
mod correction_tests {
    use super::*;
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/correction_fixture.rs"
        ));
    }

    /// Exercises the real four correction steps without constructing or invoking a Project.
    #[test]
    #[ignore = "requires a separately built worker and an explicitly supplied pinned CADCraft runtime"]
    fn synthetic_correction_steps_write_read_render_and_verify() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let home = PathBuf::from(
            std::env::var_os("OE_CORRECTION_EVIDENCE").expect("new evidence directory"),
        );
        std::fs::create_dir(&home).unwrap();
        let home = home.canonicalize().unwrap();
        let root = SkillRoot::new(source_root)
            .unwrap()
            .with_project_data_root(&home)
            .unwrap();
        let exe = PathBuf::from(std::env::var_os("OE_TEST_DRAWING_WORKER").expect("built oev2"));
        let runtime = PathBuf::from(
            std::env::var_os("OE_TEST_CADCRAFT_RUNTIME").expect("pinned runtime record"),
        );
        let bytes = fixture::fixture();
        let source = home.join("source.dxf");
        oe_core::write_new_bytes(&source, &bytes).unwrap();
        let plan = json!({"source_sha256":sha256(&bytes),"issues":[{"id":"fixture-letter","location":"synthetic annotation","source_occurrences":[],"observed":["synthetic execution fixture"],"rule":"synthetic_fixture","clause":"execution only; no drafting verdict asserted","concept_ids":["DC-G9-023"],"confidence":"certain","alternatives":[],"proposed_correction":"add test letter"}],"operations":[{"id":"add-letter","issue_id":"fixture-letter","kind":"add_text","occurrence":[],"expected_record_sha256":null,"text":{"value":"R","position":[34.,18.],"height":3.,"rotation":90.,"layer":"0"},"style":[],"translation":[]}]});
        let plan_path = home.join("plan.json");
        oe_core::write_new(&plan_path, &plan).unwrap();
        let q = MdDrawingRouteRequest {
            schema: REQUEST_SCHEMA.into(),
            project: "unused".into(),
            object: "unused".into(),
            dwg: None,
            dxf: Some(source.to_string_lossy().into_owned()),
            svg: None,
            cadcraft_runtime: Some(runtime.to_string_lossy().into_owned()),
            output_directory: "route".into(),
            title: "Synthetic correction".into(),
            chrome: None,
        };
        let out = home.join("route");
        let work = out.join("work");
        std::fs::create_dir_all(work.join("tmp")).unwrap();
        let mut run = Run {
            root: &root,
            q: &q,
            home: home.clone(),
            out: out.clone(),
            work,
            lock: lock().unwrap(),
            exe_sha256: sha256(std::fs::read(&exe).unwrap()),
            exe,
            program: None,
            runtime: None,
            given: vec![],
            dxf: Some(source.clone()),
            svg: None,
            project: None,
            context: None,
            request: None,
            page: None,
            correction: None,
            correction_plan: Some(plan_path),
        };
        let mut steps = Vec::new();
        for id in [
            "verify-runtime",
            "write-corrected",
            "read-corrected",
            "render-corrected",
            "check-corrected",
        ] {
            let mut s = MdDrawingRouteStep {
                id: id.into(),
                status: "not_run".into(),
                seconds: 0.,
                inputs: vec![],
                outputs: vec![],
                external_processes: vec![],
                facts: json!({}),
                failure: None,
            };
            match run.step(&mut s) {
                Ok(status) => s.status = status.into(),
                Err(e) => {
                    s.status = "failed".into();
                    s.failure = Some(format!("{e:#}"));
                }
            }
            steps.push(s);
            oe_core::write_new_bytes(
                &home.join(format!("step-{}.json", steps.len())),
                &serde_json::to_vec_pretty(steps.last().unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(
                steps.last().unwrap().status,
                "passed",
                "{:?}",
                steps.last().unwrap().failure
            );
        }
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        let graph: MdEngineeringExpressionGraph =
            serde_json::from_slice(&std::fs::read(out.join("corrected-graph.json")).unwrap())
                .unwrap();
        assert!(
            graph
                .source
                .entities
                .iter()
                .any(|e| e.handle == "F0" && e.kind == "TEXT")
        );
        oe_core::write_new(&home.join("result.json"),&json!({"status":"passed","steps":steps,"scope":"Synthetic correction steps only; no Project, no complete drawing route, no engineering acceptance"})).unwrap();
    }
}
