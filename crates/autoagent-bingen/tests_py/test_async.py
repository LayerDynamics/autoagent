"""FR-5: pyo3 async run/evolve are awaitable (pyo3-async-runtimes), running the
blocking core on a worker thread. Verified through real asyncio."""

import asyncio
import json
import os

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


def test_async_run_returns_outcome(tmp_path):
    plan = _seed(str(tmp_path))

    async def main():
        return json.loads(await autoagent.run(str(tmp_path), "c", plan, True))

    outcome = asyncio.run(main())
    assert "run_id" in outcome
    assert "final_state" in outcome
