"""Locations owned by this installed skill; private work stays under var/."""
from pathlib import Path

SKILL_ROOT = Path(__file__).resolve().parents[1]


def skill_path(path, label="path", *, root=None):
    """Resolve a persistent location without allowing symlinks to escape."""
    base = Path(root if root is not None else SKILL_ROOT).resolve()
    resolved = Path(path).expanduser().resolve()
    if not resolved.is_relative_to(base) or resolved == base:
        raise ValueError(label + "_must_be_inside_skill")
    return resolved


def private_path(path, label="path", *, root=None):
    """Keep mutable task records out of the reusable code and source payload."""
    base = Path(root if root is not None else SKILL_ROOT).resolve()
    resolved = skill_path(path, label, root=base)
    if not resolved.is_relative_to(base / "var"):
        raise ValueError(label + "_must_be_inside_skill_var")
    return resolved
