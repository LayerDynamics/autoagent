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
autonomous = false
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

# [llm] is optional. Omitted, AutoAgent uses a local Ollama server — no code
# egress. Uncomment and set `provider` to choose another backend:
#
# Local (on-machine, no code leaves the host):
#   provider = "local"             # Ollama          (default; http://localhost:11434)
#   provider = "lmstudio"          # LM Studio        (http://localhost:1234/v1)
#   provider = "huggingface-local" # self-hosted TGI  (http://localhost:8080/v1)
# Cloud (requires code_egress_opt_in = true + an API key from the environment):
#   provider = "openai"            # OPENAI_API_KEY
#   provider = "anthropic"         # ANTHROPIC_API_KEY
#   provider = "huggingface"       # HF_TOKEN  (hosted Inference API)
#
# [llm]
# provider = "lmstudio"
# model = "qwen2.5-coder"
# endpoint = "http://localhost:1234/v1"   # override the default for your server
# code_egress_opt_in = false              # must be true for cloud providers
"#
    .to_string()
}
