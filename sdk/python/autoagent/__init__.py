"""AutoAgent — typed Python SDK over the native bindings.

The native extension is installed as ``autoagent._native``; the typed functional
API and the ``AutoAgent`` client class (added in S2-T3/S2-T4) wrap it. For now
this re-exports the native module and the generated models.
"""

from . import _native  # noqa: F401  (the compiled pyo3 extension)
from ._models import (  # noqa: F401
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
