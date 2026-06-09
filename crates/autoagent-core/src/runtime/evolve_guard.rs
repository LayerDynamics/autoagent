//! Evolve guard (M6, SPEC-1 FR-23 / §3.7). Enforces the product's marquee
//! safety invariant: self-authoring is **plan-only by default** and self-apply
//! requires `allow_self_modification = true`. This is the load-bearing gate for
//! SPEC-1 risk R-5.

use crate::config::config_schema::AutoAgentConfig;
use crate::error::{AutoAgentError, PolicyError, Result};

pub struct EvolveGuard {
    allow_self_mod: bool,
}

impl EvolveGuard {
    pub fn new(cfg: &AutoAgentConfig) -> Self {
        Self {
            allow_self_mod: cfg.agent.allow_self_modification,
        }
    }

    /// Planning self is always allowed (no writes).
    pub fn authorize_plan(&self) -> Result<()> {
        Ok(())
    }

    /// Applying to self requires `allow_self_modification = true`.
    pub fn authorize_apply(&self) -> Result<()> {
        if self.allow_self_mod {
            Ok(())
        } else {
            Err(AutoAgentError::Policy(PolicyError::WriteNotApproved(
                "self-modification disabled (allow_self_modification=false); evolve is plan-only"
                    .into(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::config_schema::AutoAgentConfig;
    use crate::config::default_config;

    fn cfg_with_self_mod(v: bool) -> AutoAgentConfig {
        let mut c = AutoAgentConfig::from_toml_str(&default_config::default_toml()).unwrap();
        c.agent.allow_self_modification = v;
        c
    }

    #[test]
    fn blocks_apply_when_self_mod_disabled() {
        let g = EvolveGuard::new(&cfg_with_self_mod(false));
        let res = g.authorize_apply();
        match res {
            Err(e) => assert_eq!(e.error_code(), "policy.write_not_approved"),
            Ok(_) => panic!("apply must be refused when self-mod is disabled"),
        }
    }

    #[test]
    fn plan_only_always_allowed() {
        assert!(EvolveGuard::new(&cfg_with_self_mod(false))
            .authorize_plan()
            .is_ok());
    }

    #[test]
    fn apply_allowed_with_flag() {
        assert!(EvolveGuard::new(&cfg_with_self_mod(true))
            .authorize_apply()
            .is_ok());
    }
}
