"""Credential containment and bounded HTTP failure behavior, with synthetic keys."""
import io
import hashlib
import json
import os
import stat
import urllib.error

import pytest

from ontology_engineering.jev_transport import JevTransport, TransportError, _NoRedirect, resolve_credential_file


def credential(tmp_path, content="apikey_SYNTHETIC_TEST_ONLY"):
    p = tmp_path / "credential.md"; p.write_text(content); p.chmod(0o600)
    return p


@pytest.mark.parametrize("change", ["permissions", "symlink", "ambiguous", "missing", "large"])
def test_uncontrolled_credential_container_is_rejected(tmp_path, change):
    p = credential(tmp_path)
    if change == "permissions": p.chmod(0o644)
    if change == "symlink":
        link = tmp_path / "link"; link.symlink_to(p); p = link
    if change == "ambiguous": p.write_text("apikey_SYNTHETIC_A apikey_SYNTHETIC_B")
    if change == "missing": p.write_text("no key")
    if change == "large": p.write_text("a"*65537)
    with pytest.raises(ValueError): JevTransport(p)


def test_redirects_never_forward_authorization():
    assert _NoRedirect().redirect_request(None, None, 302, "", {}, "https://example.org") is None


def test_payload_containing_credential_material_does_not_reach_network(tmp_path, monkeypatch):
    t = JevTransport(credential(tmp_path))
    monkeypatch.setattr("urllib.request.build_opener", lambda *args: pytest.fail("network must not be reached"))
    with pytest.raises(TransportError, match="credential_material_in_request"):
        t({"model":"jev-1.13.0","state":"apikey_SYNTHETIC_TEST_ONLY","questions":{}})
    assert "SYNTHETIC_TEST_ONLY" not in repr(t)


@pytest.mark.parametrize("code,retryable", [(401,False),(429,True),(500,True),(529,True),(400,False)])
def test_http_error_only_retains_sanitized_status(tmp_path, monkeypatch, code, retryable):
    t = JevTransport(credential(tmp_path))
    class Opener:
        def open(self, request, **kwargs):
            raise urllib.error.HTTPError(request.full_url, code, "private response body", {"Retry-After":"2"}, io.BytesIO(b"private body"))
    monkeypatch.setattr("urllib.request.build_opener", lambda *args: Opener())
    with pytest.raises(TransportError) as error:
        t({"model":"jev-1.13.0","state":{},"questions":{}})
    assert str(error.value) == "http_"+str(code)
    assert error.value.retryable is retryable and error.value.retry_after == 2


@pytest.mark.parametrize("number", ["NaN", "Infinity", "-Infinity", "1e309", "-1e309"])
def test_nonfinite_wire_numbers_are_sanitized_before_recording(tmp_path, monkeypatch, number):
    transport = JevTransport(credential(tmp_path))

    class Response(io.BytesIO):
        headers = {"x-typesafe-request-id": "synthetic-request"}

    class Opener:
        def open(self, request, **kwargs):
            return Response(b'{"extra_metadata":' + number.encode() + b'}')

    monkeypatch.setattr("urllib.request.build_opener", lambda *args: Opener())
    with pytest.raises(TransportError, match="^malformed_response$"):
        transport({"model": "jev-1.13.0", "state": {}, "questions": {}})


def trial_fixture(tmp_path):
    root = tmp_path / "ontology-engineering"
    root.mkdir()
    inventory = b'{"synthetic":"inventory"}\n'
    (root / "PORTABLE-MANIFEST.json").write_bytes(inventory)
    key = tmp_path / "api-jev.md"
    key.write_text("apikey_SYNTHETIC_TEST_ONLY\n")
    key.chmod(0o644)
    manifest = {"schema":"ontology-engineering.public-jev-trial/v1",
                "authorization":"owner-approved-public-temporary-credential",
                "credential_file":"api-jev.md",
                "credential_sha256":hashlib.sha256(key.read_bytes()).hexdigest(),
                "skill_manifest_sha256":hashlib.sha256(inventory).hexdigest()}
    (tmp_path / "JEV-TRIAL-MANIFEST.json").write_text(json.dumps(manifest))
    return root, key


def test_verified_trial_key_is_usable_after_zip_extraction(tmp_path):
    root, key = trial_fixture(tmp_path)
    selected = resolve_credential_file(skill_root=root, personal_file=tmp_path/"absent")
    assert selected == key and stat.S_IMODE(key.stat().st_mode) == 0o600
    assert isinstance(JevTransport(selected), JevTransport)


def test_explicit_and_personal_credentials_take_precedence(tmp_path):
    root, key = trial_fixture(tmp_path)
    personal = tmp_path / "personal.md"
    personal.write_text("operator-owned")
    assert resolve_credential_file(skill_root=root, personal_file=personal) == personal
    explicit = tmp_path / "explicit.md"
    assert resolve_credential_file(explicit, skill_root=root, personal_file=personal) == explicit
    assert stat.S_IMODE(key.stat().st_mode) == 0o644


@pytest.mark.parametrize("change", ["key", "inventory", "declaration", "symlink"])
def test_trial_credential_drift_is_rejected(tmp_path, change):
    root, key = trial_fixture(tmp_path)
    if change == "key": key.write_text("apikey_DIFFERENT_SYNTHETIC")
    if change == "inventory": (root/"PORTABLE-MANIFEST.json").write_text("{}")
    if change == "declaration": (tmp_path/"JEV-TRIAL-MANIFEST.json").write_text("{}")
    if change == "symlink":
        alternate = tmp_path/"alternate.md"
        key.rename(alternate)
        key.symlink_to(alternate)
    with pytest.raises(ValueError):
        resolve_credential_file(skill_root=root, personal_file=tmp_path/"absent")


def test_unmarked_parent_key_is_not_silently_selected(tmp_path):
    root, _ = trial_fixture(tmp_path)
    (tmp_path/"JEV-TRIAL-MANIFEST.json").unlink()
    with pytest.raises(ValueError, match="jev_credential_missing"):
        resolve_credential_file(skill_root=root, personal_file=tmp_path/"absent")
