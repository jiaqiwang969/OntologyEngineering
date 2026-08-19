from __future__ import annotations

import base64
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import zipfile

from ontology_engineering import semantica_runtime as runtime


COMMIT = "1" * 40
VERSION = "9.9.9+staging.1"
WHEEL_NAME = "semantica-9.9.9+staging.1-py3-none-any.whl"


def write_descriptor(root: Path, wheel_bytes: bytes) -> tuple[Path, str]:
    digest = hashlib.sha256(wheel_bytes).hexdigest()
    (root / WHEEL_NAME).write_bytes(wheel_bytes)
    path = root / "semantica-staging-runtime.json"
    path.write_text(
        json.dumps(
            {
                "$schema": runtime.STAGING_RUNTIME_SCHEMA,
                "commit": COMMIT,
                "version": VERSION,
                "wheel_filename": WHEEL_NAME,
                "wheel_sha256": digest,
            },
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    return path, digest


class RuntimeIdentityVerificationTests(unittest.TestCase):
    def test_formal_identity_rejects_symlinked_lock_and_vendored_wheel(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime_dir = root / "runtime"
            vendor = runtime_dir / "vendor"
            vendor.mkdir(parents=True)
            wheel_bytes = b"controlled-formal-wheel"
            wheel_digest = hashlib.sha256(wheel_bytes).hexdigest()
            real_wheel = vendor / "real-wheel.whl"
            real_wheel.write_bytes(wheel_bytes)
            selected_wheel = vendor / WHEEL_NAME
            selected_wheel.symlink_to(real_wheel.name)
            document = {
                "$schema": runtime.SOURCE_LOCK_SCHEMA,
                "source": {"commit": COMMIT, "version": VERSION},
                "artifact": {"filename": WHEEL_NAME, "sha256": wheel_digest},
            }
            real_lock = runtime_dir / "real-source-lock.json"
            real_lock.write_text(json.dumps(document), encoding="utf-8")
            selected_lock = runtime_dir / "semantica-source-lock.json"
            selected_lock.symlink_to(real_lock.name)

            with (
                mock.patch.object(runtime, "SOURCE_LOCK_PATH", selected_lock),
                mock.patch.object(runtime, "SKILL_ROOT", root),
            ):
                with self.assertRaisesRegex(RuntimeError, "source lock.*non-symlink"):
                    runtime.read_runtime_source_lock(verify_vendored_artifact=True)

                selected_lock.unlink()
                selected_lock.write_text(json.dumps(document), encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "wheel.*non-symlink"):
                    runtime.read_runtime_source_lock(verify_vendored_artifact=True)

    def test_formal_mode_checks_vendor_and_installed_pep610_identity(self) -> None:
        selected = runtime.RuntimeSourceLock(
            commit=COMMIT,
            version=VERSION,
            artifact_filename=WHEEL_NAME,
            artifact_sha256="2" * 64,
        )
        with (
            mock.patch.object(
                runtime, "read_runtime_source_lock", return_value=selected
            ) as read_lock,
            mock.patch.object(
                runtime, "installed_runtime_version", return_value=VERSION
            ),
            mock.patch.object(
                runtime,
                "installed_runtime_artifact_sha256",
                return_value="2" * 64,
            ),
            mock.patch.object(
                runtime, "verify_installed_runtime_record", return_value=1
            ),
        ):
            self.assertEqual(selected, runtime.verify_runtime_source_identity())
        read_lock.assert_called_once_with(verify_vendored_artifact=True)

    def test_staging_mode_verifies_strict_descriptor_sibling_wheel_and_install(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            descriptor_path, digest = write_descriptor(root, b"controlled-wheel")
            formal_lock = runtime.SOURCE_LOCK_PATH.read_bytes()
            with (
                mock.patch.object(runtime, "read_runtime_source_lock") as read_lock,
                mock.patch.object(
                    runtime, "installed_runtime_version", return_value=VERSION
                ),
                mock.patch.object(
                    runtime,
                    "installed_runtime_artifact_sha256",
                    return_value=digest,
                ),
                mock.patch.object(
                    runtime, "verify_installed_runtime_record", return_value=1
                ),
            ):
                selected = runtime.verify_runtime_source_identity(
                    staging_descriptor=descriptor_path
                )
            read_lock.assert_not_called()
            self.assertEqual(COMMIT, selected.commit)
            self.assertEqual(VERSION, selected.version)
            self.assertEqual(digest, selected.artifact_sha256)
            self.assertEqual(formal_lock, runtime.SOURCE_LOCK_PATH.read_bytes())

    def test_staging_parser_rejects_unknown_fields_and_path_like_wheel_name(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path, digest = write_descriptor(root, b"controlled-wheel")
            value = json.loads(path.read_text(encoding="utf-8"))
            value["fallback"] = True
            path.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "unsupported fields"):
                runtime.read_staging_runtime_descriptor(path)

            value.pop("fallback")
            value["wheel_filename"] = "../" + WHEEL_NAME
            value["wheel_sha256"] = digest
            path.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "wheel filename"):
                runtime.read_staging_runtime_descriptor(path)

    def test_staging_mode_rejects_artifact_and_installed_identity_drift(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path, digest = write_descriptor(root, b"controlled-wheel")
            (root / WHEEL_NAME).write_bytes(b"tampered-wheel")
            with self.assertRaisesRegex(RuntimeError, "differs from its descriptor"):
                runtime.verify_runtime_source_identity(staging_descriptor=path)

            (root / WHEEL_NAME).write_bytes(b"controlled-wheel")
            with (
                mock.patch.object(
                    runtime, "installed_runtime_version", return_value="0.0.0"
                ),
                mock.patch.object(
                    runtime,
                    "installed_runtime_artifact_sha256",
                    return_value=digest,
                ),
            ):
                with self.assertRaisesRegex(
                    RuntimeError, "installed Semantica version"
                ):
                    runtime.verify_runtime_source_identity(staging_descriptor=path)

            with (
                mock.patch.object(
                    runtime, "installed_runtime_version", return_value=VERSION
                ),
                mock.patch.object(
                    runtime,
                    "installed_runtime_artifact_sha256",
                    return_value="3" * 64,
                ),
            ):
                with self.assertRaisesRegex(RuntimeError, "installed Semantica wheel"):
                    runtime.verify_runtime_source_identity(staging_descriptor=path)

    def test_wheel_record_binds_import_path_and_every_installed_package_file(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            site = root / "site-packages"
            package = site / "semantica"
            package.mkdir(parents=True)
            init_bytes = b'__version__ = "9.9.9+staging.1"\n'
            module_bytes = b"VALUE = 1\n"
            (package / "__init__.py").write_bytes(init_bytes)
            (package / "module.py").write_bytes(module_bytes)

            def record_hash(payload: bytes) -> str:
                return (
                    base64.urlsafe_b64encode(hashlib.sha256(payload).digest())
                    .decode("ascii")
                    .rstrip("=")
                )

            record = (
                "\n".join(
                    [
                        "semantica/__init__.py,sha256={},{}".format(
                            record_hash(init_bytes), len(init_bytes)
                        ),
                        "semantica/module.py,sha256={},{}".format(
                            record_hash(module_bytes), len(module_bytes)
                        ),
                        "semantica-9.9.9.dist-info/RECORD,,",
                    ]
                )
                + "\n"
            )
            wheel = root / WHEEL_NAME
            with zipfile.ZipFile(wheel, "w") as archive:
                archive.writestr("semantica/__init__.py", init_bytes)
                archive.writestr("semantica/module.py", module_bytes)
                archive.writestr(
                    "semantica-9.9.9.dist-info/RECORD", record.encode("utf-8")
                )

            class Distribution:
                version = VERSION

                @staticmethod
                def locate_file(relative: str) -> Path:
                    return site / relative

            with (
                mock.patch.object(
                    runtime.importlib_metadata,
                    "distribution",
                    return_value=Distribution(),
                ),
                mock.patch.object(
                    runtime._semantica,
                    "__file__",
                    str(package / "__init__.py"),
                ),
                mock.patch.object(runtime._semantica, "__path__", [str(package)]),
            ):
                self.assertEqual(
                    2,
                    runtime.verify_installed_runtime_record(
                        wheel, expected_version=VERSION
                    ),
                )

                shadow = root / "shadow" / "semantica"
                shadow.mkdir(parents=True)
                (shadow / "__init__.py").write_bytes(init_bytes)
                with (
                    mock.patch.object(
                        runtime._semantica,
                        "__file__",
                        str(shadow / "__init__.py"),
                    ),
                    mock.patch.object(runtime._semantica, "__path__", [str(shadow)]),
                ):
                    with self.assertRaisesRegex(RuntimeError, "outside"):
                        runtime.verify_installed_runtime_record(
                            wheel, expected_version=VERSION
                        )

                (package / "module.py").write_bytes(b"VALUE = 2\n")
                with self.assertRaisesRegex(RuntimeError, "differs from wheel RECORD"):
                    runtime.verify_installed_runtime_record(
                        wheel, expected_version=VERSION
                    )


if __name__ == "__main__":
    unittest.main()
