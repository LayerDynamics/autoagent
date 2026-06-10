"""The AutoAgent client class (S2-T4)."""

import json
import os

import pytest

from autoagent import AutoAgent, AutoAgentError, DoctorReport, RunOutcome


def _seed(root: str) -> str:
    AutoAgent(root).init()
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


def test_client_doctor(tmp_path):
    aa = AutoAgent(str(tmp_path))
    assert isinstance(aa.doctor(), DoctorReport)


def test_client_apply_refused_without_approval(tmp_path):
    aa = AutoAgent(str(tmp_path))
    plan = _seed(str(tmp_path))
    with pytest.raises(AutoAgentError) as e:
        aa.apply(plan, approve=False)
    assert e.value.code.startswith("policy")


async def test_client_async_run(tmp_path):
    aa = AutoAgent(str(tmp_path))
    plan = _seed(str(tmp_path))
    outcome = await aa.run("c", plan, approve=True)
    assert isinstance(outcome, RunOutcome)
    assert outcome.run_id
