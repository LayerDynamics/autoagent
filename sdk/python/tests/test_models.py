"""The generated dataclasses parse native JSON into typed objects (S2-T2)."""

import json

from autoagent._models import DoctorReport, RunOutcome
from autoagent.errors import AutoAgentError


def test_doctor_report_from_dict():
    d = DoctorReport.from_dict(
        json.loads('{"checks":[{"name":"x","ok":true,"detail":"d"}]}')
    )
    assert d.checks[0].name == "x"
    assert d.checks[0].ok is True


def test_run_outcome_nested_from_dict():
    o = RunOutcome.from_dict(
        json.loads(
            '{"run_id":"r1","final_state":"Completed",'
            '"report":{"passed":true,"commands":'
            '[{"command":"cargo build","exit_code":0,"stdout":"","stderr":"","duration_ms":12}]}}'
        )
    )
    assert o.run_id == "r1"
    assert o.final_state == "Completed"
    assert o.report.passed is True
    assert o.report.commands[0].command == "cargo build"


def test_autoagent_error_parses_native_message():
    err = AutoAgentError.from_native(Exception("[policy.write_not_approved|4] denied"))
    assert err.code == "policy.write_not_approved"
    assert err.exit_code == 4
    assert "denied" in str(err)
