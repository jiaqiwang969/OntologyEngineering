"""A public trial credential is an explicit outer artifact, never a source exception."""
import hashlib
import json
from pathlib import Path
import stat
import zipfile

import pytest

from scripts.package_jev_trial import build
from scripts.check_public_privacy import CONTENT_RULES
from ontology_engineering.jev_transport import JevTransport, resolve_credential_file


def portable(tmp_path, *, tamper=False):
    path = tmp_path/"base.zip"
    files = {"SKILL.md":b"synthetic skill", "docs/JEV-TRIAL.md":b"synthetic trial guide"}
    manifest = {"format":"ontology-engineering.portable-skill/v1", "files":[
        {"path":name,"sha256":hashlib.sha256(raw).hexdigest()} for name,raw in files.items()]}
    with zipfile.ZipFile(path,"w") as archive:
        for name,raw in files.items():
            archive.writestr("ontology-engineering/"+name, b"changed" if tamper and name=="SKILL.md" else raw)
        archive.writestr("ontology-engineering/PORTABLE-MANIFEST.json",json.dumps(manifest))
    return path


def test_public_trial_wrap_preserves_skill_and_only_extracts_the_key(tmp_path):
    base = portable(tmp_path)
    key = tmp_path/"credential.md"
    token = "apikey_"+"0"*32+"_"+"1"*64
    key.write_text("Local notes must not travel.\n"+token+"\n")
    key.chmod(0o600)
    out = tmp_path/"trial.zip"
    report = build(base,key,out)
    assert report["public_trial_credential_included"] and token not in json.dumps(report)
    with zipfile.ZipFile(base) as original, zipfile.ZipFile(out) as trial:
        assert all(original.read(n)==trial.read(n) for n in original.namelist())
        assert set(trial.namelist())-set(original.namelist()) == {"api-jev.md","JEV-TRIAL-MANIFEST.json","TRY-JEV.md"}
        assert trial.read("api-jev.md").decode() == token+"\n"
        assert stat.S_IMODE(trial.getinfo("api-jev.md").external_attr >> 16) == 0o600
        destination = tmp_path/"recipient"
        trial.extractall(destination)
    chosen = resolve_credential_file(skill_root=destination/"ontology-engineering",personal_file=tmp_path/"absent")
    assert isinstance(JevTransport(chosen),JevTransport)
    assert any(r.name=="Jev API token" and r.pattern.search(token) for r in CONTENT_RULES)


def test_changed_base_package_is_not_wrapped(tmp_path):
    base = portable(tmp_path,tamper=True)
    key = tmp_path/"credential.md";key.write_text("apikey_SYNTHETIC");key.chmod(0o600)
    out = tmp_path/"trial.zip"
    with pytest.raises(ValueError,match="archive_member_digest_mismatch"):
        build(base,key,out)
    assert not out.exists()
