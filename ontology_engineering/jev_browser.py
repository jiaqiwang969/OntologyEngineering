"""Optional, pinned Jev Ultrafast browser tool. No engineering goal controller.

The upstream policy selects observed controls. This adapter supplies exact field
values, reuses the existing private Jev transport, and records bounded outcomes.
Model DONE is always reported as awaiting independent verification.
"""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import sys
import time

from .jev_transport import ENDPOINT, JevTransport, pinned_model, resolve_credential_file, validate_response

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "runtime/jev-ultrafast"
TASK_SCHEMA = "ontology-engineering.jev-browser-task/v1"


class FieldValueRequired(ValueError):
    def __init__(self, field):
        self.field = field
        super().__init__("exact_field_value_required")


class BrowserConnectionRequired(RuntimeError):
    """No existing browser connection; no automatic launch or settings change."""


def source_identity(runtime=RUNTIME):
    lock = json.loads((runtime / "source-lock.json").read_text())
    if lock.get("schema") != "ontology-engineering.jev-browser-source/v1":
        raise ValueError("invalid_browser_source_lock")
    for item in lock["files"]:
        name = Path(item["path"])
        if name.is_absolute() or ".." in name.parts:
            raise ValueError("invalid_browser_source_path")
        path = runtime / "upstream" / name
        if path.is_symlink() or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
            raise ValueError("browser_source_mismatch")
    requirements = runtime / "requirements.txt"
    if hashlib.sha256(requirements.read_bytes()).hexdigest() != lock["requirements_sha256"]:
        raise ValueError("browser_dependency_lock_mismatch")
    return {"repository": lock["repository"], "commit": lock["commit"],
            "lock_sha256": hashlib.sha256((runtime / "source-lock.json").read_bytes()).hexdigest()}


def doctor():
    """Check local files/dependencies without importing or connecting a browser."""
    identity = source_identity()
    dependencies = {}
    for name, expected in (("browser-harness", "0.1.13"), ("httpx", "0.28.1")):
        try:
            actual = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            actual = None
        dependencies[name] = {"expected": expected, "installed": actual, "matches": actual == expected}
    ready = sys.version_info >= (3, 12) and all(v["matches"] for v in dependencies.values())
    return {"status": "local_runtime_ready" if ready else "setup_required", "source": identity,
            "dependencies": dependencies, "browser_connection": "not_checked",
            "website_account": "not_checked", "live_task": "not_run"}


def validate_task(document):
    allowed = {"schema", "goal", "url", "text_values", "max_actions", "max_decisions",
               "max_seconds", "keep_open", "required_initial_text", "backend", "tab_id", "allowed_actions",
               "custom_click_selectors"}
    if not isinstance(document, dict) or set(document) - allowed or document.get("schema") != TASK_SCHEMA:
        raise ValueError("invalid_browser_task")
    from urllib.parse import urlsplit
    if not isinstance(document.get("goal"), str) or not document["goal"].strip():
        raise ValueError("browser_goal_required")
    url = urlsplit(document.get("url", ""))
    if url.scheme not in {"http", "https"} or not url.hostname or url.username or url.password:
        raise ValueError("http_url_without_credentials_required")
    task = {"backend": "browser_harness", "max_actions": 30, "max_decisions": 60, "max_seconds": 120,
            "keep_open": False, "text_values": {}, "required_initial_text": [], **document}
    for key, ceiling in (("max_actions", 60), ("max_decisions", 120), ("max_seconds", 3600)):
        if type(task[key]) is not int or not 1 <= task[key] <= ceiling:
            raise ValueError("invalid_browser_budget")
    if type(task["keep_open"]) is not bool:
        raise ValueError("invalid_keep_open")
    if task["backend"] not in {"browser_harness", "chrome_apple_events"}:
        raise ValueError("unsupported_browser_backend")
    if task["backend"] == "chrome_apple_events":
        if type(task.get("tab_id")) is not int or task["tab_id"] <= 0 or not task["keep_open"]:
            raise ValueError("existing_tab_id_and_keep_open_required")
        rules = task.get("allowed_actions")
        if not isinstance(rules, list) or not rules or any(
            not isinstance(r, dict) or set(r) != {"kind", "label"} or
            r["kind"] not in {"click", "select", "fill"} or
            not isinstance(r["label"], str) or not r["label"].strip() for r in rules):
            raise ValueError("observed_action_scope_required")
        selectors = task.get("custom_click_selectors", [])
        if not isinstance(selectors, list) or any(not isinstance(s, str) or not s.strip() for s in selectors):
            raise ValueError("invalid_observed_control_selectors")
    elif any(k in task for k in ("tab_id", "allowed_actions", "custom_click_selectors")):
        raise ValueError("existing_tab_options_require_apple_events_backend")
    values = task["text_values"]
    if not isinstance(values, dict) or any(not isinstance(k, str) or not k.strip() or
            not isinstance(v, str) or not v.strip() or len(v) > 2000 for k, v in values.items()):
        raise ValueError("invalid_exact_field_values")
    if not isinstance(task["required_initial_text"], list) or any(
            not isinstance(v, str) or not v.strip() for v in task["required_initial_text"]):
        raise ValueError("invalid_initial_observation_check")
    return task


def exact_field_text(values, context):
    label = context["field"]["label"]
    if label not in values:
        raise FieldValueRequired(label)
    return values[label], {"model": "caller-supplied-exact-value", "latency_ms": 0, "usage": {}}


@contextmanager
def upstream_binding(task, transport):
    """Bind the unmodified upstream to our transport, without exposing a key."""
    sys.path.insert(0, str(RUNTIME / "upstream"))
    from jev_ultrafast import agent as agent_module, model as model_module, browser as browser_module
    from browser_harness.admin import require_existing_daemon
    old_post, old_text = model_module.post_json, agent_module.field_text
    old_ensure = browser_module.ensure_daemon
    old_browser = agent_module.Browser
    names = ("TYPESAFE_API_KEY", "TYPESAFE_MODEL")
    previous = {name: os.environ.get(name) for name in names}
    model = pinned_model(json.loads((ROOT / "references/context-capabilities.json").read_text())["model"])

    def post(url, _unused_key, payload):
        if url != ENDPOINT or payload.get("model") != model:
            raise ValueError("unexpected_browser_model_endpoint_or_version")
        payload["state"]["provided_field_values"] = task["text_values"]
        operations = payload["questions"]["operation"]["criteria"]
        if "TYPE_TEXT" in operations:
            operations["TYPE_TEXT"] = "Fill an observed field with its caller-provided exact value; missing values stop for input."
        response = transport(payload)
        valid, errors = validate_response(response, payload)
        if "operation" not in valid:
            raise ValueError("invalid_browser_operation_answer")
        target = valid["operation"]["choice"].lower() + "_target"
        if target in payload["questions"] and target in errors:
            raise ValueError("invalid_browser_target_answer")
        return response

    model_module.post_json = post
    agent_module.field_text = lambda context: exact_field_text(task["text_values"], context)
    def existing_connection_only():
        try:
            require_existing_daemon()
        except Exception:
            raise BrowserConnectionRequired("existing_browser_connection_required") from None
    browser_module.ensure_daemon = existing_connection_only
    if task.get("backend") == "chrome_apple_events":
        from .chrome_apple_events import AppleEventsBrowser
        agent_module.Browser = lambda url: AppleEventsBrowser(url, task)
    os.environ["TYPESAFE_API_KEY"] = "delegated-to-private-transport"
    os.environ["TYPESAFE_MODEL"] = model
    try:
        yield agent_module.Agent
    finally:
        model_module.post_json, agent_module.field_text = old_post, old_text
        browser_module.ensure_daemon = old_ensure
        agent_module.Browser = old_browser
        for name, value in previous.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
        sys.path.remove(str(RUNTIME / "upstream"))


def write_json(path, value):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())


def execute(task, output, factory, identity):
    """Record a single bounded run; never retry an ambiguous browser mutation."""
    output = Path(output).expanduser().resolve()
    if output.is_relative_to(ROOT):
        raise ValueError("browser_records_must_be_in_private_task_workspace")
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    write_json(output / "task.json", task)
    started = time.monotonic()
    events = os.open(output / "events.jsonl", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    agent, result = None, {"status": "interrupted_review_required", "verification": "not_run",
                           "engineering_acceptance": "not_evaluated", "source": identity}
    with os.fdopen(events, "w") as journal:
        def event(value):
            journal.write(json.dumps({"elapsed_seconds": round(time.monotonic() - started, 3), **value}, ensure_ascii=False) + "\n")
            journal.flush()
            os.fsync(journal.fileno())

        try:
            event({"phase": "attaching_existing_tab" if task.get("backend") == "chrome_apple_events"
                   else "opening_owned_background_tab"})
            agent = factory(task["url"], task["goal"], screenshots=False)
            write_json(output / "session.json", {"target": agent.browser.target,
                                                  "session": agent.browser.session,
                                                  "backend": task.get("backend", "browser_harness"),
                                                  "owns_tab": getattr(agent.browser, "owns_tab", True)})
            page = agent.state["page"]
            observed = "\n".join((page.get("text", ""), page.get("title", "")))
            if not all(value in observed for value in task["required_initial_text"]):
                result["status"] = "initial_observation_mismatch"
            else:
                while agent.state["status"] not in {"done", "blocked"}:
                    if (len(agent.state["history"]) >= task["max_actions"] or
                            len(agent.state["decisions"]) >= task["max_decisions"] or
                            time.monotonic() - started >= task["max_seconds"]):
                        result["status"] = "budget_exhausted"
                        break
                    sequence = len(agent.state["decisions"]) + 1
                    # A durable intent survives a process interruption; do not auto-resume it.
                    event({"phase": "tick_started", "sequence": sequence})
                    state = agent.command("tick")
                    event({"phase": "tick_returned", "sequence": sequence,
                           "model_status": state["status"], "actions": len(state["history"])})
                else:
                    result["status"] = "reported_done" if agent.state["status"] == "done" else "blocked"
            result["verification"] = "required" if result["status"] == "reported_done" else "not_run"
        except BrowserConnectionRequired:
            result["status"] = "browser_connection_required"
        except FieldValueRequired as error:
            result.update(status="field_value_required", field=error.field)
        except Exception as error:
            # Provider/browser exceptions may contain page text; expose only their class.
            result.update(status="interrupted_review_required", error_type=type(error).__name__)
        finally:
            if agent is not None:
                try:
                    write_json(output / "observation.json", agent.snapshot())
                except Exception as error:
                    result.update(status="interrupted_review_required", verification="not_run",
                                  observation_error_type=type(error).__name__)
                result["actions"] = len(agent.state["history"])
                result["decisions"] = len(agent.state["decisions"])
                result["tab"] = "retained" if task["keep_open"] else "close_requested"
                if not task["keep_open"]:
                    try:
                        agent.close()
                        result["tab"] = "closed"
                    except Exception:
                        result["tab"] = "close_unconfirmed"
            result["elapsed_seconds"] = round(time.monotonic() - started, 3)
            event({"phase": "stopped", "status": result["status"]})
            write_json(output / "result.json", result)
    return result


def run(document, output, *, credential_file=None):
    task = validate_task(document)
    identity = source_identity()
    transport = JevTransport(resolve_credential_file(credential_file, skill_root=ROOT))
    with upstream_binding(task, transport) as factory:
        return execute(task, output, factory, identity)
