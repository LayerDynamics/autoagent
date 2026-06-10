"""The typed functional API (S2-T3): native JSON -> typed models, native
exceptions -> AutoAgentError. No raw JSON strings leak to the caller."""

import json
import os

import pytest

import autoagent
from autoagent import AutoAgentError, DoctorReport, RunOutcome


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


def test_version_is_int(tmp_path):
    assert autoagent.version() == 1


def test_doctor_returns_typed_report(tmp_path):
    report = autoagent.doctor(str(tmp_path))
    assert isinstance(report, DoctorReport)
    assert isinstance(report.checks, list)


def test_apply_without_approval_raises(tmp_path):
    plan = _seed(str(tmp_path))
    with pytest.raises(AutoAgentError) as e:
        autoagent.apply(str(tmp_path), plan, approve=False)
    assert e.value.code.startswith("policy")
    assert not os.path.exists(os.path.join(str(tmp_path), "crates/x.rs"))


def test_apply_then_revert_roundtrip(tmp_path):
    plan = _seed(str(tmp_path))
    run_id = autoagent.apply(str(tmp_path), plan, approve=True)
    assert run_id
    assert os.path.exists(os.path.join(str(tmp_path), "crates/x.rs"))
    autoagent.revert(str(tmp_path), run_id)
    assert not os.path.exists(os.path.join(str(tmp_path), "crates/x.rs"))


@pytest.mark.asyncio
async def test_async_run_returns_typed_outcome(tmp_path):
    plan = _seed(str(tmp_path))
    outcome = await autoagent.run(str(tmp_path), "c", plan, approve=True)
    assert isinstance(outcome, RunOutcome)
    assert outcome.run_id
