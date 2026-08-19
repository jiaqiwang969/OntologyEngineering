#!/usr/bin/env python3
"""Enforce Semantica as the only ontology execution backend.

The gate deliberately scans every active Python, Java, shell, and dependency
surface in the repository.  Audit mode permits only exact, documented migration
exceptions.  Strict mode permits no exception at all.
"""

from __future__ import annotations

import argparse
import ast
from dataclasses import dataclass, field
import fnmatch
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import stat
import sys
from typing import Any, Iterable, Iterator, Mapping, Sequence


SCHEMA_VERSION = "ontology-engineering.semantica-backend-policy/v1"
REQUIRED_BOOTSTRAP = "ontology_engineering/semantica_runtime.py"
DEFAULT_POLICY = "runtime/semantica-backend-policy.json"
PERMITTED_LITERAL_FIXTURE_HOSTS = frozenset(
    {
        "scripts/check_semantica_backend_policy.py",
        "tests/test_semantica_backend_policy.py",
    }
)

BACKEND_MODULES = frozenset({"rdflib", "pyshacl", "pyoxigraph", "owlready2"})
PROCESS_ENGINE_TOKENS = (
    "rdflib",
    "pyshacl",
    "pyoxigraph",
    "owlready2",
    "apache.jena",
    "jena",
    "fuseki",
    "pellet",
    "hermit",
    "konclude",
)

RULE_DIRECT_BACKEND_IMPORT = "direct_backend_import"
RULE_DIRECT_BACKEND_DEPENDENCY = "direct_backend_dependency"
RULE_DIRECT_SEMANTICA_IMPORT = "direct_semantica_import"
RULE_DYNAMIC_IMPORT = "dynamic_import"
RULE_PRIVATE_BACKEND = "private_backend_access"
RULE_SPARQL_REASONER = "forbidden_sparql_reasoner"
RULE_ALTERNATE_PROCESS = "alternate_engine_subprocess"
RULE_JENA_CLIENT = "jena_client"
RULE_DUPLICATE_SEMANTIC_ASSET = "duplicate_semantic_asset"
RULE_EMBEDDED_SEMANTIC_PAYLOAD = "embedded_semantic_payload"
RULE_PARSE_FAILURE = "parse_failure"
RULE_UNSAFE_SYMLINK = "unsafe_source_symlink"

KNOWN_RULES = frozenset(
    {
        RULE_DIRECT_BACKEND_IMPORT,
        RULE_DIRECT_BACKEND_DEPENDENCY,
        RULE_DIRECT_SEMANTICA_IMPORT,
        RULE_DYNAMIC_IMPORT,
        RULE_PRIVATE_BACKEND,
        RULE_SPARQL_REASONER,
        RULE_ALTERNATE_PROCESS,
        RULE_JENA_CLIENT,
        RULE_DUPLICATE_SEMANTIC_ASSET,
        RULE_EMBEDDED_SEMANTIC_PAYLOAD,
        RULE_PARSE_FAILURE,
        RULE_UNSAFE_SYMLINK,
    }
)
ALLOWLISTABLE_RULES = KNOWN_RULES - {
    RULE_EMBEDDED_SEMANTIC_PAYLOAD,
    RULE_PARSE_FAILURE,
    RULE_UNSAFE_SYMLINK,
}

# Generated/cache trees only.  The policy file cannot extend this list, so an
# allowlist author cannot hide vendor/, references/, demos/, or another active
# source tree behind a directory-level omission.
IGNORED_DIRECTORY_NAMES = frozenset(
    {
        ".git",
        ".hg",
        ".svn",
        ".mypy_cache",
        ".nox",
        ".pytest_cache",
        ".ruff_cache",
        ".tox",
        ".venv",
        "__pycache__",
        "node_modules",
    }
)

PYTHON_SUFFIXES = frozenset({".py", ".pyw"})
JAVA_SUFFIXES = frozenset({".java"})
SHELL_SUFFIXES = frozenset(
    {".sh", ".bash", ".zsh", ".command", ".ps1", ".psm1", ".bat", ".cmd"}
)
SEMANTIC_ASSET_SUFFIXES = frozenset(
    {
        ".jsonld",
        ".nq",
        ".nt",
        ".owl",
        ".rdf",
        ".rq",
        ".shacl",
        ".sparql",
        ".swrl",
        ".trig",
        ".trix",
        ".ttl",
    }
)
ACTIVE_SCRIPT_NAMES = frozenset(
    {
        "Dockerfile",
        "Makefile",
        "GNUmakefile",
        "Justfile",
        "Taskfile",
        "tox.ini",
        ".pre-commit-config.yaml",
        ".pre-commit-config.yml",
    }
)
MANIFEST_NAMES = frozenset(
    {
        "pyproject.toml",
        "setup.cfg",
        "Pipfile",
        "environment.yml",
        "environment.yaml",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
    }
)


@dataclass(frozen=True, order=True)
class Finding:
    path: str
    line: int
    rule: str
    detail: str


@dataclass(frozen=True)
class AllowEntry:
    path: str
    rules: frozenset[str]
    reason: str
    expires_when: str


@dataclass
class GateReport:
    mode: str
    bootstrap: str
    scanned_by_kind: dict[str, int]
    findings: list[Finding]
    allowed_findings: list[Finding]
    unapproved_findings: list[Finding]
    allowlist: list[AllowEntry]
    stale_allowances: list[str] = field(default_factory=list)
    policy_errors: list[str] = field(default_factory=list)
    fixture_hosts: list[str] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        if self.policy_errors or self.stale_allowances or self.unapproved_findings:
            return False
        if self.mode == "strict" and (self.findings or self.allowlist):
            return False
        return True

    @property
    def debt_file_count(self) -> int:
        return len({finding.path for finding in self.findings})


def _normalise_relative_path(raw: Any) -> str | None:
    if not isinstance(raw, str) or not raw.strip():
        return None
    raw = raw.replace("\\", "/")
    path = PurePosixPath(raw)
    if path.is_absolute() or ".." in path.parts or "." in path.parts:
        return None
    if any(character in raw for character in "*?[]{}"):
        return None
    return path.as_posix()


def _read_policy(policy_path: Path, root: Path) -> tuple[dict[str, Any], list[str]]:
    errors: list[str] = []
    try:
        raw = json.loads(policy_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}, [f"policy file does not exist: {policy_path}"]
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        return {}, [f"cannot read policy {policy_path}: {exc}"]
    if not isinstance(raw, dict):
        return {}, ["policy root must be a JSON object"]
    if raw.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION!r}")
    if raw.get("bootstrap") != REQUIRED_BOOTSTRAP:
        errors.append(
            f"bootstrap is immutable and must be {REQUIRED_BOOTSTRAP!r}; "
            "moving it would create a bypass"
        )
    if "excluded_directories" in raw or "exclude" in raw or "exclude_globs" in raw:
        errors.append("directory/glob exclusions are forbidden; use exact per-file allowances")

    fixture_hosts = raw.get("literal_fixture_hosts", [])
    if not isinstance(fixture_hosts, list):
        errors.append("literal_fixture_hosts must be a list")
    else:
        seen_fixture_paths: set[str] = set()
        for index, item in enumerate(fixture_hosts):
            if not isinstance(item, dict):
                errors.append(f"literal_fixture_hosts[{index}] must be an object")
                continue
            path = _normalise_relative_path(item.get("path"))
            if path is None:
                errors.append(f"literal_fixture_hosts[{index}].path must be an exact relative path")
                continue
            if path in seen_fixture_paths:
                errors.append(f"duplicate literal fixture host: {path}")
            seen_fixture_paths.add(path)
            if path not in PERMITTED_LITERAL_FIXTURE_HOSTS:
                errors.append(
                    f"literal fixture host is not gate infrastructure and cannot be excluded: {path}"
                )
            if item.get("scope") != "string_literals_only":
                errors.append(
                    f"literal fixture host {path} must use scope='string_literals_only'; "
                    "the file itself remains AST-scanned"
                )
            if not isinstance(item.get("reason"), str) or not item["reason"].strip():
                errors.append(f"literal fixture host {path} needs a non-empty reason")

    allowlist = raw.get("allowlist", [])
    if not isinstance(allowlist, list):
        errors.append("allowlist must be a list")
    else:
        seen_allowance_paths: set[str] = set()
        for index, item in enumerate(allowlist):
            if not isinstance(item, dict):
                errors.append(f"allowlist[{index}] must be an object")
                continue
            path = _normalise_relative_path(item.get("path"))
            if path is None:
                errors.append(f"allowlist[{index}].path must be an exact relative path without globs")
                continue
            if path in seen_allowance_paths:
                errors.append(f"duplicate allowlist path: {path}")
            seen_allowance_paths.add(path)
            candidate = root / path
            if not candidate.is_file():
                errors.append(f"allowlist path is missing (stale): {path}")
            rules = item.get("rules")
            if not isinstance(rules, list) or not rules or not all(isinstance(rule, str) for rule in rules):
                errors.append(f"allowlist entry {path} needs a non-empty rules list")
            else:
                unknown = sorted(set(rules) - ALLOWLISTABLE_RULES)
                if unknown:
                    errors.append(f"allowlist entry {path} has non-allowlistable rules: {', '.join(unknown)}")
                if len(rules) != len(set(rules)):
                    errors.append(f"allowlist entry {path} contains duplicate rules")
            for field_name in ("reason", "expires_when"):
                value = item.get(field_name)
                if not isinstance(value, str) or len(value.strip()) < 12:
                    errors.append(f"allowlist entry {path} needs a specific {field_name}")
    return raw, errors


def _allow_entries(policy: Mapping[str, Any]) -> list[AllowEntry]:
    entries: list[AllowEntry] = []
    for item in policy.get("allowlist", []):
        if not isinstance(item, dict):
            continue
        path = _normalise_relative_path(item.get("path"))
        rules = item.get("rules")
        if path is None or not isinstance(rules, list):
            continue
        entries.append(
            AllowEntry(
                path=path,
                rules=frozenset(rule for rule in rules if isinstance(rule, str)),
                reason=str(item.get("reason", "")),
                expires_when=str(item.get("expires_when", "")),
            )
        )
    return entries


def _classify_file(path: Path) -> str | None:
    name = path.name
    suffix = path.suffix.lower()
    if suffix in PYTHON_SUFFIXES:
        return "python"
    if suffix in JAVA_SUFFIXES:
        return "java"
    if suffix in SHELL_SUFFIXES:
        return "shell"
    if suffix in SEMANTIC_ASSET_SUFFIXES:
        return "semantic_asset"
    if name in ACTIVE_SCRIPT_NAMES:
        return "shell"
    posix = path.as_posix()
    if "/.github/workflows/" in posix and suffix in {".yml", ".yaml"}:
        return "shell"
    if name in MANIFEST_NAMES or fnmatch.fnmatch(name.lower(), "requirements*.txt"):
        return "manifest"
    try:
        mode = path.stat().st_mode
        should_probe = suffix == "" or bool(mode & stat.S_IXUSR)
        if should_probe:
            with path.open("rb") as handle:
                first_line = handle.readline(256).lower()
            if first_line.startswith(b"#!"):
                if b"python" in first_line:
                    return "python"
                if any(shell in first_line for shell in (b"sh", b"bash", b"zsh", b"dash", b"ksh")):
                    return "shell"
    except (OSError, UnicodeError):
        return None
    return None


def _discover_surfaces(root: Path) -> tuple[list[tuple[Path, str]], list[Finding]]:
    surfaces: list[tuple[Path, str]] = []
    findings: list[Finding] = []
    resolved_root = root.resolve()
    for directory, directory_names, file_names in os.walk(root, followlinks=False):
        for name in directory_names:
            candidate = Path(directory) / name
            if name not in IGNORED_DIRECTORY_NAMES and candidate.is_symlink():
                relative = candidate.relative_to(root).as_posix()
                findings.append(
                    Finding(
                        relative,
                        1,
                        RULE_UNSAFE_SYMLINK,
                        "active source directory symlink is not traversable by a closed-world gate",
                    )
                )
        directory_names[:] = sorted(
            name
            for name in directory_names
            if name not in IGNORED_DIRECTORY_NAMES and not (Path(directory) / name).is_symlink()
        )
        base = Path(directory)
        for name in sorted(file_names):
            path = base / name
            kind = _classify_file(path)
            if kind is None:
                continue
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                try:
                    path.resolve().relative_to(resolved_root)
                except (OSError, ValueError):
                    findings.append(
                        Finding(relative, 1, RULE_UNSAFE_SYMLINK, "active source symlink escapes repository")
                    )
                    continue
            surfaces.append((path, kind))
    return surfaces, findings


def _module_root(name: str | None) -> str:
    return (name or "").split(".", 1)[0]


def _semantic_payload_kind(value: str) -> str | None:
    """Classify executable semantic literals, not ordinary explanatory prose."""

    if re.search(r"(?im)^\s*@prefix\s+(?:[A-Za-z][\w-]*)?:\s*<[^>]+>\s*\.", value):
        return "RDF/Turtle prefix declaration"
    if re.search(
        r"(?is)\b(?:SELECT\s+(?:DISTINCT\s+)?\?|ASK\s*\{|CONSTRUCT\s*\{|"
        r"INSERT\s+(?:DATA\s*)?\{|DELETE\s+(?:DATA\s*|WHERE\s*)?\{)",
        value,
    ):
        return "SPARQL operation"
    if "http://www.w3.org/ns/shacl#" in value or re.search(
        r"(?m)\bsh:(?:NodeShape|PropertyShape|targetClass|sparql)\b", value
    ):
        return "SHACL graph"
    if re.search(r"(?is)<rdf:RDF\b", value) and "xmlns:rdf" in value:
        return "RDF/XML graph"
    if re.search(r"(?im)^\s*Class:\s*\S+", value) and re.search(
        r"(?im)^\s*(?:SubClassOf|EquivalentTo):", value
    ):
        return "Manchester OWL axioms"
    if re.search(r"(?im)^\s*IF\s+.+\s+THEN\s+.+$", value):
        return "forward-rule program"
    return None


class PythonScanner:
    def __init__(
        self,
        relative_path: str,
        bootstrap: str,
        source: str,
        *,
        fixture_literals_are_inert: bool = False,
    ) -> None:
        self.relative_path = relative_path
        self.bootstrap = bootstrap
        self.source = source
        self.findings: dict[tuple[int, str], Finding] = {}
        self.module_aliases: dict[str, str] = {"builtins": "builtins"}
        self.symbol_aliases: dict[str, str] = {}
        self.assignments: dict[str, ast.AST] = {}
        self.tree: ast.AST | None = None
        self.fixture_literals_are_inert = fixture_literals_are_inert

    def add(self, node: ast.AST, rule: str, detail: str) -> None:
        line = int(getattr(node, "lineno", 1) or 1)
        self.findings.setdefault(
            (line, rule), Finding(self.relative_path, line, rule, detail)
        )

    def qualified_name(self, node: ast.AST | None, seen: frozenset[str] = frozenset()) -> str | None:
        if isinstance(node, ast.Name):
            if node.id in seen:
                return node.id
            return self.symbol_aliases.get(node.id) or self.module_aliases.get(node.id) or node.id
        if isinstance(node, ast.Attribute):
            base = self.qualified_name(node.value, seen)
            return f"{base}.{node.attr}" if base else node.attr
        if isinstance(node, ast.Call):
            callee = self.qualified_name(node.func, seen)
            if callee in {"getattr", "builtins.getattr"} and len(node.args) >= 2:
                attribute = self.constant_string(node.args[1], seen)
                base = self.qualified_name(node.args[0], seen)
                if base and attribute:
                    return f"{base}.{attribute}"
        return None

    def constant_string(self, node: ast.AST | None, seen: frozenset[str] = frozenset()) -> str | None:
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            return node.value
        if isinstance(node, ast.Name) and node.id in self.assignments and node.id not in seen:
            return self.constant_string(self.assignments[node.id], seen | {node.id})
        if isinstance(node, ast.BinOp) and isinstance(node.op, (ast.Add, ast.Div)):
            left = self.constant_string(node.left, seen)
            right = self.constant_string(node.right, seen)
            if left is not None and right is not None:
                if isinstance(node.op, ast.Div):
                    return f"{left.rstrip('/')}/{right.lstrip('/')}"
                return left + right
        if isinstance(node, ast.JoinedStr):
            pieces: list[str] = []
            for value in node.values:
                if isinstance(value, ast.Constant) and isinstance(value.value, str):
                    pieces.append(value.value)
                elif isinstance(value, ast.FormattedValue):
                    item = self.constant_string(value.value, seen)
                    if item is None:
                        return None
                    pieces.append(item)
                else:
                    return None
            return "".join(pieces)
        if isinstance(node, ast.Call):
            callee = self.qualified_name(node.func, seen)
            if callee in {
                "str",
                "builtins.str",
                "Path",
                "pathlib.Path",
                "PurePath",
                "pathlib.PurePath",
                "PurePosixPath",
                "pathlib.PurePosixPath",
            } and node.args:
                return self.constant_string(node.args[0], seen)
            if isinstance(node.func, ast.Attribute) and node.func.attr == "join" and node.args:
                separator = self.constant_string(node.func.value, seen)
                sequence = node.args[0]
                if separator is not None and isinstance(sequence, (ast.List, ast.Tuple)):
                    items = [self.constant_string(item, seen) for item in sequence.elts]
                    if all(item is not None for item in items):
                        return separator.join(item for item in items if item is not None)
        return None

    def string_fragments(self, node: ast.AST | None, seen: frozenset[str] = frozenset()) -> list[str]:
        if node is None:
            return []
        constant = self.constant_string(node, seen)
        if constant is not None:
            return [constant]
        if isinstance(node, ast.Name) and node.id in self.assignments and node.id not in seen:
            return self.string_fragments(self.assignments[node.id], seen | {node.id})
        fragments: list[str] = []
        for child in ast.iter_child_nodes(node):
            fragments.extend(self.string_fragments(child, seen))
        return fragments

    def _index(self, tree: ast.AST) -> None:
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    local_name = alias.asname or alias.name.split(".", 1)[0]
                    # ``import a.b`` binds ``a`` while ``import a.b as x`` binds
                    # the full module to ``x``.
                    resolved_name = alias.name if alias.asname else alias.name.split(".", 1)[0]
                    self.module_aliases[local_name] = resolved_name
            elif isinstance(node, ast.ImportFrom):
                module = node.module or ""
                for alias in node.names:
                    if alias.name != "*":
                        self.symbol_aliases[alias.asname or alias.name] = f"{module}.{alias.name}".strip(".")
            elif isinstance(node, (ast.Assign, ast.AnnAssign)):
                value = node.value
                targets = node.targets if isinstance(node, ast.Assign) else [node.target]
                for target in targets:
                    if isinstance(target, ast.Name) and value is not None:
                        self.assignments[target.id] = value
        # Resolve simple callable aliases, including aliases of aliases.
        for _ in range(4):
            changed = False
            for name, value in self.assignments.items():
                resolved = self.qualified_name(value)
                if resolved and self.symbol_aliases.get(name) != resolved:
                    self.symbol_aliases[name] = resolved
                    changed = True
            if not changed:
                break

    def _scan_import(self, node: ast.Import | ast.ImportFrom) -> None:
        names: list[str]
        if isinstance(node, ast.Import):
            names = [alias.name for alias in node.names]
        else:
            names = [node.module or ""]
            names.extend(alias.name for alias in node.names)
        for name in names:
            root = _module_root(name)
            if root in BACKEND_MODULES:
                self.add(node, RULE_DIRECT_BACKEND_IMPORT, f"direct import of alternate backend {root}")
            if root == "semantica" and self.relative_path != self.bootstrap:
                self.add(
                    node,
                    RULE_DIRECT_SEMANTICA_IMPORT,
                    f"Semantica may only be imported by {self.bootstrap}",
                )
            if "SPARQLReasoner" in name:
                self.add(node, RULE_SPARQL_REASONER, "SPARQLReasoner is forbidden")

    @staticmethod
    def _is_dynamic_callable(name: str | None) -> bool:
        if not name:
            return False
        return (
            name in {
                "__import__",
                "builtins.__import__",
                "exec",
                "builtins.exec",
                "eval",
                "builtins.eval",
                "compile",
                "builtins.compile",
                "importlib.import_module",
                "importlib.util.spec_from_file_location",
                "importlib.util.module_from_spec",
            }
            or name.endswith(".exec_module")
            or name.endswith(".SourceFileLoader")
            or name in {
                "pkgutil.resolve_name",
                "pydoc.locate",
                "runpy.run_module",
                "runpy.run_path",
                "importlib.reload",
            }
        )

    @staticmethod
    def _is_process_callable(name: str | None) -> bool:
        if not name:
            return False
        return name in {
            "subprocess.run",
            "subprocess.Popen",
            "subprocess.call",
            "subprocess.check_call",
            "subprocess.check_output",
            "subprocess.getoutput",
            "subprocess.getstatusoutput",
            "asyncio.create_subprocess_exec",
            "asyncio.create_subprocess_shell",
            "anyio.run_process",
            "pexpect.spawn",
            "os.system",
            "os.popen",
            "os.startfile",
        } or name.startswith(("os.exec", "os.spawn"))

    def _scan_process_call(
        self,
        node: ast.Call,
        known_violating_paths: frozenset[str],
        known_source_paths: frozenset[str],
    ) -> None:
        command = node.args[0] if node.args else None
        if command is None:
            for keyword in node.keywords:
                if keyword.arg in {"args", "cmd", "command"}:
                    command = keyword.value
                    break
        fragments = self.string_fragments(command)
        joined = " ".join(fragments).replace("\\", "/").lower()
        reasons: list[str] = []
        if any(token in joined for token in PROCESS_ENGINE_TOKENS):
            reasons.append("command names an alternate ontology engine")
        matched_source_paths: set[str] = set()
        for source_path in known_source_paths:
            lowered = source_path.lower()
            basename = PurePosixPath(lowered).name
            if lowered in joined or (basename and basename in joined):
                matched_source_paths.add(source_path)
        for violating_path in known_violating_paths:
            lowered = violating_path.lower()
            if lowered in joined:
                reasons.append(f"command invokes backend-violating source {violating_path}")
                break
            basename = PurePosixPath(lowered).name
            if basename and basename in joined:
                reasons.append(f"command invokes backend-violating source {basename}")
                break
        tokens = {fragment.lower() for fragment in fragments}
        shell_flag = any(
            keyword.arg == "shell"
            and isinstance(keyword.value, ast.Constant)
            and keyword.value.value is True
            for keyword in node.keywords
        )
        if shell_flag:
            reasons.append("shell=True permits an uninspectable engine command")
        if "-c" in tokens and any(
            executable in joined for executable in ("python", "bash", "sh", "zsh", "java")
        ):
            reasons.append("interpreter -c command can load an alternate engine dynamically")
        if not fragments:
            reasons.append("process command cannot be statically verified as backend-safe")
        first_token = self._first_command_token(command)
        if first_token is None and fragments:
            reasons.append("process executable cannot be statically verified as backend-safe")
        elif first_token is not None:
            executable = PurePosixPath(first_token.replace("\\", "/")).name.lower()
            if executable in {"python", "python3", "py", "java", "bash", "sh", "zsh"}:
                target_files = [
                    fragment.replace("\\", "/")
                    for fragment in fragments
                    if fragment.endswith((".py", ".sh", ".bash", ".zsh", ".jar"))
                ]
                if target_files and not matched_source_paths:
                    reasons.append("interpreter target is not a scanned repository source")
                elif not target_files and "-c" not in tokens and "-m" not in tokens:
                    reasons.append("interpreter subprocess target cannot be statically verified")
        if reasons:
            self.add(node, RULE_ALTERNATE_PROCESS, "; ".join(dict.fromkeys(reasons)))

    def _first_command_token(self, node: ast.AST | None, seen: frozenset[str] = frozenset()) -> str | None:
        if isinstance(node, ast.Name) and node.id in self.assignments and node.id not in seen:
            return self._first_command_token(self.assignments[node.id], seen | {node.id})
        if self.qualified_name(node, seen) == "sys.executable":
            return "python"
        if isinstance(node, (ast.List, ast.Tuple)) and node.elts:
            first = node.elts[0]
            if isinstance(first, ast.Starred):
                return None
            return self._first_command_token(first, seen)
        command = self.constant_string(node, seen)
        if command is not None:
            try:
                tokens = shlex.split(command)
            except ValueError:
                return None
            return tokens[0] if tokens else None
        return None

    def scan(
        self,
        known_violating_paths: frozenset[str] = frozenset(),
        known_source_paths: frozenset[str] = frozenset(),
        *,
        scan_processes: bool = True,
    ) -> list[Finding]:
        try:
            self.tree = ast.parse(self.source, filename=self.relative_path)
        except SyntaxError as exc:
            line = int(exc.lineno or 1)
            return [Finding(self.relative_path, line, RULE_PARSE_FAILURE, f"Python syntax error: {exc.msg}")]
        self._index(self.tree)
        for node in ast.walk(self.tree):
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                self._scan_import(node)
            if isinstance(node, ast.Attribute):
                if node.attr == "_store_backend":
                    self.add(node, RULE_PRIVATE_BACKEND, "private _store_backend access")
                if node.attr == "SPARQLReasoner":
                    self.add(node, RULE_SPARQL_REASONER, "SPARQLReasoner is forbidden")
            elif isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load):
                if node.id == "_store_backend":
                    self.add(node, RULE_PRIVATE_BACKEND, "private _store_backend access")
                if node.id == "SPARQLReasoner":
                    self.add(node, RULE_SPARQL_REASONER, "SPARQLReasoner is forbidden")
            elif isinstance(node, ast.Subscript):
                key = self.constant_string(node.slice)
                if key == "_store_backend":
                    self.add(node, RULE_PRIVATE_BACKEND, "private _store_backend access through mapping")
                if key == "SPARQLReasoner":
                    self.add(node, RULE_SPARQL_REASONER, "SPARQLReasoner lookup is forbidden")
                if key in {"__import__", "import_module", "spec_from_file_location", "exec_module"}:
                    self.add(node, RULE_DYNAMIC_IMPORT, f"dynamic loader lookup through mapping: {key}")
            elif (
                isinstance(node, ast.Constant)
                and isinstance(node.value, str)
                and not self.fixture_literals_are_inert
            ):
                payload_kind = _semantic_payload_kind(node.value)
                if payload_kind is not None and self.relative_path != self.bootstrap:
                    self.add(
                        node,
                        RULE_EMBEDDED_SEMANTIC_PAYLOAD,
                        f"{payload_kind} must be a Semantica package asset",
                    )
                if "_store_backend" in node.value:
                    self.add(node, RULE_PRIVATE_BACKEND, "private _store_backend token in executable source")
                if "SPARQLReasoner" in node.value:
                    self.add(node, RULE_SPARQL_REASONER, "SPARQLReasoner token in executable source")
            if isinstance(node, ast.Call):
                callee = self.qualified_name(node.func)
                if self._is_dynamic_callable(callee):
                    self.add(node, RULE_DYNAMIC_IMPORT, f"dynamic loader/evaluator call: {callee}")
                if callee in {"getattr", "builtins.getattr"} and len(node.args) >= 2:
                    attribute = self.constant_string(node.args[1])
                    if attribute == "_store_backend":
                        self.add(node, RULE_PRIVATE_BACKEND, "private _store_backend access through getattr")
                    if attribute == "SPARQLReasoner":
                        self.add(node, RULE_SPARQL_REASONER, "SPARQLReasoner lookup through getattr")
                if callee in {
                    "operator.attrgetter",
                    "object.__getattribute__",
                    "type.__getattribute__",
                }:
                    attributes = [self.constant_string(argument) for argument in node.args]
                    if "_store_backend" in attributes:
                        self.add(node, RULE_PRIVATE_BACKEND, "private _store_backend reflective lookup")
                    if "SPARQLReasoner" in attributes:
                        self.add(node, RULE_SPARQL_REASONER, "SPARQLReasoner reflective lookup")
                if scan_processes and self._is_process_callable(callee):
                    self._scan_process_call(node, known_violating_paths, known_source_paths)
        return sorted(self.findings.values())


def _strip_java_comments(source: str) -> str:
    """Replace Java comments with spaces while preserving line numbers and strings."""
    output: list[str] = []
    index = 0
    state = "code"
    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if state == "code" and char == "/" and following == "/":
            output.extend("  ")
            index += 2
            state = "line_comment"
        elif state == "code" and char == "/" and following == "*":
            output.extend("  ")
            index += 2
            state = "block_comment"
        elif state == "line_comment":
            output.append("\n" if char == "\n" else " ")
            if char == "\n":
                state = "code"
            index += 1
        elif state == "block_comment":
            if char == "*" and following == "/":
                output.extend("  ")
                index += 2
                state = "code"
            else:
                output.append("\n" if char == "\n" else " ")
                index += 1
        else:
            output.append(char)
            if char == '"':
                state = "string" if state == "code" else "code"
            elif state == "string" and char == "\\" and following:
                output.append(following)
                index += 1
            index += 1
    return "".join(output)


def _java_string_fragments(text: str) -> str:
    strings = re.findall(r'"((?:\\.|[^"\\])*)"', text)
    return "".join(bytes(item, "utf-8").decode("unicode_escape") for item in strings)


def _scan_java(relative: str, source: str) -> list[Finding]:
    clean = _strip_java_comments(source)
    findings: dict[tuple[int, str], Finding] = {}

    def add(offset: int, rule: str, detail: str) -> None:
        line = clean.count("\n", 0, offset) + 1
        findings.setdefault((line, rule), Finding(relative, line, rule, detail))

    for match in re.finditer(r"\borg\.apache\.jena(?:\.[A-Za-z_$][\w$]*)*", clean):
        add(match.start(), RULE_JENA_CLIENT, "direct Apache Jena client reference")
    for match in re.finditer(r"\b(?:Class\.forName|ServiceLoader\.load)\s*\((.*?)\)", clean, re.DOTALL):
        target = _java_string_fragments(match.group(1)).lower()
        add(match.start(), RULE_DYNAMIC_IMPORT, "Java reflection/service loading is forbidden")
        if "org.apache.jena" in target or "jena" in target:
            add(match.start(), RULE_JENA_CLIENT, "Jena client loaded through reflection")
    for match in re.finditer(
        r"\b(?:new\s+ProcessBuilder|Runtime\.getRuntime\s*\(\s*\)\.exec)\s*\((.*?)\)",
        clean,
        re.DOTALL,
    ):
        command = _java_string_fragments(match.group(1)).lower()
        if not command or any(token in command for token in PROCESS_ENGINE_TOKENS):
            add(match.start(), RULE_ALTERNATE_PROCESS, "Java process launch can invoke an alternate engine")
    for token, rule, detail in (
        ("_store_backend", RULE_PRIVATE_BACKEND, "private _store_backend access"),
        ("SPARQLReasoner", RULE_SPARQL_REASONER, "SPARQLReasoner is forbidden"),
    ):
        for match in re.finditer(rf"\b{re.escape(token)}\b", clean):
            add(match.start(), rule, detail)
    return sorted(findings.values())


def _strip_shell_comment(line: str) -> str:
    single = False
    double = False
    escaped = False
    for index, char in enumerate(line):
        if escaped:
            escaped = False
            continue
        if char == "\\" and not single:
            escaped = True
            continue
        if char == "'" and not double:
            single = not single
        elif char == '"' and not single:
            double = not double
        elif char == "#" and not single and not double and index != 0:
            return line[:index]
    return line


def _scan_shell(relative: str, source: str) -> list[Finding]:
    findings: dict[tuple[int, str], Finding] = {}
    for line_number, raw_line in enumerate(source.splitlines(), 1):
        stripped_raw = raw_line.lstrip()
        if (stripped_raw.startswith("#") and not stripped_raw.startswith("#!")) or re.match(
            r"(?i)^rem(?:\s|$)", stripped_raw
        ):
            continue
        line = _strip_shell_comment(raw_line).strip()
        if not line or line.startswith("#!"):
            continue
        lowered = line.lower()
        try:
            tokens = shlex.split(line, comments=True, posix=True)
        except ValueError:
            tokens = line.split()
        token_text = " ".join(tokens).lower()
        dynamic_switches = {"-c", "/c", "-command", "-encodedcommand"}
        if re.search(r"(^|[;&|]\s*|\b)(eval|invoke-expression)\b", lowered) or (
            dynamic_switches.intersection(token.lower() for token in tokens)
            and any(
                name in token_text
                for name in ("python", "bash", "zsh", "sh", "java", "powershell", "pwsh", "cmd")
            )
        ):
            findings[(line_number, RULE_DYNAMIC_IMPORT)] = Finding(
                relative, line_number, RULE_DYNAMIC_IMPORT, "dynamic shell/interpreter execution"
            )
        if any(re.search(rf"(?<![\w.-]){re.escape(token)}(?![\w.-])", token_text) for token in PROCESS_ENGINE_TOKENS):
            findings[(line_number, RULE_ALTERNATE_PROCESS)] = Finding(
                relative,
                line_number,
                RULE_ALTERNATE_PROCESS,
                "shell command invokes an alternate ontology engine",
            )
        if "_store_backend" in line:
            findings[(line_number, RULE_PRIVATE_BACKEND)] = Finding(
                relative, line_number, RULE_PRIVATE_BACKEND, "private _store_backend access"
            )
        if "SPARQLReasoner" in line:
            findings[(line_number, RULE_SPARQL_REASONER)] = Finding(
                relative, line_number, RULE_SPARQL_REASONER, "SPARQLReasoner is forbidden"
            )
    return sorted(findings.values())


def _scan_manifest(relative: str, source: str) -> list[Finding]:
    findings: dict[tuple[int, str], Finding] = {}
    for line_number, raw_line in enumerate(source.splitlines(), 1):
        line = raw_line.strip()
        if not line or line.startswith(("#", ";")):
            continue
        lowered = line.lower().replace("_", "-")
        for package in BACKEND_MODULES:
            package_name = package.replace("_", "-")
            if re.search(rf"(?<![a-z0-9.-]){re.escape(package_name)}(?![a-z0-9.-])", lowered):
                findings[(line_number, RULE_DIRECT_BACKEND_DEPENDENCY)] = Finding(
                    relative,
                    line_number,
                    RULE_DIRECT_BACKEND_DEPENDENCY,
                    f"direct dependency on alternate backend {package}",
                )
                break
        if re.search(r"\borg\.apache\.jena\b|apache[-.]jena", lowered):
            findings[(line_number, RULE_JENA_CLIENT)] = Finding(
                relative, line_number, RULE_JENA_CLIENT, "direct Apache Jena dependency"
            )
    return sorted(findings.values())


def _read_text(path: Path, relative: str) -> tuple[str | None, Finding | None]:
    try:
        return path.read_text(encoding="utf-8"), None
    except (OSError, UnicodeError) as exc:
        return None, Finding(relative, 1, RULE_PARSE_FAILURE, f"cannot read active source: {exc}")


def scan_repository(
    root: Path,
    bootstrap: str = REQUIRED_BOOTSTRAP,
    *,
    literal_fixture_hosts: frozenset[str] = frozenset(),
) -> tuple[list[Finding], dict[str, int]]:
    root = root.resolve()
    surfaces, discovery_findings = _discover_surfaces(root)
    counts = {"python": 0, "java": 0, "shell": 0, "manifest": 0, "semantic_asset": 0}
    sources: dict[str, tuple[str, str]] = {}
    findings: list[Finding] = list(discovery_findings)
    for path, kind in surfaces:
        relative = path.relative_to(root).as_posix()
        counts[kind] += 1
        source, error = _read_text(path, relative)
        if error:
            findings.append(error)
        elif source is not None:
            sources[relative] = (kind, source)

    # Pass one finds direct backend users.  Pass two can then reject a subprocess
    # that calls one of those files even when the command itself contains no
    # backend package name.
    first_pass_python: dict[str, list[Finding]] = {}
    first_pass_other: dict[str, list[Finding]] = {}
    for relative, (kind, source) in sources.items():
        if kind == "python":
            first_pass_python[relative] = PythonScanner(
                relative,
                bootstrap,
                source,
                fixture_literals_are_inert=relative in literal_fixture_hosts,
            ).scan(scan_processes=False)
        elif kind == "java":
            first_pass_other[relative] = _scan_java(relative, source)
        elif kind == "shell":
            first_pass_other[relative] = _scan_shell(relative, source)
    policy_violating_paths = frozenset(
        relative
        for relative, items in {**first_pass_python, **first_pass_other}.items()
        if items
    )
    known_source_paths = frozenset(
        relative for relative, (kind, _) in sources.items() if kind in {"python", "java", "shell"}
    )

    for relative, (kind, source) in sources.items():
        if kind == "python":
            findings.extend(
                PythonScanner(
                    relative,
                    bootstrap,
                    source,
                    fixture_literals_are_inert=relative in literal_fixture_hosts,
                ).scan(policy_violating_paths, known_source_paths)
            )
        elif kind == "java":
            findings.extend(_scan_java(relative, source))
        elif kind == "shell":
            findings.extend(_scan_shell(relative, source))
        elif kind == "manifest":
            findings.extend(_scan_manifest(relative, source))
        elif kind == "semantic_asset":
            findings.append(
                Finding(
                    relative,
                    1,
                    RULE_DUPLICATE_SEMANTIC_ASSET,
                    "executable ontology/query/shape asset must move to the Semantica package",
                )
            )
    return sorted(set(findings)), counts


def evaluate_repository(root: Path, policy_path: Path, mode: str) -> GateReport:
    if mode not in {"audit", "strict"}:
        raise ValueError("mode must be 'audit' or 'strict'")
    root = root.resolve()
    policy_path = policy_path if policy_path.is_absolute() else root / policy_path
    policy, policy_errors = _read_policy(policy_path, root)
    bootstrap = str(policy.get("bootstrap", REQUIRED_BOOTSTRAP))
    configured_fixture_hosts = frozenset(
        str(item.get("path"))
        for item in policy.get("literal_fixture_hosts", [])
        if isinstance(item, dict) and item.get("path") in PERMITTED_LITERAL_FIXTURE_HOSTS
    )
    findings, counts = scan_repository(
        root,
        bootstrap=bootstrap,
        literal_fixture_hosts=configured_fixture_hosts,
    )
    allowlist = _allow_entries(policy)
    allowance_map = {(entry.path, rule): entry for entry in allowlist for rule in entry.rules}
    allowed: list[Finding] = []
    unapproved: list[Finding] = []
    for finding in findings:
        if mode == "audit" and (finding.path, finding.rule) in allowance_map:
            allowed.append(finding)
        else:
            unapproved.append(finding)

    finding_keys = {(finding.path, finding.rule) for finding in findings}
    stale = sorted(
        f"{entry.path}: {rule}"
        for entry in allowlist
        for rule in entry.rules
        if (entry.path, rule) not in finding_keys
    )
    if mode == "strict" and allowlist:
        policy_errors.append(
            f"strict mode requires zero allowlist entries; found {len(allowlist)}"
        )
    fixture_hosts = [
        str(item.get("path"))
        for item in policy.get("literal_fixture_hosts", [])
        if isinstance(item, dict) and item.get("path")
    ]
    return GateReport(
        mode=mode,
        bootstrap=bootstrap,
        scanned_by_kind=counts,
        findings=findings,
        allowed_findings=allowed,
        unapproved_findings=unapproved,
        allowlist=allowlist,
        stale_allowances=stale,
        policy_errors=policy_errors,
        fixture_hosts=fixture_hosts,
    )


def _report_as_json(report: GateReport) -> str:
    payload = {
        "passed": report.passed,
        "mode": report.mode,
        "bootstrap": report.bootstrap,
        "scanned_by_kind": report.scanned_by_kind,
        "scanned_total": sum(report.scanned_by_kind.values()),
        "remaining_findings": len(report.findings),
        "remaining_files": report.debt_file_count,
        "allowlist_entries": len(report.allowlist),
        "allowed_findings": [finding.__dict__ for finding in report.allowed_findings],
        "unapproved_findings": [finding.__dict__ for finding in report.unapproved_findings],
        "stale_allowances": report.stale_allowances,
        "policy_errors": report.policy_errors,
        "literal_fixture_hosts": report.fixture_hosts,
    }
    return json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True)


def _report_as_text(report: GateReport) -> str:
    status = "PASS" if report.passed else "FAIL"
    lines = [
        f"Semantica unique-backend gate: {status} ({report.mode})",
        f"bootstrap: {report.bootstrap}",
        "scanned active surfaces: "
        + str(sum(report.scanned_by_kind.values()))
        + " ("
        + ", ".join(f"{kind}={count}" for kind, count in sorted(report.scanned_by_kind.items()))
        + ")",
        (
            "remaining migration debt: "
            f"{len(report.findings)} finding(s) in {report.debt_file_count} file(s); "
            f"exact allowlist entries={len(report.allowlist)}"
        ),
        (
            "gate fixture hosts: "
            f"{len(report.fixture_hosts)} (files remain AST-scanned; only inert string literals are fixtures)"
        ),
    ]
    for error in report.policy_errors:
        lines.append(f"POLICY ERROR: {error}")
    for stale in report.stale_allowances:
        lines.append(f"STALE ALLOWANCE: {stale}")
    for finding in report.unapproved_findings:
        lines.append(
            f"BLOCK {finding.path}:{finding.line} [{finding.rule}] {finding.detail}"
        )
    if report.mode == "audit":
        for finding in report.allowed_findings:
            lines.append(
                f"DEBT  {finding.path}:{finding.line} [{finding.rule}] {finding.detail}"
            )
    return "\n".join(lines)


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--policy", type=Path, default=Path(DEFAULT_POLICY))
    parser.add_argument("--mode", choices=("audit", "strict"), default="audit")
    parser.add_argument("--json", action="store_true", dest="as_json")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = _build_parser().parse_args(argv)
    report = evaluate_repository(args.root, args.policy, args.mode)
    print(_report_as_json(report) if args.as_json else _report_as_text(report))
    return 0 if report.passed else 1


if __name__ == "__main__":
    sys.exit(main())
