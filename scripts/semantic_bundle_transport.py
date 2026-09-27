"""Compatibility imports for the package-owned Semantica bundle transport.

The implementation lives in ontology_engineering.semantic_bundle_transport.
Keep the historical script import usable for existing packaging and CLI callers.
"""

from pathlib import Path
import sys

SKILL_ROOT = Path(__file__).resolve().parents[1]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from ontology_engineering.semantic_bundle_transport import (
    ROOT, LOCK_PATH, MAX_EXPANDED_BYTES, BundleError, digest, safe_relative,
    _regular_inside, validate_data_archive, validate_archive, load_bundle,
    materialized,
)
