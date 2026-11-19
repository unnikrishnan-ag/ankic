use clap::Subcommand;
use serde_json::json;

use crate::error::CliError;

use super::helpers::call_plan;
use super::RpcPlan;

#[derive(Subcommand)]
pub enum OperationsCmd {
    /// Get the current status of an operation
    Get { operation_id: String },
    /// Poll an operation until it reaches a terminal state
    Wait {
        operation_id: String,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Request cancellation of an operation
    Cancel { operation_id: String },
}

/// Translate an `operations` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &OperationsCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        OperationsCmd::Get { operation_id } => Ok(call_plan(
            "operations.get",
            json!({ "operationId": operation_id }),
        )),
        OperationsCmd::Wait {
            operation_id,
            timeout,
        } => Ok(RpcPlan::WaitOperation {
            operation_id: operation_id.clone(),
            timeout_secs: *timeout,
        }),
        OperationsCmd::Cancel { operation_id } => Ok(call_plan(
            "operations.cancel",
            json!({ "operationId": operation_id }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    #[test]
    fn operations_commands_produce_valid_rpc_plans() {
        let get_plan = to_rpc_plan(&OperationsCmd::Get {
            operation_id: "op-1".into(),
        })
        .unwrap();
        match get_plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                ..
            } => {
                assert_eq!(method, "operations.get");
                assert!(!wait);
                assert_eq!(params, json!({ "operationId": "op-1" }));
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }

        let cancel_plan = to_rpc_plan(&OperationsCmd::Cancel {
            operation_id: "op-2".into(),
        })
        .unwrap();
        match cancel_plan {
            RpcPlan::Call {
                method,
                params,
                wait,
                ..
            } => {
                assert_eq!(method, "operations.cancel");
                assert!(!wait);
                assert_eq!(params, json!({ "operationId": "op-2" }));
                schema::validate_method_params(&method, &params).unwrap();
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }

        let wait_plan = to_rpc_plan(&OperationsCmd::Wait {
            operation_id: "op-99".into(),
            timeout: 90,
        })
        .unwrap();
        assert_eq!(
            wait_plan,
            RpcPlan::WaitOperation {
                operation_id: "op-99".into(),
                timeout_secs: 90,
            }
        );
    }
}
