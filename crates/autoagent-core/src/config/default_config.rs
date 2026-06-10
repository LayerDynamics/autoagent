//! The canonical default `Autoagent.toml` (SPEC-1 §6 / Appendix C).
//! Written by `init` and used as the reference schema in tests.

pub fn default_toml() -> String {
    r#"[project]
name = "autoagent"
type = "rust-cli"
language = "rust"
package_manager = "cargo"

[agent]
mode = "supervised"
allow_self_modification = false
max_steps_per_run = 12
require_approval_before_write = true
require_approval_before_command = true

[workspace]
root = "."
include = [
  "crates/**/*.rs",
  "src/**/*.rs",
  "tests/**/*.rs",
  "Cargo.toml",
  "README.md",
  "Autoagent.toml",
]
exclude = [
  "target/**",
  ".git/**",
  ".agent/runs/**",
  ".agent/patches/**",
  ".agent/logs/**",
  ".env",
  ".env.*",
]

[commands]
test = "cargo test"
lint = "cargo clippy --all-targets --all-features -- -D warnings"
format = "cargo fmt --all -- --check"
build = "cargo build"

[safety]
allowed_write_paths = [
  "crates/",
  "src/",
  "tests/",
  "README.md",
  "Cargo.toml",
  "Autoagent.toml",
]
blocked_write_paths = [
  ".git/",
  "target/",
  ".env",
  ".env.local",
  ".ssh/",
  "/",
  "../",
]
allowed_commands = [
  "cargo test",
  "cargo build",
  "cargo fmt --all -- --check",
  "cargo clippy --all-targets --all-features -- -D warnings",
  "git status",
  "git diff",
  "git branch",
  "git checkout",
]
blocked_commands = [
  "sudo",
  "rm -rf /",
  "curl",
  "wget",
  "ssh",
  "scp",
  "chmod 777",
  "chown",
]

[memory]
enabled = true
directory = ".agent/memory"

[logging]
directory = ".agent/logs"
level = "info"

[patches]
directory = ".agent/patches"
create_before_write = true

[runs]
directory = ".agent/runs"
"#
    .to_string()
}
