use clap::Subcommand;
use serde_json::json;

use crate::error::CliError;

use super::helpers::call_plan;
use super::RpcPlan;

#[derive(Subcommand)]
pub enum ProfilesCmd {
    /// List available Anki profiles
    List,
    /// Get the currently active Anki profile
    Active,
    /// Switch to another Anki profile
    Switch {
        /// Target profile name
        name: String,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
}

/// Translate a `profiles` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &ProfilesCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        ProfilesCmd::List => Ok(call_plan("profiles.list", json!({}))),
        ProfilesCmd::Active => Ok(call_plan("profiles.getActive", json!({}))),
        ProfilesCmd::Switch {
            name,
            no_wait,
            timeout,
        } => {
            if name.trim().is_empty() {
                return Err(CliError::InvalidInput(
                    "Profile name must not be empty".into(),
                ));
            }
            Ok(RpcPlan::Call {
                method: "profiles.switch".into(),
                params: json!({ "name": name }),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;
    use serde_json::Value;

    fn assert_valid_call(
        cmd: ProfilesCmd,
        expected_method: &str,
        expected_wait: bool,
        expected_timeout: u64,
    ) -> Value {
        let plan = to_rpc_plan(&cmd).expect("to_rpc_plan should succeed");
        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                timeout_secs,
            } => {
                assert_eq!(method, expected_method);
                assert_eq!(wait, expected_wait);
                assert_eq!(timeout_secs, expected_timeout);
                schema::validate_method_params(&method, &params)
                    .expect("params should satisfy api-v1 schema");
                params
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn list_and_active_build_empty_params() {
        let list = assert_valid_call(ProfilesCmd::List, "profiles.list", false, 0);
        assert_eq!(list, json!({}));

        let active = assert_valid_call(ProfilesCmd::Active, "profiles.getActive", false, 0);
        assert_eq!(active, json!({}));
    }

    #[test]
    fn switch_defaults_to_waiting_and_supports_no_wait() {
        let waiting = assert_valid_call(
            ProfilesCmd::Switch {
                name: "User 1".into(),
                no_wait: false,
                timeout: 300,
            },
            "profiles.switch",
            true,
            300,
        );
        assert_eq!(waiting, json!({ "name": "User 1" }));

        let no_wait = assert_valid_call(
            ProfilesCmd::Switch {
                name: "Study".into(),
                no_wait: true,
                timeout: 60,
            },
            "profiles.switch",
            false,
            60,
        );
        assert_eq!(no_wait, json!({ "name": "Study" }));
    }

    #[test]
    fn switch_rejects_empty_name() {
        let err = to_rpc_plan(&ProfilesCmd::Switch {
            name: "   ".into(),
            no_wait: false,
            timeout: 300,
        })
        .expect_err("empty profile name should fail");
        assert!(matches!(err, CliError::InvalidInput(_)));
    }
}
