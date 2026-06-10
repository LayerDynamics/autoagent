"""The SDK's typed error. The native extension raises an ``AutoAgentError``
whose message is ``[code|exit_code] message``; this richer class exposes the
stable ``code`` and numeric ``exit_code`` from core's taxonomy (FR-8)."""

from __future__ import annotations

import re

_PATTERN = re.compile(r"^\[([^|]+)\|(-?\d+)\]\s*(.*)$", re.S)


class AutoAgentError(Exception):
    """A failure surfaced by the AutoAgent engine.

    Attributes:
        code: Stable error code, e.g. ``policy.path_escape`` or ``plan``.
        exit_code: Numeric process exit code from core's error taxonomy.
    """

    code: str
    exit_code: int

    def __init__(self, message: str, code: str = "", exit_code: int = 1) -> None:
        super().__init__(message)
        self.code = code
        self.exit_code = exit_code

    @classmethod
    def from_native(cls, exc: BaseException) -> "AutoAgentError":
        """Build from a native ``_native.AutoAgentError`` (or any exception),
        parsing the ``[code|exit_code] message`` shape."""
        text = str(exc)
        m = _PATTERN.match(text)
        if m:
            return cls(m.group(3), code=m.group(1), exit_code=int(m.group(2)))
        return cls(text, code="", exit_code=1)
