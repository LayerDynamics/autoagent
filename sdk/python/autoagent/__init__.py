"""AutoAgent — typed Python SDK over the native bindings.

The native pyo3 extension (``autoagent._native``) returns JSON strings; this
module parses them into the generated typed models and maps native errors to
:class:`AutoAgentError`. Both a functional API (``autoagent.doctor(root)``) and
the :class:`AutoAgent` client class are provided. Mutating operations preserve
the engine's fail-closed safety: an unapproved op raises ``AutoAgentError``.
"""

from __future__ import annotations

import json
from typing import Optional

from . import _native
from ._models import (
    Check,
    CommandValidationResult,
    DependencySummary,
    DoctorReport,
    EvolveOutcome,
    MemorySummary,
    ProjectAnalysis,
    RunOutcome,
    RunState,
    ValidationReport,
)
from .errors import AutoAgentError

__all__ = [
    "AutoAgent",
    "AutoAgentError",
    "Check",
    "CommandValidationResult",
    "DependencySummary",
    "DoctorReport",
    "EvolveOutcome",
    "MemorySummary",
    "ProjectAnalysis",
    "RunOutcome",
    "RunState",
    "ValidationReport",
    "analyze",
    "apply",
    "config_show",
    "doctor",
    "evolve",
    "evolve_sync",
    "init",
    "memory_show",
    "patch_list",
    "patch_show",
    "revert",
    "run",
    "run_sync",
    "tools_list",
    "version",
]


def _call(fn, *args):
    """Invoke a native function, re-raising native errors as AutoAgentError."""
    try:
        return fn(*args)
    except AutoAgentError:
        raise
    except Exception as e:  # native _native.AutoAgentError or anything else
        raise AutoAgentError.from_native(e) from None


# --- read surface ---------------------------------------------------------

def version() -> int:
    """Schema version this build supports."""
    return _native.version()


def doctor(root: str) -> DoctorReport:
    """System, config, and workspace health checks."""
    return DoctorReport.from_dict(json.loads(_call(_native.doctor, root)))


def analyze(root: str) -> ProjectAnalysis:
    """Analyze the project and write its report."""
    return ProjectAnalysis.from_dict(json.loads(_call(_native.analyze, root)))


def config_show(root: str) -> str:
    """Render the effective ``Autoagent.toml`` (raw TOML)."""
    return _call(_native.config_show, root)


def patch_list(root: str) -> list[str]:
    """List patch-artifact run ids."""
    return json.loads(_call(_native.patch_list, root))


def patch_show(root: str, run_id: str) -> str:
    """Show a patch body (raw unified diff)."""
    return _call(_native.patch_show, root, run_id)


def memory_show(root: str) -> MemorySummary:
    """Project-memory summary."""
    return MemorySummary.from_dict(json.loads(_call(_native.memory_show, root)))


def tools_list(root: str) -> list[str]:
    """Registered plugin tools."""
    return json.loads(_call(_native.tools_list, root))


# --- mutating surface (fail-closed) ---------------------------------------

def init(root: str) -> bool:
    """Initialize ``Autoagent.toml`` + the ``.agent/`` tree."""
    return _call(_native.init, root)


def apply(root: str, plan_path: str, *, approve: bool = False) -> str:
    """Apply a plan through the policy engine; returns the run id. Refuses with
    ``AutoAgentError`` when ``approve`` is False and the config requires it."""
    return _call(_native.apply, root, plan_path, approve)


def revert(root: str, run_id: str) -> None:
    """Revert a previous run."""
    _call(_native.revert, root, run_id)


def run_sync(
    root: str, objective: str, from_: Optional[str] = None, *, approve: bool = False
) -> RunOutcome:
    """Supervised run (blocking) from a plan or generated objective."""
    return RunOutcome.from_dict(
        json.loads(_call(_native.run_sync, root, objective, from_, approve))
    )


def evolve_sync(
    root: str, objective: str, from_: Optional[str] = None, *, apply: bool = False
) -> EvolveOutcome:
    """Controlled self-authoring (blocking); ``apply`` is gated by
    ``allow_self_modification``."""
    return EvolveOutcome.from_dict(
        json.loads(_call(_native.evolve_sync, root, objective, from_, apply))
    )


async def run(
    root: str, objective: str, from_: Optional[str] = None, *, approve: bool = False
) -> RunOutcome:
    """Supervised run (awaitable)."""
    try:
        out = await _native.run(root, objective, from_, approve)
    except AutoAgentError:
        raise
    except Exception as e:
        raise AutoAgentError.from_native(e) from None
    return RunOutcome.from_dict(json.loads(out))


async def evolve(
    root: str, objective: str, from_: Optional[str] = None, *, apply: bool = False
) -> EvolveOutcome:
    """Controlled self-authoring (awaitable)."""
    try:
        out = await _native.evolve(root, objective, from_, apply)
    except AutoAgentError:
        raise
    except Exception as e:
        raise AutoAgentError.from_native(e) from None
    return EvolveOutcome.from_dict(json.loads(out))


from .client import AutoAgent  # noqa: E402  (depends on the functions above)
