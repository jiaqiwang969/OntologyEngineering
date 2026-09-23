"""Credential containment and bounded HTTP failure behavior, with synthetic keys."""
import io
import os
import urllib.error

import pytest

from ontology_engineering.jev_transport import JevTransport, TransportError, _NoRedirect


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
