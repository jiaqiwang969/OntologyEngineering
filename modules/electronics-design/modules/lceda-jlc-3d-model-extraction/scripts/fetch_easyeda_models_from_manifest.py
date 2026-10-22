#!/usr/bin/env python3
"""Fetch EasyEDA/LCSC OBJ/STEP model evidence from a role manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


SEARCH_URL = "https://easyeda.com/api/components/search"
COMPONENT_URL = "https://easyeda.com/api/components/{uuid}?version=6.4.7&uuid={uuid}&datastrid="
OBJ_URL = "https://modules.easyeda.com/3dmodel/{uuid}"
STEP_URL = "https://modules.easyeda.com/qAxj6KHrDKw4blvCG8QJPs7Y/{uuid}"


HEADERS = {
    "authority": "easyeda.com",
    "pragma": "no-cache",
    "cache-control": "no-cache",
    "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36",
    "Accept": "application/json, text/javascript, */*; q=0.01",
    "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
    "X-Requested-With": "XMLHttpRequest",
    "Origin": "https://easyeda.com",
    "Referer": "https://easyeda.com/editor",
    "sec-fetch-site": "same-origin",
    "sec-fetch-mode": "cors",
    "sec-fetch-dest": "empty",
    "accept-language": "cs,en;q=0.9,sk;q=0.8,en-GB;q=0.7",
}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def sanitize(value: Any) -> str:
    text = str(value or "unknown")
    text = re.sub(r"[^A-Za-z0-9._-]+", "_", text).strip("_")
    return text or "unknown"


def normalize(value: Any) -> str:
    return re.sub(r"[^A-Z0-9]+", "", str(value or "").upper())


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def dump_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def parse_data_str(value: Any) -> Any:
    if isinstance(value, str):
        return json.loads(value)
    return value


def http_json(url: str, *, data: bytes | None = None, headers: dict[str, str] | None = None) -> Any:
    req = urllib.request.Request(url, data=data, headers=headers or HEADERS, method="POST" if data else "GET")
    with urllib.request.urlopen(req, timeout=45) as resp:
        return json.loads(resp.read().decode("utf-8", errors="replace"))


def http_bytes(url: str) -> tuple[int, str | None, bytes]:
    req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, resp.headers.get("content-type"), resp.read()
    except urllib.error.HTTPError as exc:
        return exc.code, exc.headers.get("content-type"), exc.read()


def c_para(component: dict[str, Any]) -> dict[str, Any]:
    data_str = parse_data_str(component.get("dataStr"))
    return (((data_str or {}).get("head") or {}).get("c_para") or {})


def get_model_uuid(component: dict[str, Any]) -> str | None:
    package_detail = component.get("packageDetail") or {}
    data_str = parse_data_str(package_detail.get("dataStr"))
    for shape in (data_str or {}).get("shape") or []:
        svg_data = None
        if isinstance(shape, str) and shape.startswith("SVGNODE~"):
            try:
                svg_data = json.loads(shape.removeprefix("SVGNODE~"))
            except json.JSONDecodeError:
                pass
        elif isinstance(shape, dict):
            svg_data = shape.get("svgData") or shape
        uuid = (((svg_data or {}).get("attrs") or {}).get("uuid"))
        if isinstance(uuid, str) and uuid:
            return uuid
    raw = json.dumps(component)
    match = re.search(r'"uuid_3d"\s*:\s*"([a-f0-9]{32})"', raw, re.I)
    return match.group(1) if match else None


def search_easyeda(query: str) -> dict[str, Any]:
    body = urllib.parse.urlencode(
        {
            "type": "3",
            "doctype[]": "2",
            "uid": "0819f05c4eef4c71ace90d822a990e87",
            "returnListStyle": "classifyarr",
            "wd": query,
            "version": "6.4.7",
        }
    ).encode()
    return http_json(SEARCH_URL, data=body)


def fetch_component(uuid: str) -> dict[str, Any]:
    return http_json(COMPONENT_URL.format(uuid=uuid))["result"]


def choose_search_hit(search_result: dict[str, Any], part: dict[str, Any]) -> tuple[dict[str, Any] | None, str]:
    hits = (((search_result.get("result") or {}).get("lists") or {}).get("lcsc") or [])
    if not hits:
        return None, "no_lcsc_hits"
    target_mpn = normalize(part.get("mpn") or part.get("bom_mpn") or part.get("query"))
    target_manufacturer = normalize(part.get("manufacturer"))
    best = None
    best_reason = "first_lcsc_hit"
    for hit in hits:
        para = (((hit.get("dataStr") or {}).get("head") or {}).get("c_para") or {})
        fields = [
            para.get("Manufacturer Part"),
            para.get("name"),
            hit.get("title"),
            para.get("Supplier Part"),
        ]
        mpn_match = target_mpn and any(normalize(field) == target_mpn for field in fields)
        manufacturer = normalize(para.get("Manufacturer"))
        manufacturer_match = not target_manufacturer or target_manufacturer in manufacturer or manufacturer in target_manufacturer
        if mpn_match and manufacturer_match:
            return hit, "exact_mpn_manufacturer_match"
        if mpn_match and best is None:
            best = hit
            best_reason = "exact_mpn_match"
    return best or hits[0], best_reason


def obj_summary(data: bytes) -> dict[str, Any]:
    vertices = []
    faces = 0
    for raw in data.decode("utf-8", errors="replace").splitlines():
        if raw.startswith("v "):
            parts = raw.split()
            if len(parts) >= 4:
                try:
                    vertices.append((float(parts[1]), float(parts[2]), float(parts[3])))
                except ValueError:
                    pass
        elif raw.startswith("f "):
            faces += 1
    result: dict[str, Any] = {"vertices": len(vertices), "faces": faces}
    if vertices:
        xs, ys, zs = zip(*vertices)
        result["bbox_min"] = [min(xs), min(ys), min(zs)]
        result["bbox_max"] = [max(xs), max(ys), max(zs)]
        result["bbox_size"] = [max(xs) - min(xs), max(ys) - min(ys), max(zs) - min(zs)]
    return result


def accepted_obj(data: bytes) -> bool:
    summary = obj_summary(data)
    return summary["vertices"] > 0 and summary["faces"] > 0


def accepted_step(data: bytes) -> bool:
    head = data[:4096].decode("utf-8", errors="replace")
    return "ISO-10303" in head or "HEADER;" in head or "FILE_DESCRIPTION" in head


def write_sha256sums(folder: Path, filenames: list[str]) -> None:
    rows = []
    for name in filenames:
        target = folder / name
        if target.is_file():
            rows.append(f"{sha256(target)}  {name}")
    (folder / "SHA256SUMS").write_text("\n".join(rows) + "\n", encoding="utf-8")


def copy_source_files(out_root: Path, source_files: list[Path]) -> None:
    if not source_files:
        return
    folder = out_root / "source-files"
    folder.mkdir(parents=True, exist_ok=True)
    copied = []
    for src in source_files:
        if not src.is_file():
            print(f"[WARN] missing source file: {src}", file=sys.stderr)
            continue
        name = src.name
        shutil.copy2(src, folder / name)
        copied.append(name)
    write_sha256sums(folder, copied)


def run(manifest_path: Path, out_root: Path, source_files: list[Path], strict: bool) -> int:
    manifest_payload = load_json(manifest_path)
    parts = manifest_payload.get("parts") if isinstance(manifest_payload, dict) else manifest_payload
    if not isinstance(parts, list):
        raise SystemExit("manifest must be a list or an object with a parts list")

    out_root.mkdir(parents=True, exist_ok=True)
    copy_source_files(out_root, source_files)
    manifest_out: dict[str, Any] = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "input_manifest": str(manifest_path),
        "parts": [],
    }

    for part in parts:
        query = str(part.get("query") or part.get("mpn") or part.get("bom_mpn") or "").strip()
        if not query:
            print(f"[WARN] skipping row without query/mpn: {part}", file=sys.stderr)
            continue
        search_result = search_easyeda(query)
        hit, match_reason = choose_search_hit(search_result, part)
        if hit is None:
            print(f"[WARN] no EasyEDA hit for {query}", file=sys.stderr)
            continue
        component = fetch_component(hit["uuid"])
        para = c_para(component)
        model_uuid = get_model_uuid(component)
        lcsc = ((component.get("lcsc") or {}).get("number") or (component.get("szlcsc") or {}).get("number") or para.get("Supplier Part"))
        mpn = part.get("mpn") or part.get("bom_mpn") or query
        accepted_match = match_reason.startswith("exact_mpn")
        if strict and not accepted_match:
            print(f"[WARN] rejecting near-miss for {query}: {component.get('title')} ({match_reason})", file=sys.stderr)
            continue

        role = part.get("role") or part.get("designators") or "ROLE"
        folder = out_root / f"{sanitize(role)}-{sanitize(mpn)}-{sanitize(lcsc)}"
        folder.mkdir(parents=True, exist_ok=True)
        raw_name = f"easyeda-device-{sanitize(component.get('title') or mpn)}-{sanitize(lcsc)}.raweasy.json"
        dump_json(folder / raw_name, component)

        evidence = {
            "role": role,
            "designators": part.get("designators"),
            "bom_mpn": mpn,
            "bom_manufacturer": part.get("manufacturer"),
            "expected_package_from_bom": part.get("expected_package"),
            "match_reason": match_reason,
            "easyeda_uuid": component.get("uuid"),
            "lcsc_number": lcsc,
            "easyeda_title": component.get("title"),
            "easyeda_manufacturer": para.get("Manufacturer"),
            "easyeda_manufacturer_part": para.get("Manufacturer Part"),
            "easyeda_supplier_part": para.get("Supplier Part"),
            "easyeda_package": para.get("package"),
            "package_detail_uuid": (component.get("packageDetail") or {}).get("uuid"),
            "package_detail_title": (component.get("packageDetail") or {}).get("title"),
            "model_uuid": model_uuid,
            "source_urls": {
                "easyeda_obj": OBJ_URL.format(uuid=model_uuid) if model_uuid else None,
                "easyeda_step": STEP_URL.format(uuid=model_uuid) if model_uuid else None,
            },
            "altium_import_status": "not_imported",
            "footprint_alignment_checked": False,
            "formal_fabrication_release_allowed": False,
        }
        dump_json(folder / "source-evidence.json", evidence)

        filenames = [raw_name, "source-evidence.json"]
        downloads = []
        if model_uuid:
            for kind, suffix, url, accept_fn in [
                ("obj", "obj", OBJ_URL.format(uuid=model_uuid), accepted_obj),
                ("step", "step", STEP_URL.format(uuid=model_uuid), accepted_step),
            ]:
                status, content_type, data = http_bytes(url)
                accepted = 200 <= status < 300 and bool(data) and accept_fn(data)
                name = f"{sanitize(component.get('title') or mpn)}-{sanitize(lcsc)}.easyeda-source.{suffix}"
                report: dict[str, Any] = {
                    "kind": kind,
                    "url": url,
                    "status": status,
                    "content_type": content_type,
                    "bytes": len(data),
                    "accepted": accepted,
                }
                if kind == "obj" and accepted:
                    report["obj_summary"] = obj_summary(data)
                if accepted:
                    (folder / name).write_bytes(data)
                    filenames.append(name)
                downloads.append(report)
        dump_json(folder / "download-report.json", {"downloads": downloads})
        filenames.append("download-report.json")

        readme = (
            f"# {role} {mpn}\n\n"
            f"Status: {match_reason}\n\n"
            "This folder preserves EasyEDA/LCSC source evidence and downloaded OBJ/STEP candidates. "
            "OBJ is source model evidence; direct STEP is an import candidate only.\n\n"
            "altium_import_status=not_imported\n"
            "footprint_alignment_checked=false\n"
            "formal_fabrication_release_allowed=false\n"
        )
        (folder / "README.md").write_text(readme, encoding="utf-8")
        filenames.append("README.md")
        write_sha256sums(folder, filenames)
        manifest_out["parts"].append({**evidence, "folder": str(folder.relative_to(out_root)), "downloads": downloads})
        print(f"[OK] {role} {mpn} -> {folder}")

    dump_json(out_root / "manifest.json", manifest_out)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path, help="JSON list or object with parts[] role rows")
    parser.add_argument("output_root", type=Path)
    parser.add_argument("--source-file", action="append", default=[], type=Path, help="BOM/Gerber/pick-place evidence to copy and hash")
    parser.add_argument("--allow-near-miss", action="store_true", help="Save first EasyEDA hit even when exact MPN matching fails")
    args = parser.parse_args()
    return run(args.manifest, args.output_root, args.source_file, strict=not args.allow_near_miss)


if __name__ == "__main__":
    raise SystemExit(main())
