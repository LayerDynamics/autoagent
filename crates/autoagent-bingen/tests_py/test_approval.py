"""Fail-closed approval through the real pyo3 backend (FR-7 / FR-20): a
privileged op with approve=False must raise a policy error and not mutate."""

import json
import os

import pytest

import _native as autoagent


def _seed(root: str) -> str:
    autoagent.init(root)
    plan = os.path.join(root, "p.json")
    with open(plan, "w") as f:
        f.write(
            json.dumps(
                {
                    "objective": "c",
                    "summary": "s",
                    "files_to_read": [],
                    "files_to_create": [{"path": "crates/x.rs", "purpose": "p"}],
                    "files_to_modify": [],
                    "operations": [
                        {
                            "kind": "Create",
                            "path": "crates/x.rs",
                            "destination_path": None,
                            "reason": "r",
                            "before_hash": None,
                            "after_hash": None,
                            "content": "// x",
                        }
                    ],
                    "validation_commands": [],
                    "risks": [],
                    "rollback_strategy": "snapshot",
                }
            )
        )
    return plan


def test_apply_without_approval_refused(tmp_path):
    plan = _seed(str(tmp_path))
    with pytest.raises(Exception) as e:
        autoagent.apply(str(tmp_path), plan, False)
    assert "policy" in str(e.value).lower()
    assert not os.path.exists(os.path.join(str(tmp_path), "crates/x.rs"))


def test_apply_with_approval_succeeds(tmp_path):
    plan = _seed(str(tmp_path))
    run_id = autoagent.apply(str(tmp_path), plan, True)
    assert run_id
    assert os.path.exists(os.path.join(str(tmp_path), "crates/x.rs"))
