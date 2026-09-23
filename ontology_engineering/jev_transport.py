"""Bounded Jev HTTP transport. No ontology, fact admission or semantic rules.

Wire contract: https://docs.typesafe.ai/api (checked 2026-09-23).
Credentials are read from an owner-only local file and never enter records.
"""
from __future__ import annotations

import json
import math
import os
from pathlib import Path
import re
import stat
import urllib.error
import urllib.request

ENDPOINT = "https://api.typesafe.ai/v1/systemone"
ADAPTER_VERSION = "1.0.0"


class TransportError(RuntimeError):
    def __init__(self, code, *, retryable=False, retry_after=None):
        super().__init__(code)
        self.code, self.retryable, self.retry_after = code, retryable, retry_after


def strict_json(raw):
    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise ValueError("duplicate_json_key")
            result[key] = value
        return result
    def invalid(_):
        raise ValueError("nonfinite_json_number")
    return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid)


def pinned_model(model):
    if not isinstance(model, str) or not re.fullmatch(r"jev-\d+\.\d+\.\d+", model):
        raise ValueError("exact_model_version_required")
    return model


def _probability(value):
    return type(value) in (int, float) and math.isfinite(value) and 0 <= value <= 1


def validate_response(response, payload):
    """Return valid answers and per-question errors; preserve partial successes.

Only Choice is enabled by the first controlled question catalog. Adding another
    primitive requires its own validation and calibration, not a guessed threshold.
    """
    expected = payload["questions"]
    if not isinstance(response, dict) or response.get("model") != payload["model"]:
        raise TransportError("resolved_model_mismatch")
    answers = response.get("answers")
    if not isinstance(answers, dict) or set(answers) - set(expected):
        raise TransportError("unexpected_answer_identity")
    usage = response.get("usage")
    if not isinstance(usage, dict):
        raise TransportError("usage_missing")
    for name in ("input_tokens", "output_tokens"):
        value = usage.get(name)
        if value is not None and (type(value) is not int or value < 0):
            raise TransportError("invalid_usage")
    valid, errors = {}, {}
    for qid, question in expected.items():
        answer = answers.get(qid)
        if not isinstance(answer, dict):
            errors[qid] = "answer_missing"
            continue
        if question.get("type") != "choice" or answer.get("type") != "choice":
            errors[qid] = "answer_type_mismatch"
            continue
        probabilities = answer.get("probabilities")
        if not isinstance(probabilities, dict) or set(probabilities) != set(question["criteria"]):
            errors[qid] = "answer_options_mismatch"
            continue
        if not all(_probability(p) for p in probabilities.values()) or not _probability(answer.get("confidence")):
            errors[qid] = "invalid_probability"
            continue
        if abs(sum(probabilities.values()) - 1) > 0.015:
            errors[qid] = "probability_sum_mismatch"
            continue
        choice = answer.get("choice")
        if choice not in probabilities or probabilities[choice] + 1e-9 < max(probabilities.values()):
            errors[qid] = "choice_not_maximum"
            continue
        valid[qid] = answer
    return valid, errors


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class JevTransport:
    """Fixed official endpoint; key never appears in repr, exceptions or output."""
    identity = "typesafe-http/v1"
    kind = "live"

    def __init__(self, credential_file, *, timeout=25):
        path = Path(credential_file).expanduser()
        if path.is_symlink():
            raise ValueError("credential_file_symlink")
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        try:
            info = os.fstat(fd)
            if not stat.S_ISREG(info.st_mode) or stat.S_IMODE(info.st_mode) != 0o600 or info.st_uid != os.getuid():
                raise ValueError("credential_file_requires_owner_and_mode_0600")
            with os.fdopen(fd, "r", encoding="utf-8", closefd=False) as stream:
                content = stream.read(65537)
            if len(content) > 65536:
                raise ValueError("credential_file_too_large")
        finally:
            os.close(fd)
        matches = set(re.findall(r"apikey_[A-Za-z0-9_-]+", content))
        if len(matches) != 1:
            raise ValueError("credential_file_requires_one_unique_key")
        self._key = matches.pop()
        self.timeout = timeout

    def __call__(self, payload):
        pinned_model(payload.get("model"))
        raw = json.dumps(payload, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode()
        if b"apikey_" in raw or self._key.encode() in raw:
            raise TransportError("credential_material_in_request")
        request = urllib.request.Request(ENDPOINT, data=raw, method="POST", headers={
            "Authorization":"Bearer " + self._key, "Content-Type":"application/json"})
        try:
            # Disable redirects so authorization can never be forwarded elsewhere.
            with urllib.request.build_opener(_NoRedirect()).open(request, timeout=self.timeout) as handle:
                data = handle.read(2 * 1024 * 1024 + 1)
                if len(data) > 2 * 1024 * 1024:
                    raise TransportError("response_too_large")
                if self._key.encode() in data:
                    raise TransportError("credential_material_in_response")
                result = strict_json(data)
                request_id = handle.headers.get("x-typesafe-request-id")
                if request_id and re.fullmatch(r"[A-Za-z0-9_.:-]{1,200}", request_id):
                    result["provider_request_id"] = request_id
                return result
        except urllib.error.HTTPError as exc:
            retry_after = exc.headers.get("Retry-After") if exc.headers else None
            retry_after = float(retry_after) if retry_after and re.fullmatch(r"\d+(\.\d+)?", retry_after) else None
            raise TransportError("http_" + str(exc.code), retryable=exc.code in (429, 500, 502, 503, 504, 529), retry_after=retry_after) from None
        except (urllib.error.URLError, TimeoutError, OSError):
            raise TransportError("connection_or_timeout", retryable=True) from None
        except (ValueError, UnicodeError, TypeError):
            raise TransportError("malformed_response") from None
