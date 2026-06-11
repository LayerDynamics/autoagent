"""SDK ``replay`` (functional + client): reproduces a recorded session through
the typed wrapper, stays fail-closed without approval, and maps native errors to
``AutoAgentError``. The session is laid down on disk in the on-disk session
format the engine records, so this exercises the real reproduce path end-to-end.
"""

import json
import os

import pytest

import autoagent
from autoagent import AutoAgent, AutoAgentError, RunOutcome

_PLAN = {
    "objective": "build x",
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
            "content": "// x\n",
        }
    ],
    "validation_commands": [],
    "risks": [],
    "rollback_strategy": "snapshot",
}

_SESSION_ID = "20200101T000000Z-build-x"


def _record_session(root: str, session_id: str = _SESSION_ID) -> str:
    """Init the workspace and lay down a one-step recorded session on disk."""
    AutoAgent(root).init()
    sdir = os.path.join(root, ".agent", "sessions", session_id)
    os.makedirs(sdir, exist_ok=True)
    with open(os.path.join(sdir, "session.json"), "w") as f:
        json.dump(
            {
                "session_id": session_id,
                "objective": "build x",
                "created": "20200101T000000Z",
                "steps": 1,
            },
            f,
        )
    with open(os.path.join(sdir, "step-001.plan.json"), "w") as f:
        json.dump(_PLAN, f)
    return session_id


def test_replay_reproduces_recorded_change(tmp_path):
    root = str(tmp_path)
    sid = _record_session(root)
    outcome = autoagent.replay(root, sid, approve=True)
    assert isinstance(outcome, RunOutcome)
    assert outcome.final_state == "Completed"
    assert outcome.run_id
    with open(os.path.join(root, "crates", "x.rs")) as f:
        assert f.read() == "// x\n"


def test_replay_refused_without_approval(tmp_path):
    root = str(tmp_path)
    sid = _record_session(root)
    with pytest.raises(AutoAgentError) as e:
        autoagent.replay(root, sid, approve=False)
    assert e.value.code.startswith("policy")
    assert not os.path.exists(os.path.join(root, "crates", "x.rs"))


def test_replay_unknown_session_raises(tmp_path):
    root = str(tmp_path)
    AutoAgent(root).init()
    with pytest.raises(AutoAgentError):
        autoagent.replay(root, "nope-not-a-session", approve=True)


def test_client_replay_reproduces(tmp_path):
    root = str(tmp_path)
    sid = _record_session(root)
    outcome = AutoAgent(root).replay(sid, approve=True)
    assert isinstance(outcome, RunOutcome)
    assert outcome.final_state == "Completed"
    assert os.path.exists(os.path.join(root, "crates", "x.rs"))
