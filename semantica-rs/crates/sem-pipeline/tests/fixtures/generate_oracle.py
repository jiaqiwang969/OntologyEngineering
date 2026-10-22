"""Expected values for the sem-pipeline characterisation tests, produced by the original.

Run in the oracle environment (see PORTING.md "Oracle"), from var/semantica-original/oracle/src:

    env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin PYTHONDONTWRITEBYTECODE=1 \
        ../venv/bin/python <crate>/tests/fixtures/generate_oracle.py <crate>/tests/fixtures/oracle.json

Times, timestamps, memory addresses and machine-specific capacities are left out or masked.
"""

import json
import sys
import threading
import time

from semantica.pipeline import (
    ErrorRecovery,
    ErrorSeverity,
    ExecutionEngine,
    ExecutionResult,
    FailureHandler,
    FailureRecovery,
    FallbackHandler,
    ParallelExecutionResult,
    ParallelExecutor,
    ParallelismManager,
    Pipeline,
    PipelineBuilder,
    PipelineSerializer,
    PipelineStatus,
    PipelineStep,
    PipelineTemplate,
    PipelineTemplateManager,
    PipelineValidator,
    ProgressTracker,
    Resource,
    ResourceAllocation,
    ResourceScheduler,
    ResourceType,
    RetryHandler,
    RetryPolicy,
    RetryStrategy,
    StepStatus,
    Task,
    ValidationResult,
)
from semantica.pipeline.failure_handler import RecoveryAction
from semantica.utils.exceptions import ProcessingError, ValidationError

OUT = {}


def err(e):
    return {"type": type(e).__name__, "message": str(e)}


def call(fn):
    try:
        return {"ok": fn()}
    except Exception as e:  # noqa: BLE001
        return {"error": err(e)}


def history(handler):
    rows = []
    for row in handler.get_error_history():
        row = dict(row)
        row.pop("timestamp")
        rows.append(row)
    return rows


def classification(c):
    return {
        "error_type": c["error_type"],
        "severity": c["severity"].value,
        "message": c["message"],
        "traceback": c["traceback"],
    }


def step_state(step):
    return {
        "name": step.name,
        "status": step.status.value,
        "result": step.result,
        "error": None if step.error is None else err(step.error),
    }


# ---------------------------------------------------------------- enums and records
OUT["enums"] = {
    cls.__name__: [[m.name, m.value, str(m), repr(m)] for m in cls]
    for cls in (StepStatus, PipelineStatus, ErrorSeverity, RetryStrategy, ResourceType)
}
OUT["enum_value_error"] = call(lambda: StepStatus("bogus"))
OUT["records"] = {
    "step_repr": repr(PipelineStep("a", "b")),
    "step_repr_full": repr(
        PipelineStep(
            name="s",
            step_type="t",
            config={"k": 1, "x": "y", "f": 0.5, "n": None, "b": True, "l": ["a", 2]},
            dependencies=["a", "b"],
            status=StepStatus.FAILED,
            result={"r": [1, 2]},
            error=ValueError("bad"),
            delta_mode=True,
            base_version_id="v1",
        )
    ),
    "pipeline_repr": repr(Pipeline("p")),
    "pipeline_repr_steps": repr(
        Pipeline("p", steps=[PipelineStep("a", "t")], config={"parallelism": 2}, metadata={"m": "v"})
    ),
    "retry_policy_repr": repr(RetryPolicy()),
    "retry_policy_repr_errors": repr(RetryPolicy(max_retries=1, retryable_errors=[ValueError, KeyError])),
    "validation_result": repr(ValidationResult(True)),
    "execution_result": repr(ExecutionResult(True, None)),
    "failure_recovery": repr(FailureRecovery(False)),
    "recovery_action": [RecoveryAction(True).should_retry, RecoveryAction(True).retry_delay,
                        RecoveryAction(False, 2.5).retry_delay],
    "parallel_result": repr(ParallelExecutionResult("t", True)),
    "resource": repr(Resource("cpu", ResourceType.CPU, 4.0)),
    "allocation": repr(ResourceAllocation("a1", "cpu", ResourceType.CPU, 1.0, "p")),
    "template": repr(PipelineTemplate("t", "d")),
}

# ---------------------------------------------------------------- builder
b = PipelineBuilder()
s = b.add_step(
    "s1", "t1", foo="bar", delta_mode=True, base_version_id="v1", target_version_id="v2",
    dependencies=["x"], n=3,
)
OUT["add_step"] = {
    "returned_type": type(s).__name__,
    "config": s.config,
    "dependencies": s.dependencies,
    "delta_mode": s.delta_mode,
    "base_version_id": s.base_version_id,
    "target_version_id": s.target_version_id,
    "same_object": s is b.steps[0],
    "chained_build": call(lambda: b.add_step("s2", "t2").build()),
    "chained_add_step": call(lambda: b.add_step("s3", "t3").add_step("s4", "t4")),
    "chained_steps": call(lambda: b.add_step("s5", "t5").steps),
}

b = PipelineBuilder()
b.add_step("a", "t")
b.add_step("b", "t", dependencies=["a"])
b.add_step("c", "t")
b.connect_steps("a", "c")
b.connect_steps("a", "c")
b.connect_steps("c", "b")
OUT["connect_steps"] = {
    "b_deps": b.get_step("b").dependencies,
    "b_config": b.get_step("b").config,
    "c_deps": b.get_step("c").dependencies,
    "c_config": b.get_step("c").config,
    "missing": call(lambda: b.connect_steps("a", "zzz")),
    "serialize": b.serialize(),
    "serialize_dict": b.serialize(format="dict"),
}

b = PipelineBuilder()
b.add_step("only", "t", a=1)
b.set_parallelism(3)
p = b.build()
OUT["build"] = {
    "name": p.name,
    "config": p.config,
    "metadata": p.metadata,
    "steps": [st.name for st in p.steps],
}
b2 = PipelineBuilder()
b2.add_step("x", "t")
OUT["build"]["default_parallelism_metadata"] = b2.build("named").metadata
OUT["build_errors"] = {}
OUT["build_errors"]["empty"] = call(lambda: PipelineBuilder().build())
b = PipelineBuilder()
b.add_step("a", "t")
b.add_step("a", "t")
OUT["build_errors"]["duplicate"] = call(lambda: b.build())
b = PipelineBuilder()
b.add_step("a", "t", dependencies=["ghost"])
OUT["build_errors"]["missing_dep"] = call(lambda: b.build())
b = PipelineBuilder()
b.add_step("a", "t", dependencies=["b"])
b.add_step("b", "t", dependencies=["a"])
OUT["build_errors"]["cycle"] = call(lambda: b.build())
b = PipelineBuilder()
b.add_step("", "")
OUT["build_errors"]["blank"] = call(lambda: b.build())

b = PipelineBuilder()
cfg = {
    "name": "config_pipeline",
    "steps": [
        {"name": "s1", "type": "t1", "config": {"a": 1}},
        {"name": "s2", "type": "t2", "config": {"dependencies": ["s1"]}},
        {"name": "", "type": "skipped"},
        {"type": "no_name"},
    ],
    "parallelism": 4,
}
p = b.build_pipeline(cfg)
OUT["build_pipeline"] = {
    "name": p.name,
    "steps": [[st.name, st.step_type, st.config, st.dependencies] for st in p.steps],
    "config": p.config,
    "metadata": p.metadata,
}
OUT["build_pipeline_errors"] = {
    "bad_item": call(lambda: PipelineBuilder().build_pipeline({"steps": ["x"]})),
    "none_steps": call(lambda: PipelineBuilder().build_pipeline({"steps": None})),
    "bad_config": call(lambda: PipelineBuilder().build_pipeline({"steps": [{"name": "a", "type": "t", "config": None}]})),
    "default_name": PipelineBuilder().build_pipeline({"steps": [{"name": "a", "type": "t"}]}).name,
}

b = PipelineBuilder()
b.register_step_handler("t", lambda d, **k: d)
OUT["register_step_handler"] = list(b.step_registry)
OUT["get_step"] = [PipelineBuilder().get_step("x") is None]


def h(data, **kw):
    return data


b = PipelineBuilder()
b.add_step("s", "t", handler=h)
OUT["serialize_handler"] = call(lambda: b.serialize())
OUT["builder_validate_pipeline"] = call(lambda: b.validate_pipeline())

# ---------------------------------------------------------------- serializer
b = PipelineBuilder()
b.add_step("s1", "t1", source="docs/", delta_mode=True, base_version_id="v1", target_version_id="v2")
b.add_step("s2", "t2", dependencies=["s1"], weight=0.5)
b.add_step("s3", "t3")
b.connect_steps("s1", "s3")
b.set_parallelism(2)
p = b.build("ser")
ser = PipelineSerializer()
text = ser.serialize_pipeline(p)
d = ser.serialize_pipeline(p, format="dict")
back = ser.deserialize_pipeline(text)
back_d = ser.deserialize_pipeline(d)
OUT["serializer"] = {
    "json": text,
    "dict": d,
    "back": {
        "name": back.name,
        "steps": [[st.name, st.step_type, st.config, st.dependencies, st.delta_mode] for st in back.steps],
        "config": back.config,
        "metadata": back.metadata,
    },
    "back_from_dict_names": [st.name for st in back_d.steps],
    "invalid_json": call(lambda: ser.deserialize_pipeline("{not json")),
    "json_list": call(lambda: ser.deserialize_pipeline("[1, 2]")),
}
v = ser.version_pipeline(p, {"version": "2.0", "author": "me"})
OUT["serializer"]["versioned"] = {"same": v is p, "metadata": v.metadata}
OUT["serializer"]["versioned_default"] = ser.version_pipeline(Pipeline("q"), {}).metadata
b = PipelineBuilder()
b.add_step("s", "t", handler=h)
text = PipelineSerializer().serialize_pipeline(b.build("h"))
OUT["serializer"]["handler_json_has_function"] = '"handler": "<function h at 0x' in text

# ---------------------------------------------------------------- validator
val = PipelineValidator()


def vres(r):
    return {"valid": r.valid, "errors": r.errors, "warnings": r.warnings, "metadata": r.metadata}


cases = {}
b = PipelineBuilder()
b.add_step("a", "t")
b.add_step("b", "t", dependencies=["a"])
cases["builder_valid"] = vres(val.validate(b))
cases["builder_empty"] = vres(val.validate(PipelineBuilder()))
b = PipelineBuilder()
b.add_step("a", "t")
b.add_step("a", "t")
b.add_step("b", "t")
b.add_step("b", "t")
cases["builder_duplicates"] = vres(val.validate(b))
b = PipelineBuilder()
b.add_step("", "")
b.add_step("x", "")
cases["builder_blank"] = vres(val.validate(b))
b = PipelineBuilder()
b.add_step("a", "t")
st = b.add_step("b", "t")
st.dependencies.append("NON_EXISTENT")
cases["builder_missing"] = vres(val.validate(b))
b = PipelineBuilder()
b.add_step("a", "t", dependencies=["c"])
b.add_step("b", "t", dependencies=["a"])
b.add_step("c", "t", dependencies=["b"])
b.add_step("d", "t", dependencies=["d"])
cases["builder_cycles"] = vres(val.validate(b))
b = PipelineBuilder()
b.add_step("root", "t")
b.add_step("x", "t", dependencies=["y"])
b.add_step("y", "t", dependencies=["x"])
cases["builder_cycle_unreachable"] = vres(val.validate(b))
p = Pipeline("p", steps=[PipelineStep("a", "t"), PipelineStep("b", "t", config={"k": 1}, dependencies=["a"], handler=h)])
cases["pipeline_warnings"] = vres(val.validate_pipeline(p))
p = Pipeline("p", steps=[PipelineStep("", "", dependencies=["zz"])])
cases["pipeline_errors"] = vres(val.validate_pipeline(p))
cases["path"] = vres(val.validate("pipeline.yaml"))
OUT["validator"] = cases

ct_step = PipelineStep("build_foaf", "construct_template", config={"template_name": "person_to_foaf", "params": {"a": 1}})
OUT["construct_template"] = {"no_registry": vres(val.validate_pipeline(Pipeline("p", steps=[ct_step])))}


class _Param:
    def __init__(self, name, required):
        self.name = name
        self.required = required


class _Template:
    def __init__(self, params):
        self.parameters = params


class _Registry:
    def __init__(self, templates):
        self.templates = templates

    def get(self, name):
        return self.templates.get(name)


reg = _Registry({"person_to_foaf": _Template([_Param("a", True), _Param("b", True), _Param("c", False)])})
OUT["construct_template"]["missing_params"] = vres(
    val.validate_pipeline(Pipeline("p", steps=[ct_step]), construct_template_registry=reg)
)
OUT["construct_template"]["unregistered"] = vres(
    val.validate_pipeline(
        Pipeline("p", steps=[PipelineStep("s", "construct_template", config={"template_name": "nope"})]),
        construct_template_registry=reg,
    )
)
OUT["construct_template"]["no_template_name"] = vres(
    val.validate_pipeline(
        Pipeline("p", steps=[PipelineStep("s", "construct_template", config={"x": 1})]),
        construct_template_registry=reg,
    )
)
ok_step = PipelineStep("s", "construct_template", config={"template_name": "person_to_foaf", "params": {"a": 1, "b": 2}}, handler=h)
OUT["construct_template"]["valid"] = vres(
    val.validate_pipeline(Pipeline("p", steps=[ok_step]), construct_template_registry=reg)
)
OUT["validate_step"] = {
    "allow_no_handler": vres(val.validate_step(PipelineStep("a", "t"), allow_no_handler=True)),
    "default": vres(val.validate_step(PipelineStep("a", "t"))),
}
b = PipelineBuilder()
b.add_step("a", "t")
b.add_step("b", "t", dependencies=["a", "ghost"])
b.add_step("c", "t", dependencies=["c"])
dep = val.check_dependencies(b)
OUT["check_dependencies"] = {
    "errors": dep["errors"],
    "warnings": dep["warnings"],
    "circular_dependencies": dep["circular_dependencies"],
    "reachable_steps": sorted(dep["reachable_steps"]),
    "keys": list(dep),
}
b = PipelineBuilder()
b.add_step("a", "t")
b.add_step("b", "t", dependencies=["a"])
b.add_step("c", "t", dependencies=["b"])
b.add_step("d", "t", dependencies=["a"])
OUT["check_dependencies"]["reachable_chain"] = sorted(val.check_dependencies(b)["reachable_steps"])
perf = {}
b = PipelineBuilder()
b.add_step("s1", "t1")
perf["single"] = val.validate_performance(b.build())
b = PipelineBuilder()
b.add_step("a", "t", dependencies=["b"])
b.add_step("b", "t", dependencies=["a"])
perf["all_sequential"] = val.validate_performance(Pipeline("p", steps=b.steps))
perf["empty"] = val.validate_performance(Pipeline("p"))
perf["many"] = val.validate_performance(Pipeline("p", steps=[PipelineStep(f"s{i}", "t") for i in range(101)]))
perf["not_a_pipeline"] = call(lambda: val.validate_performance("x"))
OUT["validate_performance"] = perf

# ---------------------------------------------------------------- execution engine
eng = {}


def run(engine, pipeline, data, **options):
    r = engine.execute_pipeline(pipeline, data, **options)
    return {
        "success": r.success,
        "output": r.output,
        "errors": r.errors,
        "metrics_keys": list(r.metrics),
        "metrics": {k: v for k, v in r.metrics.items() if k != "execution_time"},
        "metadata_keys": list(r.metadata),
        "metadata_pipeline_id": r.metadata.get("pipeline_id"),
    }


calls = []


def rec(name):
    def handler(data, **kw):
        calls.append([name, list(kw)])
        return (data or []) + [name]
    return handler


b = PipelineBuilder()
b.add_step("c", "tc", handler=rec("c"), dependencies=["b"], extra=1)
b.add_step("a", "ta", handler=rec("a"))
b.add_step("b", "", handler=rec("b"), dependencies=["a"])
b.add_step("d", "td")
p = Pipeline("order", steps=list(b.steps))
e = ExecutionEngine()
eng["order"] = run(e, p, [], cpu_cores=1, flag=True)
eng["order"]["calls"] = calls[:]
eng["order"]["steps"] = [step_state(st) for st in p.steps]
eng["order"]["progress"] = e.get_progress("order")
eng["order"]["status"] = e.get_pipeline_status("order").value
eng["order"]["builder_shares_steps"] = b.steps[0].status.value

calls.clear()
b = PipelineBuilder()
for name, deps in [("D", ["B", "C"]), ("C", ["A"]), ("B", ["A"]), ("A", [])]:
    b.add_step(name, "t", handler=rec(name), dependencies=deps)
eng["diamond"] = run(ExecutionEngine(), b.build("diamond"), None)
eng["diamond"]["calls"] = [c[0] for c in calls]

e = ExecutionEngine()
eng["empty"] = run(e, Pipeline("empty"), {"x": 1})
eng["empty"]["progress"] = e.get_progress("empty")
eng["unknown_progress"] = e.get_progress("nope")
e.pause_pipeline("nope")
e.resume_pipeline("nope")
e.stop_pipeline("nope")
eng["unknown_status"] = e.get_pipeline_status("nope")


def failing(data, **kw):
    raise ValueError("Oops")


e = ExecutionEngine()
e.failure_handler.set_retry_policy("type1", RetryPolicy(max_retries=0))
b = PipelineBuilder()
st = b.add_step("step1", "type1", handler=failing)
p = b.build("fail")
eng["failure"] = run(e, p, {})
eng["failure"]["step"] = step_state(st)
eng["failure"]["status"] = e.get_pipeline_status("fail").value
eng["failure"]["history"] = history(e.failure_handler)
eng["failure"]["progress"] = e.get_progress("fail")

attempts = {"n": 0}


def flaky(data, **kw):
    attempts["n"] += 1
    if attempts["n"] < 3:
        raise ValueError(f"Fail {attempts['n']}")
    return "Success"


e = ExecutionEngine()
e.failure_handler.set_retry_policy("flaky", RetryPolicy(max_retries=3, strategy=RetryStrategy.FIXED, initial_delay=0.0))
b = PipelineBuilder()
st = b.add_step("flaky", "flaky", handler=flaky)
eng["retry_success"] = run(e, b.build("retry"), {})
eng["retry_success"]["attempts"] = attempts["n"]
eng["retry_success"]["history"] = history(e.failure_handler)
eng["retry_success"]["step"] = step_state(st)

attempts["n"] = 0


def always(data, **kw):
    attempts["n"] += 1
    raise RuntimeError(f"attempt {attempts['n']}")


e = ExecutionEngine()
e.failure_handler.set_retry_policy("t", RetryPolicy(max_retries=2, strategy=RetryStrategy.FIXED, initial_delay=0.0))
b = PipelineBuilder()
b.add_step("s", "t", handler=always)
eng["retry_exhausted"] = run(e, b.build("exhausted"), {})
eng["retry_exhausted"]["attempts"] = attempts["n"]
eng["retry_exhausted"]["history"] = history(e.failure_handler)

attempts["n"] = 0
e = ExecutionEngine()
e.failure_handler.set_retry_policy("t", RetryPolicy(max_retries=3, initial_delay=0.0, retryable_errors=[KeyError]))
b = PipelineBuilder()
b.add_step("s", "t", handler=always)
eng["not_retryable"] = run(e, b.build("nr"), {})
eng["not_retryable"]["attempts"] = attempts["n"]
eng["not_retryable"]["history"] = history(e.failure_handler)


def kw_handler(data, **kw):
    return list(kw)


e = ExecutionEngine()
e.failure_handler.set_retry_policy("t", RetryPolicy(max_retries=2, strategy=RetryStrategy.FIXED, initial_delay=0.0))
b = PipelineBuilder()
st = b.add_step("s", "t", handler=kw_handler, cpu_cores=2)
eng["duplicate_kwargs"] = run(e, b.build("dup"), {}, cpu_cores=1)
eng["duplicate_kwargs"]["history_errors"] = [[r["error_type"], r["error"]] for r in e.failure_handler.get_error_history()]
eng["duplicate_kwargs"]["step_error"] = err(st.error)

b = PipelineBuilder()
b.add_step("a", "t", dependencies=["b"])
b.add_step("b", "t", dependencies=["a"])
e = ExecutionEngine()
eng["cycle"] = run(e, Pipeline("cyc", steps=b.steps), None)
eng["cycle"]["status"] = e.get_pipeline_status("cyc").value
eng["missing_dep"] = run(ExecutionEngine(), Pipeline("md", steps=[PipelineStep("a", "t", dependencies=["zz"])]), None)
eng["no_resources"] = run(ExecutionEngine(), Pipeline("nores", steps=[PipelineStep("a", "t")]), None,
                          cpu_cores=10 ** 6, memory_gb=10 ** 9)

e = ExecutionEngine()


def stopper(data, **kw):
    e.stop_pipeline("stop")
    return "s1"


b = PipelineBuilder()
s1 = b.add_step("s1", "t", handler=stopper)
s2 = b.add_step("s2", "t", handler=rec("s2"), dependencies=["s1"])
p = b.build("stop")
eng["stop"] = run(e, p, None)
eng["stop"]["steps"] = [step_state(s1), step_state(s2)]
eng["stop"]["status"] = e.get_pipeline_status("stop").value
eng["stop"]["progress"] = e.get_progress("stop")

e = ExecutionEngine()
timeline = []


def pauser(data, **kw):
    e.pause_pipeline("pause")
    timeline.append(["paused", e.get_pipeline_status("pause").value])
    threading.Timer(0.3, lambda: (timeline.append(["resume"]), e.resume_pipeline("pause"))).start()
    return 1


def after(data, **kw):
    timeline.append(["s2", e.get_pipeline_status("pause").value])
    return data + 1


b = PipelineBuilder()
b.add_step("s1", "t", handler=pauser)
b.add_step("s2", "t", handler=after, dependencies=["s1"])
eng["pause"] = run(e, b.build("pause"), None)
eng["pause"]["timeline"] = timeline
eng["pause"]["status"] = e.get_pipeline_status("pause").value


class VM:
    def __init__(self, snaps):
        self.snaps = snaps
        self.calls = []

    def get_version(self, v):
        self.calls.append(v)
        return self.snaps.get(v)


class TS:
    def __init__(self):
        self.calls = []

    def compute_delta(self, a, b, **options):
        self.calls.append([a, b, sorted(options)])
        return {"old_graph_uri": a, "new_graph_uri": b, "added_count": 1}


def delta_case(step_kwargs, snaps, with_vm=True, with_ts=True, engine_config=None, **options):
    vm, ts = VM(snaps), TS()
    if with_vm:
        options["version_manager"] = vm
    if with_ts:
        options["triplet_store"] = ts
    b = PipelineBuilder()
    b.add_step("delta", "validation", handler=lambda data, **kw: data, delta_mode=True, **step_kwargs)
    e = ExecutionEngine(**(engine_config or {}))
    e.failure_handler.set_retry_policy("validation", RetryPolicy(max_retries=0))
    out = run(e, b.build("delta"), {"full_graph": "huge"}, **options)
    out["vm_calls"] = vm.calls
    out["ts_calls"] = ts.calls
    return out


SNAPS = {"v1": {"graph_uri": "urn:graph:v1"}, "v2": {"graph_uri": "urn:graph:v2"}}
eng["delta"] = {
    "ok": delta_case({"base_version_id": "v1", "target_version_id": "v2"}, SNAPS),
    "extra_option": delta_case({"base_version_id": "v1", "target_version_id": "v2"}, SNAPS, flag=1),
    "no_manager": delta_case({"base_version_id": "v1", "target_version_id": "v2"}, SNAPS, with_vm=False),
    "no_ids": delta_case({"base_version_id": "v1"}, SNAPS),
    "base_missing": delta_case({"base_version_id": "v9", "target_version_id": "v2"}, SNAPS),
    "target_missing": delta_case({"base_version_id": "v1", "target_version_id": "v9"}, SNAPS),
    "no_uri": delta_case({"base_version_id": "v1", "target_version_id": "v3"},
                         dict(SNAPS, v3={"graph_uri": ""})),
}
vm_cfg, ts_cfg = VM(SNAPS), TS()
b = PipelineBuilder()
b.add_step("delta", "validation", delta_mode=True, base_version_id="v1", target_version_id="v2")
e = ExecutionEngine(version_manager=vm_cfg, triplet_store=ts_cfg)
eng["delta"]["from_config"] = run(e, b.build("delta_cfg"), None)
eng["delta"]["from_config"]["ts_calls"] = ts_cfg.calls

pt = ProgressTracker()
pt.track_progress("p", "a", 50.0)
pt.track_progress("p", "b", 100.0)
pt.track_progress("p", "a", 20.0)
eng["progress_tracker"] = {
    "completion": pt.get_completion_percentage("p"),
    "unknown": pt.get_completion_percentage("q"),
    "steps": list(pt.tracking_data["p"]),
    "record_keys": list(pt.tracking_data["p"]["a"]),
    "remaining_none": pt.estimate_remaining_time("q", time.time()),
    "remaining_clamped": pt.estimate_remaining_time("p", time.time() - 10),
}
pt2 = ProgressTracker()
pt2.track_progress("p", "a", 0.5)
remaining = pt2.estimate_remaining_time("p", time.time() - 10)
eng["progress_tracker"]["remaining_half_close_to_10"] = abs(remaining - 10) < 1
OUT["engine"] = eng

# ---------------------------------------------------------------- failure handler
fh = FailureHandler()
errors = {
    "value": ValueError("Something wrong"),
    "validation": ValidationError("bad input"),
    "processing": ProcessingError("broken"),
    "timeout": RuntimeError("Connection timeout"),
    "memory": RuntimeError("Out of Memory"),
    "resource": OSError("resource exhausted"),
    "key": KeyError("k"),
}
OUT["classify"] = {k: classification(fh.classify_error(v)) for k, v in errors.items()}
fh = FailureHandler()
fh.set_retry_policy("lin", RetryPolicy(strategy=RetryStrategy.LINEAR, initial_delay=0.5))
fh.set_retry_policy("fix", RetryPolicy(strategy=RetryStrategy.FIXED, initial_delay=0.0))
fh.set_retry_policy("cap", RetryPolicy(initial_delay=100.0, max_delay=7.5))
fh.set_retry_policy("only_key", RetryPolicy(retryable_errors=[KeyError]))
fh.set_retry_policy("only_lookup", RetryPolicy(retryable_errors=[LookupError]))
hsf = {}
for step_type, error in [("lin", ValueError("v")), ("fix", ValidationError("x")), ("cap", ProcessingError("p")),
                         ("only_key", ValueError("v")), ("only_lookup", KeyError("k")), ("default", RuntimeError("Connection reset"))]:
    r = fh.handle_step_failure(PipelineStep(f"step_{step_type}", step_type), error)
    hsf[step_type] = {
        "retry": r["retry"],
        "retry_delay": r["retry_delay"],
        "error_classification": classification(r["error_classification"]),
        "recovery_action": r["recovery_action"],
        "keys": list(r),
    }
OUT["handle_step_failure"] = hsf
OUT["error_history"] = {
    "all": history(fh),
    "filtered": [r["step_name"] for r in fh.get_error_history("step_lin")],
    "empty_filter": len(fh.get_error_history("")),
}
fh.clear_error_history()
OUT["error_history"]["cleared"] = fh.get_error_history()
OUT["policies"] = {
    "default": repr(fh.get_retry_policy("unknown")),
    "custom_defaults": repr(FailureHandler(default_max_retries=5, default_backoff_factor=3.0).get_retry_policy("x")),
    "config_dict": repr(FailureHandler(config={"default_max_retries": 7}).get_retry_policy("x")),
    "set": repr(fh.get_retry_policy("lin")),
}
hf = {}
for strategy in RetryStrategy:
    policy = RetryPolicy(max_retries=3, strategy=strategy, backoff_factor=2.0, initial_delay=1.5, max_delay=5.0)
    hf[strategy.value] = [[fh.handle_failure(ValueError("x"), policy, retry_count=n).should_retry,
                           fh.handle_failure(ValueError("x"), policy, retry_count=n).retry_delay] for n in range(5)]
hf["filtered"] = [fh.handle_failure(ValueError("x"), RetryPolicy(retryable_errors=[KeyError])).should_retry,
                  fh.handle_failure(KeyError("x"), RetryPolicy(retryable_errors=[KeyError])).should_retry]
hf["default_count"] = fh.handle_failure(ValueError("x"), RetryPolicy()).retry_delay
OUT["handle_failure"] = hf

fh = FailureHandler()
fh.set_retry_policy("fix", RetryPolicy(strategy=RetryStrategy.FIXED, initial_delay=0.0))
fh.set_retry_policy("never", RetryPolicy(retryable_errors=[KeyError]))
r = fh.retry_failed_step(PipelineStep("s", "fix"), ValueError("v"))
OUT["retry_failed_step"] = {
    "retry": r["retry"],
    "delay": r["retry_delay"],
    "action": r["recovery_action"],
    "not_retryable": call(lambda: fh.retry_failed_step(PipelineStep("s", "never"), ValueError("v"))),
    "history_len": len(fh.get_error_history()),
}
rh = RetryHandler()
rh2 = RetryHandler(max_retries=5, backoff_factor=1.5, default_extra=1)
rh.set_retry_policy("fix", RetryPolicy(max_retries=1, strategy=RetryStrategy.FIXED, initial_delay=0.0))
r = rh.retry_failed_step(PipelineStep("s", "fix"), ValueError("v"))
OUT["retry_handler"] = {
    "default_policy": repr(rh.failure_handler.get_retry_policy("x")),
    "custom_policy": repr(rh2.failure_handler.get_retry_policy("x")),
    "custom_config": rh2.failure_handler.config,
    "retry": [r["retry"], r["retry_delay"], r["recovery_action"]],
    "duplicate": call(lambda: RetryHandler(default_max_retries=1)),
    "duplicate_backoff": call(lambda: RetryHandler(max_retries=2, default_backoff_factor=1)),
}
fb = FallbackHandler(mode="x")
before = fb.handle_service_failure("db")
fb.set_fallback_strategy("use_cache")
fb.fallback_strategies["search"] = "skip"
OUT["fallback"] = {
    "before": before,
    "after": fb.handle_service_failure("db"),
    "specific": fb.handle_service_failure("search"),
    "switch": [fb.switch_to_backup(True), fb.switch_to_backup(False)],
    "strategies": fb.fallback_strategies,
    "config": fb.config,
}
er = ErrorRecovery()
OUT["error_recovery"] = {
    k: {
        "analyze": classification(er.analyze_error(v)),
        "recover_action": er.recover_from_error(v, {"step": "x"})["recovery_action"],
        "recover_keys": list(er.recover_from_error(v, {})),
    }
    for k, v in errors.items()
}

# ---------------------------------------------------------------- parallelism
par = {}
order = []
lock = threading.Lock()


def worker(x, tag=None):
    with lock:
        order.append(tag or x)
    if x == "boom":
        raise ValueError("boom!")
    return x * 2


pm = ParallelismManager(max_workers=1)
tasks = [
    Task("t1", worker, args=(1,), kwargs={"tag": "t1"}, priority=0),
    Task("t2", worker, args=(2,), kwargs={"tag": "t2"}, priority=5),
    Task("t3", worker, args=(3,), kwargs={"tag": "t3"}, priority=1),
    Task("t4", worker, args=(4,), kwargs={"tag": "t4"}, priority=5),
    Task("t5", worker, args=("boom",), kwargs={"tag": "t5"}, priority=-1),
]
res = pm.execute_parallel(tasks)
par["single_worker"] = {
    "order": order[:],
    "results": [[r.task_id, r.success, r.result, None if r.error is None else err(r.error)] for r in res],
}
pm2 = ParallelismManager(max_workers=3)
res = pm2.execute_parallel([Task(f"t{i}", worker, args=(i,)) for i in range(6)])
par["three_workers_sorted"] = sorted([[r.task_id, r.success, r.result] for r in res])
par["empty"] = ParallelismManager().execute_parallel([])
par["zero_workers"] = call(lambda: ParallelismManager(max_workers=0).execute_parallel([Task("t", worker, args=(1,))]))
par["option_zero_workers"] = call(lambda: ParallelismManager().execute_parallel([Task("t", worker, args=(1,))], max_workers=0))
par["defaults"] = {"max_workers": ParallelismManager().max_workers, "use_processes": ParallelismManager().use_processes,
                   "task": repr(Task("t", None)).replace("None", "None")}


def plus(data, **kw):
    return data + kw.get("inc", 0)


def boom(data, **kw):
    raise ValueError("boom")


steps = [PipelineStep("s1", "t", config={"inc": 1}, handler=plus), PipelineStep("s2", "t"),
         PipelineStep("s3", "t", config={"inc": 10}, handler=plus)]
par["steps_parallel"] = ParallelismManager().execute_pipeline_steps_parallel(steps, 5)
par["steps_parallel_fail"] = call(lambda: ParallelismManager().execute_pipeline_steps_parallel(
    [PipelineStep("s1", "t", handler=plus), PipelineStep("s2", "t", handler=boom)], 5))
b = PipelineBuilder()
for name, deps in [("A", []), ("B", ["A"]), ("C", ["A"]), ("D", ["B", "C"]), ("E", []), ("F", ["E", "D"])]:
    b.add_step(name, "t", dependencies=deps)
p = b.build("levels")
par["groups"] = [[s.name for s in g] for g in ParallelismManager().identify_parallelizable_steps(p)]
par["optimize"] = ParallelismManager().optimize_parallel_execution(p, 2)
par["optimize_empty"] = ParallelismManager().optimize_parallel_execution(Pipeline("e"), 4)
par["missing_dep"] = call(lambda: ParallelismManager().identify_parallelizable_steps(
    Pipeline("m", steps=[PipelineStep("a", "t", dependencies=["ghost"])])))
par["cycle"] = call(lambda: ParallelismManager().identify_parallelizable_steps(
    Pipeline("c", steps=[PipelineStep("a", "t", dependencies=["b"]), PipelineStep("b", "t", dependencies=["a"])])))
pe = ParallelExecutor(max_workers=2)
res = pe.execute_parallel([Task("a", worker, args=(1,)), Task("b", worker, args=(2,))])
par["executor"] = {
    "max_workers": pe.parallelism_manager.max_workers,
    "results": sorted([[r.task_id, r.result] for r in res]),
    "max_workers_from_kwargs": ParallelExecutor(**{"max_workers": 3}).parallelism_manager.max_workers,
    "default_workers": ParallelExecutor().parallelism_manager.max_workers,
}
OUT["parallel"] = par

# ---------------------------------------------------------------- resource scheduler
rs = ResourceScheduler()
OUT["resources_structure"] = {
    rid: [r.resource_type.value, sorted(r.metadata)] for rid, r in rs.resources.items()
}
p = Pipeline("test_pipe")
alloc = rs.allocate_resources(p, cpu_cores=1, memory_gb=0.25, gpu_device=0)
usage = rs.get_resource_usage()
res = {
    "keys": list(alloc),
    "allocations": {
        k: [a.resource_id, a.resource_type.value, a.amount, a.pipeline_id, a.step_name, a.metadata,
            a.allocation_id.rsplit("_", 1)[0]]
        for k, a in alloc.items()
    },
    "usage_keys": list(usage),
    "usage_entry_keys": list(usage["cpu"]),
    "cpu_allocated": usage["cpu"]["allocated"],
    "gpu_usage": usage["gpu_0"],
    "gpu_resource": [rs.resources["gpu_0"].capacity, rs.resources["gpu_0"].metadata],
    "live_allocations": len(rs.allocations),
}
res["second_gpu"] = rs.allocate_gpu(0, "other") is None
rs.release_resources(alloc)
rs.release_resources(alloc)
usage = rs.get_resource_usage()
res["after_release"] = [usage["cpu"]["allocated"], usage["memory"]["allocated"], usage["gpu_0"]["allocated"],
                        len(rs.allocations)]
res["cpu_too_much"] = rs.allocate_cpu(10 ** 6, "p") is None
res["memory_too_much"] = rs.allocate_memory(10 ** 9, "p") is None
a = rs.allocate_cpu(1, "p", step_name="s")
res["step_allocation_prefix"] = a.allocation_id.rsplit("_", 1)[0]
rs.release_resources({"cpu": a})
res["type_error"] = call(lambda: rs.allocate_resources(p, cpu_cores="2"))
res["memory_type_error_releases_cpu"] = [call(lambda: rs.allocate_resources(p, cpu_cores=1, memory_gb=None))["error"],
                                         rs.get_resource_usage()["cpu"]["allocated"], len(rs.allocations)]
rs0 = ResourceScheduler()
for r in rs0.resources.values():
    r.capacity = 0.0
res["no_resources"] = call(lambda: rs0.allocate_resources(p))
res["zero_capacity_usage"] = rs0.get_resource_usage()["cpu"]
rs1 = ResourceScheduler()
rs1.resources["cpu"].capacity = 1.0
opt = {}
for n in (0, 1, 3):
    opt[str(n)] = rs1.optimize_resource_allocation(Pipeline("p", steps=[PipelineStep(f"s{i}", "t") for i in range(n)]))["recommendations"]
del rs1.resources["cpu"]
opt["no_cpu"] = rs1.optimize_resource_allocation(Pipeline("p", steps=[PipelineStep("a", "t"), PipelineStep("b", "t")]))["recommendations"]
opt["keys"] = list(rs.optimize_resource_allocation(Pipeline("p")))
res["optimize"] = opt
rs2 = ResourceScheduler()
rs2.resources["cpu"].capacity = 2.0
res["exact_fit"] = rs2.allocate_cpu(2, "p") is not None
res["exact_fit_then_none"] = rs2.allocate_cpu(0.5, "p") is None
OUT["scheduler"] = res

# ---------------------------------------------------------------- templates
tm = PipelineTemplateManager()
tpl = {
    "list": tm.list_templates(),
    "by_category": tm.list_templates("rag"),
    "unknown_category": tm.list_templates("nope"),
    "info": {name: tm.get_template_info(name) for name in tm.list_templates()},
    "steps": {name: tm.get_template(name).steps for name in tm.list_templates()},
    "missing_info": tm.get_template_info("nope"),
    "missing": call(lambda: tm.create_pipeline_from_template("nope")),
}
builder = tm.create_pipeline_from_template("document_processing", pipeline_config={"parallelism": 5},
                                           ingest={"source": "custom_source"}, normalize={"lower": True})
p = builder.build()
tpl["created"] = {
    "pipeline_config": p.config,
    "metadata": p.metadata,
    "steps": [[s.name, s.step_type, s.config, s.dependencies] for s in p.steps],
    "template_after": tm.get_template("document_processing").steps[:3],
}
builder = tm.create_pipeline_from_template("rag_pipeline")
tpl["created_plain"] = [[s.name, s.config, s.dependencies] for s in builder.steps] + [builder.pipeline_config]
tm.register_template(PipelineTemplate(name="custom", description="Custom", steps=[{"name": "step1", "type": "test"}]))
tpl["registered"] = tm.get_template_info("custom")
tpl["registered_build"] = [[s.name, s.config, s.dependencies] for s in tm.create_pipeline_from_template("custom").steps]
tm.register_template(PipelineTemplate(name="bad", description="", steps=[{"type": "x"}]))
tpl["missing_name"] = call(lambda: tm.create_pipeline_from_template("bad"))
tm.register_template(PipelineTemplate(name="dupdeps", description="", steps=[{"name": "a", "type": "x", "config": {"dependencies": []}}]))
tpl["duplicate_dependencies"] = call(lambda: tm.create_pipeline_from_template("dupdeps"))
OUT["templates"] = tpl

# ---------------------------------------------------------------- provenance
from semantica.pipeline.pipeline_provenance import PipelineWithProvenance  # noqa: E402

b = PipelineBuilder()
b.add_step("ingest", "file_ingest")
b.add_step("parse", "document_parse")
pipe = b.build("test_provenance_pipeline")
runner = PipelineWithProvenance(pipe, provenance=True, agent_id="agent-7", is_automated=False)
r = runner.run(data={"k": "v"}, source="unit-test")
entries = runner._prov_manager.storage.retrieve_all()
entry = entries[0]
OUT["provenance"] = {
    "provenance": runner.provenance,
    "success": r.success,
    "entries": len(entries),
    "entity_id_prefix": entry.entity_id[:9],
    "entity_id_len": len(entry.entity_id),
    "entity_type": entry.entity_type,
    "agent_id": entry.agent_id,
    "agent_type": entry.agent_type,
    "is_automated": entry.is_automated,
    "activity_is_entity": entry.activity_id == entry.entity_id,
    "source_document": entry.source_document,
    "metadata_keys": sorted(entry.metadata),
    "metadata_steps": entry.metadata["steps"],
    "metadata_status": entry.metadata["status"],
    "started_suffix": entry.activity_started_at_time[-6:],
    "default_agent": PipelineWithProvenance(pipe)._agent_id,
}
fail_b = PipelineBuilder()
fail_b.add_step("x", "t", dependencies=["ghost"])
runner = PipelineWithProvenance(Pipeline("broken", steps=fail_b.steps), provenance=True)
r = runner.run()
entry = runner._prov_manager.storage.retrieve_all()[0]
OUT["provenance"]["failed_run"] = [r.success, entry.metadata["status"], entry.source_document, entry.agent_id,
                                   entry.is_automated]
OUT["provenance"]["disabled"] = [PipelineWithProvenance(pipe).provenance, PipelineWithProvenance(pipe)._prov_manager is None]

with open(sys.argv[1], "w", encoding="utf-8") as fh_out:
    json.dump(OUT, fh_out, indent=1, sort_keys=False, default=repr)
    fh_out.write("\n")
