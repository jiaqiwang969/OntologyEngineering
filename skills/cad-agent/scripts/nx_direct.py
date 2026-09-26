#!/usr/bin/env python3
"""Run a reviewed NXOpen Python journal in an isolated Windows batch process.

No MCP, server, desktop automation, or credentials. SSH is supplied by a local
profile (fleet in the studio). A job ID is single-use; uncertainty is inspected,
never retried. This is orchestration, not a sandbox for untrusted Python.
"""
from __future__ import annotations
import argparse
import base64
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import shlex
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
JOB_ID = re.compile(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}\Z")
MAX_BYTES = 100 * 1024 * 1024

def digest(data):
    return hashlib.sha256(data).hexdigest()

def load_profile(path):
    p = json.loads(Path(path).read_text())
    cmd = p.get("ssh_command")
    if not isinstance(cmd, list) or not cmd or not all(isinstance(v, str) and v for v in cmd):
        raise ValueError("ssh_command must be an argv array, not shell text")
    for key in ("remote_root", "run_journal"):
        if not isinstance(p.get(key), str) or not p[key] or any(c in p[key] for c in '\r\n"'):
            raise ValueError(f"invalid {key}")
    return p

def ps_quote(value):
    return "'" + str(value).replace("'", "''") + "'"

def remote(profile, script, stdin=None, timeout=60):
    preamble = "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); "
    encoded = base64.b64encode((preamble + script).encode("utf-16le")).decode()
    transport = profile["ssh_command"]
    if Path(transport[0]).name == "fleet" and len(transport) >= 3 and transport[1] == "ssh":
        # Resolve identity first with stdin closed: fleet discovery subprocesses
        # must never consume bytes intended for the Windows upload stream.
        route = subprocess.run(transport + ["--print-only"], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60, check=False)
        if route.returncode: raise RuntimeError(route.stderr.decode("utf-8", "replace")[-2000:])
        transport = shlex.split(route.stdout.decode("utf-8"))
        if not transport or Path(transport[0]).name != "ssh": raise ValueError("fleet did not return an SSH command")
    return subprocess.run(transport + ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", encoded], input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout, check=False)

def checked_json(result):
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", "replace")[-3000:] or result.stdout.decode("utf-8", "replace")[-3000:])
    return json.loads(result.stdout.decode("utf-8-sig").strip())

def safe_members(zf):
    total = 0
    names = set()
    for item in zf.infolist():
        name = item.filename
        p = PurePosixPath(name)
        if not name or p.is_absolute() or ".." in p.parts or "\\" in name or ":" in name or name.casefold() in names or ((item.external_attr >> 16) & 0o170000) == 0o120000:
            raise ValueError(f"unsafe archive member: {name!r}")
        total += item.file_size
        if total > MAX_BYTES or len(names) >= 10000:
            raise ValueError("archive exceeds intake limits")
        names.add(name.casefold())
    bad = zf.testzip()
    if bad:
        raise ValueError(f"archive CRC failure: {bad}")

def prepare(journal, params, inputs, dest, job_id):
    if not JOB_ID.fullmatch(job_id):
        raise ValueError("job ID must contain only ASCII letters, numbers, _ and -")
    journal = Path(journal).resolve()
    payloads = {"journal.py": journal.read_bytes(), "params.json": (json.dumps(params, ensure_ascii=False, indent=2) + "\n").encode()}
    for name in ("worker.py", "run-job.ps1"):
        payloads[name] = (ROOT / "assets/nx-direct" / name).read_bytes()
    for src in inputs:
        src = Path(src).resolve()
        name = "inputs/" + src.name
        if name.casefold() in {x.casefold() for x in payloads} or not src.is_file() or any(c in src.name for c in ':\\'):
            raise ValueError(f"duplicate/invalid input: {src.name}")
        payloads[name] = src.read_bytes()
    if sum(map(len, payloads.values())) > MAX_BYTES:
        raise ValueError("job exceeds 100 MiB transport limit; use a reviewed file-transfer route")
    spec = {"schema": "cad-agent.nx-direct-job/v1", "job_id": job_id, "journal_source": journal.name, "files": {name: {"sha256": digest(data), "bytes": len(data)} for name, data in payloads.items()}}
    payloads["job.json"] = (json.dumps(spec, indent=2) + "\n").encode()
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=False)
    for name, data in payloads.items():
        out = dest / name
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(data)
    return spec

def bundle(job):
    job = Path(job)
    spec = json.loads((job / "job.json").read_text())
    if not JOB_ID.fullmatch(spec["job_id"]):
        raise ValueError("invalid job ID")
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(job / "job.json", "job.json")
        for name, expected in spec["files"].items():
            source = (job / name).resolve()
            if not source.is_relative_to(job.resolve()) or not source.is_file():
                raise ValueError("job input leaves directory")
            data = source.read_bytes()
            if digest(data) != expected["sha256"] or len(data) != expected["bytes"]:
                raise ValueError(f"job input changed: {name}")
            z.writestr(name, data)
    data = stream.getvalue()
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        safe_members(z)
    return spec, data

def binding(profile, job_id):
    if not JOB_ID.fullmatch(job_id):
        raise ValueError("invalid job ID")
    return "$root=[Environment]::ExpandEnvironmentVariables(" + ps_quote(profile["remote_root"]) + "); $job=Join-Path $root " + ps_quote(job_id) + "; "

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("prepare")
    p.add_argument("--journal", required=True); p.add_argument("--params")
    p.add_argument("--input", action="append", default=[])
    p.add_argument("--job-id", required=True); p.add_argument("--out", required=True)
    for verb in ("probe", "stage", "run", "status", "fetch"):
        p = sub.add_parser(verb); p.add_argument("--profile", required=True)
        if verb == "stage": p.add_argument("--job", required=True)
        elif verb != "probe": p.add_argument("--job-id", required=True)
        if verb == "run": p.add_argument("--wait-seconds", type=int, default=120)
        if verb == "fetch": p.add_argument("--out", required=True)
    a = parser.parse_args()
    try:
        if a.action == "prepare":
            result = prepare(a.journal, json.loads(Path(a.params).read_text()) if a.params else {}, a.input, a.out, a.job_id)
        else:
            p = load_profile(a.profile)
            if a.action == "probe":
                ps = "$exe=[Environment]::ExpandEnvironmentVariables(" + ps_quote(p["run_journal"]) + "); $ok=Test-Path -LiteralPath $exe -PathType Leaf; @{hostname=$env:COMPUTERNAME; run_journal=$exe; executable_present=$ok; nx_version=$(if($ok){(Get-Item -LiteralPath $exe).VersionInfo.ProductVersion}); processes=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -in @('run_journal.exe','ugraf.exe')} | Select-Object ProcessId,SessionId,CreationDate,CommandLine); license_status='not_tested'; native_execution='not_run'} | ConvertTo-Json -Depth 6"
                result = checked_json(remote(p, ps))
            elif a.action == "stage":
                spec, data = bundle(a.job)
                encoded_data = base64.b64encode(data)
                ps = binding(p, spec["job_id"]) + "$inputChars=" + str(len(encoded_data)) + "; " + r"""
if(Test-Path -LiteralPath $job){throw 'Single-use job exists; inspect status, do not restage'}
New-Item -ItemType Directory -Path $job -ErrorAction Stop | Out-Null
$zip=Join-Path $job 'bundle.zip'
$buffer=[char[]]::new($inputChars)
$received=[Console]::In.ReadBlock($buffer,0,$inputChars)
if($received -ne $inputChars){throw 'Incomplete upload stream'}
[IO.File]::WriteAllBytes($zip,[Convert]::FromBase64CharArray($buffer,0,$received))
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::ExtractToDirectory($zip,$job)
$stage=@{state='STAGED'; archive_sha256=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower(); job=$job}
[IO.File]::WriteAllText((Join-Path $job 'stage.json'),($stage | ConvertTo-Json),[Text.UTF8Encoding]::new($false))
@{state='STAGED'; job=$job} | ConvertTo-Json
"""
                result = checked_json(remote(p, ps, encoded_data))
            elif a.action == "run":
                if not 1 <= a.wait_seconds <= 300: raise ValueError("wait-seconds must be 1..300")
                ps = binding(p, a.job_id) + "$exe=[Environment]::ExpandEnvironmentVariables(" + ps_quote(p["run_journal"]) + "); & (Join-Path $job 'run-job.ps1') -JobDir $job -RootDir $root -RunJournal $exe"
                try:
                    result = checked_json(remote(p, ps, timeout=a.wait_seconds))
                except (subprocess.TimeoutExpired, RuntimeError) as exc:
                    detail = f"Local wait limit reached ({a.wait_seconds} s)" if isinstance(exc, subprocess.TimeoutExpired) else str(exc)
                    print(json.dumps({"state": "UNKNOWN_AFTER_DISPATCH", "job_id": a.job_id, "detail": detail, "next": "status on this job; never rerun"}))
                    return 3
            elif a.action == "status":
                ps = binding(p, a.job_id) + r"""
$s=Join-Path $job 'state.json'; $r=Join-Path $job 'native-result.json'
@{exists=(Test-Path -LiteralPath $job); state=$(if(Test-Path -LiteralPath $s){Get-Content -Raw -LiteralPath $s | ConvertFrom-Json}else{$null}); native_result=$(if(Test-Path -LiteralPath $r){Get-Content -Raw -LiteralPath $r | ConvertFrom-Json}else{$null}); processes=@(Get-CimInstance Win32_Process | Where-Object {$_.CommandLine -and $_.CommandLine.Contains((Join-Path $job 'worker.py'))} | Select-Object ProcessId,SessionId,CreationDate,CommandLine)} | ConvertTo-Json -Depth 30
"""
                result = checked_json(remote(p, ps))
            else:
                out = Path(a.out)
                if out.exists(): raise ValueError("fetch output must be a new directory")
                ps = binding(p, a.job_id) + r"""
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.IO.Compression
$stream=[IO.MemoryStream]::new(); $zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create,$true)
$total=0
try { Get-ChildItem -LiteralPath $job -File -Recurse | Where-Object {$_.Name -ne 'bundle.zip'} | ForEach-Object {
  $total += $_.Length; if($total -gt 104857600){throw 'fetch exceeds 100 MiB'}
  $name=$_.FullName.Substring($job.Length+1).Replace('\','/')
  [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,$_.FullName,$name) | Out-Null
} } finally {$zip.Dispose()}
[Console]::Write([Convert]::ToBase64String($stream.ToArray())); $stream.Dispose()
"""
                raw = remote(p, ps)
                if raw.returncode: raise RuntimeError(raw.stderr.decode("utf-8", "replace")[-2000:])
                blob = base64.b64decode(raw.stdout.strip(), validate=True)
                if len(blob) > MAX_BYTES: raise ValueError("fetch exceeds byte limit")
                with zipfile.ZipFile(io.BytesIO(blob)) as z:
                    safe_members(z)
                    out.mkdir(parents=True, exist_ok=False)
                    z.extractall(out)
                result = {"fetched": str(out), "archive_sha256": digest(blob), "note": "Inspect native-result and original CAD; file presence alone is not acceptance"}
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 1 if a.action == "run" and result.get("state") != "COMPLETED" else 0
    except (OSError, ValueError, KeyError, RuntimeError, zipfile.BadZipFile, subprocess.TimeoutExpired) as exc:
        detail = "Transport wait limit reached; inspect the same job before any further operation" if isinstance(exc, subprocess.TimeoutExpired) else str(exc)
        print(json.dumps({"error": detail}, ensure_ascii=False), file=sys.stderr)
        return 2

if __name__ == "__main__":
    raise SystemExit(main())
