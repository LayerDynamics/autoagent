"""Drift safety (S2-T4): the generated models must cover every field the native
engine actually emits. If core adds a field but the models weren't regenerated,
the native JSON carries a key absent from the dataclass and this test fails.
(The Rust-side `bingen check` guards the generator; this guards the live shape.)
"""

import dataclasses
import json
import os

import autoagent
from autoagent._models import Check, DoctorReport, MemorySummary, ProjectAnalysis


def _fields(cls) -> set[str]:
    return {f.name for f in dataclasses.fields(cls)}


def test_doctor_native_keys_covered(tmp_path):
    raw = json.loads(autoagent._native.doctor(str(tmp_path)))
    assert set(raw.keys()) <= _fields(DoctorReport)
    if raw["checks"]:
        assert set(raw["checks"][0].keys()) <= _fields(Check)
    # and the typed wrapper produces the real model
    assert isinstance(autoagent.doctor(str(tmp_path)), DoctorReport)


def test_analyze_native_keys_covered(tmp_path):
    autoagent.init(str(tmp_path))
    raw = json.loads(autoagent._native.analyze(str(tmp_path)))
    extra = set(raw.keys()) - _fields(ProjectAnalysis)
    assert not extra, f"core ProjectAnalysis grew fields not in the model: {extra}"
    pa = autoagent.analyze(str(tmp_path))
    assert pa.root and pa.language is not None and isinstance(pa.file_count, int)


def test_memory_native_keys_covered(tmp_path):
    autoagent.init(str(tmp_path))
    raw = json.loads(autoagent._native.memory_show(str(tmp_path)))
    extra = set(raw.keys()) - _fields(MemorySummary)
    assert not extra, f"core MemorySummary grew fields not in the model: {extra}"


def test_package_layout():
    # py.typed marker present (PEP 561) and native nested correctly
    here = os.path.dirname(__file__)
    assert os.path.exists(os.path.join(here, "..", "autoagent", "py.typed"))
    assert hasattr(autoagent, "_native")
