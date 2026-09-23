"""Recipient orchestration of a frozen method chain through the OE native entry.

This is transport and local execution bookkeeping, not a registry or semantic
engine. All workspace writes use semantic_engagement. Publisher method-source
authorship is preserved; decisions, execution receipts and adoption are local.
"""
from __future__ import annotations

from contextlib import contextmanager
from copy import deepcopy
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile
import uuid

from ontology_engineering import semantic_engagement as entry
from ontology_engineering import semantica_runtime as runtime
from scripts.semantic_bundle_transport import (
    ROOT, _regular_inside, load_bundle, safe_relative, validate_data_archive,
)

LOCK = "runtime/semantic-bootstrap.json"
CODE = ("ontology_engineering/method_bootstrap.py", "scripts/method_bootstrap.py",
        "ontology_engineering/semantic_engagement.py", "ontology_engineering/semantica_runtime.py",
        "scripts/semantic_bundle_transport.py")
CATEGORIES = ("ontology", "competency_questions", "shapes", "queries", "rules",
              "cases", "contract", "provenance")
CAPS = ["semantic.package.load", "sparql.select", "shacl.validate", "rule.forward_chain"]
DECISION_SCOPE = ["receive-frozen-method-commit", "receive-frozen-method-promotion"]
STATES = ["candidate", "proposed", "committed", "regression_passed", "release_complete", "promoted"]


class BootstrapError(ValueError):
    pass


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def now():
    return datetime.now(timezone.utc).isoformat()


def read(path):
    path = Path(path)
    if path.is_symlink():
        raise BootstrapError("symbolic bookkeeping path is not accepted")
    return json.loads(path.read_bytes())


def retain(path, value):
    """Keep immutable local inputs; native state is always read from Semantica."""
    path = Path(path)
    data = encoded(value)
    if path.exists():
        if path.is_symlink() or path.read_bytes() != data:
            raise BootstrapError("retained input differs: " + path.name)
        return path
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    return path


def code_identity(root):
    return {name: digest(_regular_inside(root, name).read_bytes()) for name in CODE}


def load_capsule(name="engineering-judgment-intake", root=ROOT):
    root = Path(root)
    lock = read(_regular_inside(root, LOCK))
    if lock.get("schema") != "ontology-engineering.method-bootstrap-lock/v1":
        raise BootstrapError("unsupported bootstrap lock")
    spec = lock["capsules"][name]
    bundle, _, _ = load_bundle(name, root)
    for key in ("package_id", "package_version", "package_sha256", "runtime"):
        if spec[key] != bundle[key]:
            raise BootstrapError("bootstrap and active bundle differ: " + key)
    payload = validate_data_archive(_regular_inside(root, spec["path"]).read_bytes(), spec)
    capsule = json.loads(payload["capsule.json"])
    if capsule.get("schema") != "semantica.method-bootstrap-capsule/v1":
        raise BootstrapError("unsupported capsule schema")
    if (capsule["package_id"], capsule["target_version"], capsule["target_package_sha256"], capsule["runtime"]) != (
        spec["package_id"], spec["package_version"], spec["package_sha256"], spec["runtime"]
    ):
        raise BootstrapError("capsule target differs from locked bundle")
    if entry._native_authority_token(capsule["publisher_fact_authority"]) != capsule["native_source_authority"]:
        raise BootstrapError("method-source authorship identity differs")
    native = runtime._native_refinery_module()
    previous = ("0", native.EMPTY_PACKAGE_SHA256)
    inventory = {"capsule.json"}
    for step in capsule["steps"]:
        delta_path = safe_relative(step["delta_path"])
        delta = native.PackageDelta.from_dict(json.loads(payload[delta_path]))
        if (delta.base_version, delta.base_package_sha256) != previous:
            raise BootstrapError("capsule predecessor chain differs")
        if (delta.package_id, delta.target_version, delta.delta_sha256, delta.created_by) != (
            capsule["package_id"], step["version"], step["delta_sha256"], capsule["native_source_authority"]
        ):
            raise BootstrapError("capsule delta identity differs")
        manifest_path = safe_relative(step["manifest_path"])
        manifest = json.loads(payload[manifest_path])
        if (manifest["package_id"], manifest["version"]) != (delta.package_id, delta.target_version):
            raise BootstrapError("capsule execution manifest differs")
        inventory.update((delta_path, manifest_path))
        for asset in manifest["assets"]:
            member = str(Path(manifest_path).parent / safe_relative(asset["path"]))
            if digest(payload[member]) != asset["sha256"]:
                raise BootstrapError("capsule asset differs")
            inventory.add(member)
        registry = json.loads(payload[safe_relative(step["scenario_registry"])])
        if registry["package_id"] != delta.package_id or not registry["scenarios"]:
            raise BootstrapError("capsule scenario registry differs")
        for source in delta.source_evidence:
            member = "sources/" + source.sha256 + ".json"
            if digest(payload[member]) != source.sha256 or not source.uri.startswith(capsule["source_logical_root"] + "/"):
                raise BootstrapError("capsule source identity differs")
            inventory.add(member)
        previous = (step["version"], step["package_sha256"])
    if previous != (spec["package_version"], spec["package_sha256"]) or set(payload) != inventory:
        raise BootstrapError("capsule chain or complete inventory differs")
    return spec, payload, capsule


def prepare(directory, *, project, domain, actor, fact_authority, evidence_root,
            name="engineering-judgment-intake", root=ROOT):
    """Create a reviewable local plan; does not initialize a native workspace."""
    directory = Path(directory).absolute()
    if directory.exists() or directory.is_symlink():
        raise BootstrapError("plan requires a new directory")
    for value in (project, domain, actor, fact_authority, evidence_root):
        if not isinstance(value, str) or not value.strip():
            raise BootstrapError("recipient identities must be explicit")
    spec, _, capsule = load_capsule(name, root)
    discovery = entry.discover()
    empty = discovery["corpus_found"]["native_workspace_bootstrap"]
    plan = {"schema": "ontology-engineering.method-bootstrap-plan/v1", "plan_id": uuid.uuid4().hex,
            "directory": str(directory), "workspace": str(directory / "registry"), "bundle": name,
            "capsule_sha256": spec["sha256"], "package_id": capsule["package_id"],
            "package_version": spec["package_version"], "package_sha256": spec["package_sha256"],
            "runtime": spec["runtime"], "code": code_identity(Path(root)), "project": project, "domain": domain,
            "actor": actor, "fact_authority": fact_authority, "evidence_root": evidence_root,
            "workspace_id": "method-library-" + uuid.uuid4().hex,
            "empty_baseline": {"version": empty["baseline_version"], "digest": empty["baseline_digest"]},
            "created_at": now(),
            "scope": "Re-execute and locally commit/promote the frozen method chain, saving each library successor binding separately. Project adoption is a separate action; no physical or model-quality approval."}
    directory.mkdir(parents=True)
    retain(directory / "plan.json", plan)
    # Parse both bindings before any workspace mutation.
    initial = _library_binding(plan, capsule)
    path = retain(directory / "initial-binding.json", initial)
    entry.read_project_binding(path)
    project_template = _project_binding(initial, plan)
    template_path = retain(directory / "project-binding-template.json", project_template)
    entry.read_project_binding(template_path)
    return {"plan": str(directory / "plan.json"), "plan_sha256": digest(encoded(plan)),
            "status": "prepared", "workspace_created": False, "project_adopted": False}


def _library_binding(plan, capsule):
    return {"$schema": "ontology-engineering.semantic-project-binding/v1",
            "binding_id": plan["workspace_id"] + "-0", "project": {"project_id": plan["project"], "domain": plan["domain"]},
            "semantic_target": {"kind": "workspace", "workspace_id": plan["workspace_id"], "package_id": plan["package_id"]},
            "baseline": plan["empty_baseline"], "evidence": {"logical_root": capsule["source_logical_root"]},
            "authority": {"fact": capsule["publisher_fact_authority"],
                          "decision": {"authority_id": plan["actor"], "scope": DECISION_SCOPE}},
            "allowed_actions": ["open", "discover", "run", "review", "propose", "commit", "verify", "history", "promote"],
            "lifecycle_actions": STATES,
            "promotion": {"target": "industry-registry", "requires_decision_authority": True},
            "created_at": plan["created_at"], "semantic_api": "semantica.ontology.refinery/v1"}


def _load_plan(directory, root):
    directory = Path(directory).absolute()
    plan = read(directory / "plan.json")
    if plan.get("schema") != "ontology-engineering.method-bootstrap-plan/v1" or plan["directory"] != str(directory):
        raise BootstrapError("plan directory or schema differs")
    if plan["workspace"] != str(directory / "registry") or directory.is_symlink():
        raise BootstrapError("plan workspace differs")
    spec, payload, capsule = load_capsule(plan["bundle"], root)
    if plan["capsule_sha256"] != spec["sha256"] or plan["code"] != code_identity(Path(root)) or plan["runtime"] != spec["runtime"]:
        raise BootstrapError("bootstrap configuration changed; retain this plan and prepare a separate one")
    for key in ("package_id", "package_version", "package_sha256"):
        if plan[key] != spec[key]:
            raise BootstrapError("plan target differs")
    runtime.verify_runtime_source_identity()
    return directory, plan, payload, capsule


def _project_binding(library, plan):
    proposed = deepcopy(library)
    proposed["binding_id"] = plan["plan_id"] + "-project"
    proposed["evidence"] = {"logical_root": plan["evidence_root"]}
    proposed["authority"]["fact"] = {"authority_id": plan["fact_authority"], "scope": ["controlled-project-records"]}
    # Native workspace bindings require a candidate prefix and its command.
    # This permits nonauthoritative proposals, never commit or promotion.
    proposed["allowed_actions"] = ["open", "discover", "run", "review", "history", "propose"]
    proposed["lifecycle_actions"] = ["candidate", "proposed"]
    return proposed


def _check_project_scope(binding, plan):
    expected = {
        "project": {"project_id": plan["project"], "domain": plan["domain"]},
        "semantic_target": {"kind": "workspace", "workspace_id": plan["workspace_id"], "package_id": plan["package_id"]},
        "baseline": {"version": plan["package_version"], "digest": plan["package_sha256"]},
        "authority": {"fact": {"authority_id": plan["fact_authority"], "scope": ["controlled-project-records"]},
                      "decision": {"authority_id": plan["actor"], "scope": DECISION_SCOPE}},
    }
    for key, value in expected.items():
        if binding[key] != value:
            raise BootstrapError("project adoption exceeds the exact plan: " + key)


def _authorization(path, plan, action, *, binding_sha256=None):
    value = read(path)
    required = {"schema", "action", "actor_id", "plan_sha256", "reason", "issued_at"}
    if action == "adopt-project-binding":
        required.add("binding_sha256")
    if set(value) != required or value.get("schema") != "ontology-engineering.method-bootstrap-authorization/v1":
        raise BootstrapError("explicit local authorization fields required")
    if (value["action"], value["actor_id"], value["plan_sha256"]) != (action, plan["actor"], digest(encoded(plan))):
        raise BootstrapError("authorization differs from exact plan/action/actor")
    if not isinstance(value["reason"], str) or not value["reason"].strip():
        raise BootstrapError("authorization basis must be stated")
    stamp = datetime.fromisoformat(value["issued_at"].replace("Z", "+00:00"))
    if stamp.tzinfo is None:
        raise BootstrapError("authorization timestamp must have timezone")
    if binding_sha256 is not None and value["binding_sha256"] != binding_sha256:
        raise BootstrapError("project binding authorization differs")
    return value


@contextmanager
def _exclusive(directory):
    path = directory / ".bootstrap.lock"
    if path.is_symlink():
        raise BootstrapError("symbolic lock is not accepted")
    with path.open("a+b") as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise BootstrapError("bootstrap already running") from exc
        try:
            yield
        finally:
            fcntl.flock(stream, fcntl.LOCK_UN)


def _task(plan, capsule, delta, action):
    return {"$schema": "ontology-engineering.semantic-task-envelope/v1",
            "task_id": uuid.uuid4().hex, "task_kind": "receive-frozen-engineering-methods",
            "intent": "Execute this locally authorized step of the exact frozen method chain",
            "project": plan["project"], "domain": plan["domain"], "requested_decision": "Local method package lifecycle; no project fact acceptance",
            "actor_id": plan["actor"], "requested_actions": [action], "required_capabilities": CAPS,
            "evidence": delta["source_evidence"], "created_at": now()}


def _authoring_execution(directory, payload, step):
    """Execute all actual cases; no publisher PASS or supplied gate is imported."""
    runner = runtime.create_package_runner()
    source = runtime.read_runtime_source_lock()
    checks = []
    with tempfile.TemporaryDirectory(prefix="method-bootstrap-") as temporary:
        temporary = Path(temporary)
        for name, data in payload.items():
            path = temporary / safe_relative(name)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        registry = json.loads(payload[step["scenario_registry"]])
        for scenario in registry["scenarios"]:
            result = runner.run_manifest(temporary / step["manifest_path"], scenario["id"],
                runtime_commit=source.commit, runtime_artifact_sha256=source.artifact_sha256, runtime_version=source.version)
            execution, verification = result.as_dict(), runner.verify(result).as_dict()
            check = {"execution": execution, "verification": verification}
            if execution["status"] != "passed" or verification["status"] != "complete":
                retain(directory / ("authoring-failure-" + uuid.uuid4().hex + ".json"), check)
                raise BootstrapError("recipient authoring execution did not pass")
            checks.append(check)
    return checks


def _engagement(checks):
    value = {"engagement_id": uuid.uuid4().hex, "learning": {"status": "candidate",
             "rationale": "Frozen reusable method chain; all authoring scenarios executed and receipts verified locally."},
             "execution_receipts": [{**{k: c["execution"][k] for k in ("package_id", "package_version", "package_digest")},
                "receipt_sha256": c["execution"]["receipt"]["receipt_sha256"]} for c in checks], "created_at": now()}
    for phase in ("execution", "receipt", "regression", "release"):
        value[phase] = {"name": phase, "status": "passed", "required_capabilities": CAPS,
                        "observed_capabilities": CAPS, "evidence_sha256": digest(encoded(checks)),
                        "details": {"scope": "Actual recipient authoring-manifest executions and receipt checks. Native committed-subject gates are derived separately; no physical acceptance."}}
    return value


def _step_authorization(plan, capsule, delta, action, basis):
    return {"$schema": "ontology-engineering.refinery-authorization/v1", "authorization_id": uuid.uuid4().hex,
            "action": action, "actor_id": plan["actor"], "authority_id": plan["actor"], "authority_scope": DECISION_SCOPE,
            "package_id": delta["package_id"], "delta_sha256": delta["delta_sha256"], "promotion_target": "industry-registry",
            "reason": "Recipient frozen-chain action under exact local plan: " + basis["reason"],
            "source": {"source_id": "recipient-initialization-authorization", "uri": capsule["source_logical_root"] + "/recipient-authorization",
                       "sha256": digest(encoded(basis)), "media_type": "application/json", "captured_at": basis["issued_at"]},
            "issued_at": now(), "decisions": [
                {**{k: a[k] for k in ("category", "asset_id", "operation", "replaces_sha256")}, "verdict": "approve",
                 "reason": "Replay the frozen method successor under this local initialization authorization; preserve predecessor history."}
                for category in CATEGORIES for a in delta[category]
                if action == "commit" and a["operation"] in ("replace", "remove")]}


def _expected_context(binding, task, candidate, action):
    native = runtime._native_refinery_module()
    return native.TransitionContextDTO.create(action=action, delta_sha256=candidate,
        envelope=native.SemanticTaskEnvelope.from_dict(entry._native_task(entry.read_task_envelope(task, binding=binding), action)),
        binding=native.ProjectOntologyBinding.from_dict(entry._native_binding(binding))).context_sha256


def _check_events(binding_path, step_dir, events, candidate):
    """Compare retained local requests with verified native events on every resume."""
    binding = entry.read_project_binding(binding_path)
    actions = {"candidate": "propose", "proposed": "propose", "committed": "commit",
               "regression_passed": "verify", "release_complete": "verify", "promoted": "promote"}
    native = runtime._native_refinery_module()
    for event in events:
        state = event["state"]; action = actions[state]
        task = step_dir / ("task-" + action + ".json")
        if event["payload"]["transition_context_sha256"] != _expected_context(binding, task, candidate, state):
            raise BootstrapError("retained task differs from native event: " + state)
        if action in ("commit", "promote"):
            auth = entry._native_authorization_input(step_dir / ("authorization-" + action + ".json"),
                binding=binding, expected_action=action, candidate_sha256=candidate)
            if native.RefineryAuthorizationDTO.from_dict(auth).authorization_sha256 != event["payload"]["authorization_sha256"]:
                raise BootstrapError("retained authorization differs from native event")


def apply(directory, authorization, *, root=ROOT, progress=None):
    directory, plan, payload, capsule = _load_plan(directory, root)
    basis = _authorization(authorization, plan, "initialize-frozen-methods")
    with _exclusive(directory):
        retain(directory / "initialization-authorization.json", basis)
        binding_path = retain(directory / "initial-binding.json", _library_binding(plan, capsule))
        for index, step in enumerate(capsule["steps"]):
            step_dir = directory / ("step-" + str(index).zfill(3)); step_dir.mkdir(exist_ok=True)
            binding_path = retain(step_dir / "binding.json", read(binding_path))
            delta = json.loads(payload[step["delta_path"]]); candidate = step["delta_sha256"]
            projected = deepcopy(delta); projected["created_by"] = capsule["publisher_fact_authority"]["authority_id"]
            delta_path = retain(step_dir / "delta-input.json", projected)
            binding = entry.read_project_binding(binding_path)
            native = runtime._native_refinery_module()
            if native.PackageDelta.from_dict(entry._native_delta_input(delta_path, binding=binding)).delta_sha256 != candidate:
                raise BootstrapError("source projection changed native delta identity")
            # A promotion can stop between native ledgers. Only replay its exact
            # retained request; the native API owns cross-ledger recovery.
            pending_promotion = step_dir / "authorization-promote.json"
            if pending_promotion.exists() and not (step_dir / "promote.json").exists():
                resumed = entry.promote(binding_path, workspace=plan["workspace"], task=step_dir / "task-promote.json",
                                        candidate_sha256=candidate, authorization=pending_promotion)
                if resumed["command_verdict"] != "passed":
                    raise BootstrapError("native promotion recovery did not pass")
                retain(step_dir / "promote.json", resumed)
            for action in ("open", "propose", "commit", "verify", "promote"):
                task_path = step_dir / ("task-" + action + ".json")
                task_existed = task_path.exists()
                if not task_existed:
                    retain(task_path, _task(plan, capsule, delta, action))
                result_path = step_dir / (action + ".json")
                if action == "open":
                    if index == 0 and not task_existed and Path(plan["workspace"]).exists():
                        raise BootstrapError("workspace already exists outside this initialization")
                    if not result_path.exists():
                        result = entry.open_engagement(binding_path, task=task_path, workspace=plan["workspace"])
                        retain(result_path, result)
                    continue
                # A first or interrupted proposal has a native idempotent entry.
                # Later stages read verified history; errors are never treated
                # as absence of a candidate.
                events = []
                if (step_dir / "propose.json").exists() or (step_dir / "task-commit.json").exists() or action != "propose":
                    hist = entry.history(binding_path, workspace=plan["workspace"], candidate_sha256=candidate)
                    events = hist["execution"]["events"]
                _check_events(binding_path, step_dir, events, candidate)
                target = {"propose": "proposed", "commit": "committed", "verify": "release_complete", "promote": "promoted"}[action]
                completed = target in [e["state"] for e in events]
                if completed and action != "promote":
                    if not result_path.exists():
                        retain(result_path, {"status": "recovered_from_native_history", "event": next(e for e in events if e["state"] == target)})
                    continue
                kwargs = {"workspace": plan["workspace"], "task": task_path}
                if action == "propose":
                    engagement_path = step_dir / "engagement.json"
                    if not engagement_path.exists():
                        checks = _authoring_execution(step_dir, payload, step)
                        retain(step_dir / "authoring-checks.json", checks)
                        retain(engagement_path, _engagement(checks))
                    else:
                        checks = read(step_dir / "authoring-checks.json")
                        receipt = read(engagement_path)
                        if any(receipt[p]["evidence_sha256"] != digest(encoded(checks)) for p in ("execution", "receipt", "regression", "release")):
                            raise BootstrapError("local authoring evidence changed")
                    result = entry.propose(binding_path, delta=delta_path, engagement=engagement_path, **kwargs)
                else:
                    kwargs["candidate_sha256"] = candidate
                    if action in ("commit", "promote"):
                        auth_path = step_dir / ("authorization-" + action + ".json")
                        if not auth_path.exists():
                            retain(auth_path, _step_authorization(plan, capsule, delta, action, basis))
                        kwargs["authorization"] = auth_path
                    result = getattr(entry, action)(binding_path, **kwargs)
                if result["command_verdict"] != "passed":
                    retain(step_dir / ("failure-" + uuid.uuid4().hex + ".json"), result)
                    raise BootstrapError("native " + action + " did not pass")
                if not result_path.exists():
                    retain(result_path, result)
                if progress:
                    progress({"version": step["version"], "action": action, "state": result["learning"]["current_state"]})
            next_binding = result["learning"]["next_binding"]
            if next_binding["auto_applied"] or next_binding["document"]["baseline"] != {"version": step["version"], "digest": step["package_sha256"]}:
                raise BootstrapError("native successor identity differs")
            binding_path = retain(step_dir / "library-adopted-binding.json", next_binding["document"])
            retain(step_dir / "library-adoption.json", {"actor": plan["actor"], "basis_sha256": digest(encoded(basis)),
                   "binding_sha256": digest(binding_path.read_bytes()), "scope": "Local method-library continuation only; separate project adoption required."})
        proposed = _project_binding(read(binding_path), plan)
        proposal = retain(directory / "proposed-project-binding.json", proposed)
        entry.read_project_binding(proposal)
        return {"status": "promoted_local_method_library", "package_version": plan["package_version"],
                "package_sha256": plan["package_sha256"], "workspace": plan["workspace"],
                "proposed_binding": str(proposal), "binding_sha256": digest(proposal.read_bytes()), "project_adopted": False}


def adopt(directory, authorization, *, root=ROOT):
    directory, plan, _, capsule = _load_plan(directory, root)
    with _exclusive(directory):
        proposal = directory / "proposed-project-binding.json"
        _check_project_scope(read(proposal), plan)
        library = directory / ("step-" + str(len(capsule["steps"]) - 1).zfill(3)) / "library-adopted-binding.json"
        if read(proposal) != _project_binding(read(library), plan):
            raise BootstrapError("proposed project binding differs from initialization plan")
        basis = _authorization(authorization, plan, "adopt-project-binding", binding_sha256=digest(proposal.read_bytes()))
        discovery = entry.discover(proposal, workspace=plan["workspace"])
        if discovery["command_verdict"] != "passed":
            raise BootstrapError("project binding does not resolve to the current native baseline")
        binding = retain(directory / "project-binding.json", read(proposal))
        retain(directory / "project-adoption.json", {"authorization": basis,
               "binding_sha256": digest(binding.read_bytes()), "discovery": discovery,
               "scope": "Local project use of frozen methods; no project fact, physical, model-quality or publication approval."})
        return {"status": "project_binding_adopted", "binding": str(binding), "workspace": plan["workspace"],
                "package_sha256": plan["package_sha256"]}
