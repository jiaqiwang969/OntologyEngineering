from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
import zipfile


ROOT = Path(__file__).resolve().parents[1]

from runtime import doctor_runtime as DOCTOR  # noqa: E402


class RuntimeDoctorTests(unittest.TestCase):
    def _runtime_fixture(self, root: Path) -> tuple[Path, dict[str, object]]:
        runtime = root / "runtime with spaces"
        vendor = runtime / "vendor"
        vendor.mkdir(parents=True)
        version = "0.6.5+portable.1"
        filename = f"semantica-{version}-py3-none-any.whl"
        wheel = vendor / filename
        dist_info = f"semantica-{version}.dist-info"
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr(
                f"{dist_info}/METADATA",
                "\n".join(
                    (
                        "Metadata-Version: 2.4",
                        "Name: semantica",
                        f"Version: {version}",
                        "Requires-Python: >=3.8",
                        "",
                    )
                ),
            )
            archive.writestr(
                f"{dist_info}/WHEEL",
                "\n".join(
                    (
                        "Wheel-Version: 1.0",
                        "Generator: portability-test",
                        "Root-Is-Purelib: true",
                        "Tag: py3-none-any",
                        "",
                    )
                ),
            )
        digest = hashlib.sha256(wheel.read_bytes()).hexdigest()
        document: dict[str, object] = {
            "$schema": DOCTOR.LOCK_SCHEMA,
            "source": {
                "commit": "a" * 40,
                "version": version,
            },
            "artifact": {
                "filename": filename,
                "sha256": digest,
            },
        }
        (runtime / DOCTOR.LOCK_NAME).write_text(json.dumps(document), encoding="utf-8")
        return runtime, document

    @staticmethod
    def _errors(checks: list[DOCTOR.Check]) -> dict[str, str]:
        return {item.code: item.message for item in checks if item.level == "error"}

    @staticmethod
    def _fake_venv_install(
        runtime: Path, *, version: str, artifact_sha256: str
    ) -> None:
        venv = runtime / ".venv"
        binary = runtime / ".venv" / "bin" / "python"
        binary.parent.mkdir(parents=True)
        binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        binary.chmod(0o755)
        (venv / "pyvenv.cfg").write_text(
            "home = /portable/python\nversion = 3.13.0\n", encoding="utf-8"
        )
        dist_info = (
            venv
            / "lib"
            / "python3.13"
            / "site-packages"
            / f"semantica-{version}.dist-info"
        )
        dist_info.mkdir(parents=True)
        (dist_info / "METADATA").write_text(
            "\n".join(
                (
                    "Metadata-Version: 2.4",
                    "Name: semantica",
                    f"Version: {version}",
                    "",
                )
            ),
            encoding="utf-8",
        )
        (dist_info / "direct_url.json").write_text(
            json.dumps(
                {
                    "archive_info": {
                        "hash": f"sha256={artifact_sha256}",
                        "hashes": {"sha256": artifact_sha256},
                    },
                    "url": "file:///old/location/semantica.whl",
                }
            ),
            encoding="utf-8",
        )

    def test_preflight_is_non_mutating_and_supports_space_in_path(self) -> None:
        with tempfile.TemporaryDirectory(prefix="runtime doctor parent ") as temporary:
            runtime, _ = self._runtime_fixture(Path(temporary))
            shutil.copy2(ROOT / "runtime" / "doctor_runtime.py", runtime)
            shutil.copy2(ROOT / "runtime" / "setup_runtime.sh", runtime)
            before = sorted(path.relative_to(runtime) for path in runtime.rglob("*"))
            checks = DOCTOR.run_checks(runtime, include_venv=False)
            completed = subprocess.run(
                ["bash", str(runtime / "setup_runtime.sh"), "--preflight"],
                cwd=Path(temporary),
                env=dict(os.environ),
                check=False,
                capture_output=True,
                text=True,
            )
            after = sorted(path.relative_to(runtime) for path in runtime.rglob("*"))

            self.assertEqual(self._errors(checks), {})
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(before, after)
            self.assertFalse((runtime / ".venv").exists())
            self.assertTrue(any(item.code == "offline_boundary" for item in checks))

    def test_preflight_rejects_stale_lock_digest(self) -> None:
        with tempfile.TemporaryDirectory(prefix="runtime stale lock ") as temporary:
            runtime, document = self._runtime_fixture(Path(temporary))
            artifact = document["artifact"]
            assert isinstance(artifact, dict)
            artifact["sha256"] = "0" * 64
            (runtime / DOCTOR.LOCK_NAME).write_text(
                json.dumps(document), encoding="utf-8"
            )

            errors = self._errors(DOCTOR.run_checks(runtime, include_venv=False))
            self.assertIn("wheel_hash", errors)

    def test_preflight_rejects_symlinked_lock_and_wheel(self) -> None:
        with tempfile.TemporaryDirectory(prefix="runtime symlink guard ") as temporary:
            runtime, document = self._runtime_fixture(Path(temporary))
            lock = runtime / DOCTOR.LOCK_NAME
            real_lock = runtime / "real-source-lock.json"
            lock.replace(real_lock)
            lock.symlink_to(real_lock.name)
            errors = self._errors(DOCTOR.run_checks(runtime, include_venv=False))
            self.assertIn("lock_invalid", errors)

            lock.unlink()
            lock.write_text(json.dumps(document), encoding="utf-8")
            artifact = document["artifact"]
            assert isinstance(artifact, dict)
            wheel = runtime / "vendor" / str(artifact["filename"])
            real_wheel = runtime / "vendor" / "real-semantica.whl"
            wheel.replace(real_wheel)
            wheel.symlink_to(real_wheel.name)
            errors = self._errors(DOCTOR.run_checks(runtime, include_venv=False))
            self.assertIn("wheel_missing", errors)

    def test_doctor_detects_same_version_installed_from_stale_wheel(self) -> None:
        with tempfile.TemporaryDirectory(prefix="runtime stale install ") as temporary:
            runtime, document = self._runtime_fixture(Path(temporary))
            source = document["source"]
            artifact = document["artifact"]
            assert isinstance(source, dict)
            assert isinstance(artifact, dict)
            self._fake_venv_install(
                runtime,
                version=str(source["version"]),
                artifact_sha256="f" * 64,
            )

            errors = self._errors(DOCTOR.run_checks(runtime, include_venv=True))
            self.assertIn("installed_hash", errors)
            self.assertNotIn("installed_version", errors)

    def test_doctor_detects_incomplete_venv(self) -> None:
        with tempfile.TemporaryDirectory(prefix="runtime stale venv ") as temporary:
            runtime, document = self._runtime_fixture(Path(temporary))
            source = document["source"]
            artifact = document["artifact"]
            assert isinstance(source, dict)
            assert isinstance(artifact, dict)
            self._fake_venv_install(
                runtime,
                version=str(source["version"]),
                artifact_sha256=str(artifact["sha256"]),
            )
            shutil.rmtree(runtime / ".venv" / "bin")
            errors = self._errors(DOCTOR.run_checks(runtime, include_venv=True))
            self.assertIn("venv_python", errors)

    def test_setup_refuses_incomplete_venv_without_deleting_it(self) -> None:
        with tempfile.TemporaryDirectory(prefix="runtime setup guard ") as temporary:
            root = Path(temporary)
            runtime, _ = self._runtime_fixture(root)
            shutil.copy2(ROOT / "runtime" / "doctor_runtime.py", runtime)
            shutil.copy2(ROOT / "runtime" / "setup_runtime.sh", runtime)
            stale = runtime / ".venv"
            stale.mkdir()
            marker = stale / "user-marker"
            marker.write_text("preserve\n", encoding="utf-8")

            completed = subprocess.run(
                ["bash", str(runtime / "setup_runtime.sh")],
                cwd=root,
                env=dict(os.environ),
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("runtime venv is incomplete", completed.stderr)
            self.assertTrue(marker.is_file())


if __name__ == "__main__":
    unittest.main()
