//! The CallbackGate bridges core's `ApprovalGate` to a host callback. These
//! tests pin the fail-closed contract (FR-7 / FR-20): absent an affirmative
//! decision, every privileged op is refused.

use autoagent_bingen::bind::CallbackGate;
use autoagent_core::safety::approval_gate::ApprovalGate;

#[test]
fn deny_all_refuses_write_and_command() {
    let gate = CallbackGate::deny_all();
    assert!(gate.confirm_write("crates/x.rs").is_err());
    assert!(gate.confirm_command("cargo test").is_err());
}

#[test]
fn callback_false_denies() {
    let gate = CallbackGate::from_fn(|_req| false);
    assert!(gate.confirm_command("cargo test").is_err());
}

#[test]
fn callback_true_allows() {
    let gate = CallbackGate::from_fn(|_req| true);
    assert!(gate.confirm_write("crates/x.rs").is_ok());
    assert!(gate.confirm_command("cargo test").is_ok());
}

#[test]
fn approve_all_allows_without_callback() {
    let gate = CallbackGate::approve_all();
    assert!(gate.confirm_write("x").is_ok());
}

#[test]
fn denial_carries_policy_code() {
    // A refusal must surface as a policy error so the host sees `policy.*`.
    let gate = CallbackGate::deny_all();
    let err = gate.confirm_write("crates/x.rs").unwrap_err();
    assert!(
        err.error_code().starts_with("policy"),
        "got {}",
        err.error_code()
    );
}
