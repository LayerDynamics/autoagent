"""Python smoke test (B2-T3): the pyo3 abi3 module loads and the read surface
returns sane values straight from autoagent-core."""

import json

import autoagent


def test_version_is_schema_version():
    assert autoagent.version() == 1


def test_doctor_returns_checks(tmp_path):
    report = json.loads(autoagent.doctor(str(tmp_path)))
    assert "checks" in report
    assert isinstance(report["checks"], list)


def test_config_show_after_init(tmp_path):
    autoagent.init(str(tmp_path))
    assert "[agent]" in autoagent.config_show(str(tmp_path))


def test_patch_list_empty_is_array(tmp_path):
    assert json.loads(autoagent.patch_list(str(tmp_path))) == []
