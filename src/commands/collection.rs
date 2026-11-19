use clap::Subcommand;
use serde_json::json;

use crate::error::CliError;

use super::helpers::call_plan;
use super::RpcPlan;

#[derive(Subcommand)]
pub enum CollectionCmd {
    /// Inspect the active collection and its session ID
    Get,
    /// Run a coordinated sync (waits for completion unless --no-wait)
    Sync {
        #[arg(long, default_value_t = true)]
        include_media: bool,
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Refresh Anki windows (and optionally caches)
    Refresh {
        #[arg(long, default_value = "windows")]
        target: String,
    },
    /// Run database integrity check and repairs
    CheckIntegrity {
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Undo the last Anki undo entry
    Undo {
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
}

/// Translate a `collection` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &CollectionCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        CollectionCmd::Get => Ok(call_plan("collection.get", json!({}))),
        CollectionCmd::Sync {
            include_media,
            no_wait,
            timeout,
        } => Ok(RpcPlan::Call {
            method: "collection.sync".into(),
            params: json!({ "includeMedia": include_media }),
            wait: !no_wait,
            timeout_secs: *timeout,
        }),
        CollectionCmd::Refresh { target } => {
            if target != "windows" && target != "cachesAndWindows" {
                return Err(CliError::InvalidInput(format!(
                    "Invalid refresh target '{target}': expected 'windows' or 'cachesAndWindows'"
                )));
            }
            Ok(call_plan("collection.refresh", json!({ "target": target })))
        }
        CollectionCmd::CheckIntegrity { no_wait, timeout } => Ok(RpcPlan::Call {
            method: "collection.checkIntegrity".into(),
            params: json!({}),
            wait: !no_wait,
            timeout_secs: *timeout,
        }),
        CollectionCmd::Undo { no_wait, timeout } => Ok(RpcPlan::Call {
            method: "collection.undo".into(),
            params: json!({}),
            wait: !no_wait,
            timeout_secs: *timeout,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_valid_call(cmd: CollectionCmd, expected_method: &str, expected_wait: bool) {
        let plan = to_rpc_plan(&cmd).expect("build collection rpc plan");
        match plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                ..
            } => {
                assert_eq!(method, expected_method);
                assert_eq!(wait, expected_wait);
                schema::validate_method_params(&method, &params).unwrap_or_else(|e| {
                    panic!("schema validation failed for {method}: {e}");
                });
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn collection_commands_produce_valid_rpc_plans() {
        assert_valid_call(CollectionCmd::Get, "collection.get", false);
        assert_valid_call(
            CollectionCmd::Sync {
                include_media: true,
                no_wait: false,
                timeout: 300,
            },
            "collection.sync",
            true,
        );
        assert_valid_call(
            CollectionCmd::Sync {
                include_media: false,
                no_wait: true,
                timeout: 30,
            },
            "collection.sync",
            false,
        );
        assert_valid_call(
            CollectionCmd::Refresh {
                target: "windows".into(),
            },
            "collection.refresh",
            false,
        );
        assert_valid_call(
            CollectionCmd::Refresh {
                target: "cachesAndWindows".into(),
            },
            "collection.refresh",
            false,
        );
        assert_valid_call(
            CollectionCmd::CheckIntegrity {
                no_wait: false,
                timeout: 300,
            },
            "collection.checkIntegrity",
            true,
        );
        assert_valid_call(
            CollectionCmd::Undo {
                no_wait: true,
                timeout: 60,
            },
            "collection.undo",
            false,
        );
    }

    #[test]
    fn collection_refresh_rejects_invalid_target() {
        let err = to_rpc_plan(&CollectionCmd::Refresh {
            target: "invalid".into(),
        })
        .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}
