use clap::Subcommand;
use serde_json::json;

use crate::error::CliError;

use super::helpers::make_ref;
use super::RpcPlan;

#[derive(Subcommand)]
pub enum PackagesCmd {
    /// Import an Anki package (.apkg or .colpkg) from a local filesystem path
    Import {
        /// Local filesystem path to the package file
        path: String,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Export a deck to an Anki package (.apkg) at a local filesystem path
    Export {
        /// Deck name
        #[arg(long, visible_aliases = ["deck-name", "deck"], conflicts_with = "id")]
        name: Option<String>,
        /// Deck ID
        #[arg(long, visible_alias = "deck-id", conflicts_with = "name")]
        id: Option<String>,
        /// Destination filesystem path for the exported .apkg file
        #[arg(long)]
        path: String,
        /// Include card scheduling information in the exported package
        #[arg(long)]
        include_scheduling: bool,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
}

/// Translate a `packages` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &PackagesCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        PackagesCmd::Import {
            path,
            no_wait,
            timeout,
        } => {
            if path.trim().is_empty() {
                return Err(CliError::InvalidInput(
                    "Package path must not be empty".into(),
                ));
            }
            Ok(RpcPlan::Call {
                method: "packages.import".into(),
                params: json!({ "path": path }),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        PackagesCmd::Export {
            name,
            id,
            path,
            include_scheduling,
            no_wait,
            timeout,
        } => {
            if path.trim().is_empty() {
                return Err(CliError::InvalidInput(
                    "Package path must not be empty".into(),
                ));
            }
            let deck = make_ref(name.as_deref(), id.as_deref(), "deck")?;
            Ok(RpcPlan::Call {
                method: "packages.export".into(),
                params: json!({
                    "deck": deck,
                    "path": path,
                    "includeScheduling": include_scheduling,
                }),
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
        cmd: PackagesCmd,
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
    fn import_defaults_to_waiting() {
        let params = assert_valid_call(
            PackagesCmd::Import {
                path: "/tmp/deck.apkg".into(),
                no_wait: false,
                timeout: 300,
            },
            "packages.import",
            true,
            300,
        );
        assert_eq!(params, json!({ "path": "/tmp/deck.apkg" }));
    }

    #[test]
    fn import_no_wait_disables_polling() {
        let params = assert_valid_call(
            PackagesCmd::Import {
                path: "/tmp/backup.colpkg".into(),
                no_wait: true,
                timeout: 120,
            },
            "packages.import",
            false,
            120,
        );
        assert_eq!(params, json!({ "path": "/tmp/backup.colpkg" }));
    }

    #[test]
    fn export_by_deck_name_and_scheduling() {
        let params = assert_valid_call(
            PackagesCmd::Export {
                name: Some("Japanese::Vocab".into()),
                id: None,
                path: "/tmp/vocab.apkg".into(),
                include_scheduling: true,
                no_wait: false,
                timeout: 300,
            },
            "packages.export",
            true,
            300,
        );
        assert_eq!(
            params,
            json!({
                "deck": { "name": "Japanese::Vocab" },
                "path": "/tmp/vocab.apkg",
                "includeScheduling": true,
            })
        );
    }

    #[test]
    fn export_by_deck_id_no_wait() {
        let params = assert_valid_call(
            PackagesCmd::Export {
                name: None,
                id: Some("1234567890".into()),
                path: "/tmp/out.apkg".into(),
                include_scheduling: false,
                no_wait: true,
                timeout: 60,
            },
            "packages.export",
            false,
            60,
        );
        assert_eq!(
            params,
            json!({
                "deck": { "id": "1234567890" },
                "path": "/tmp/out.apkg",
                "includeScheduling": false,
            })
        );
    }

    #[test]
    fn export_rejects_missing_deck_reference() {
        let err = to_rpc_plan(&PackagesCmd::Export {
            name: None,
            id: None,
            path: "/tmp/out.apkg".into(),
            include_scheduling: false,
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn rejects_empty_path() {
        let err_import = to_rpc_plan(&PackagesCmd::Import {
            path: "   ".into(),
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err_import, CliError::InvalidInput(_)));

        let err_export = to_rpc_plan(&PackagesCmd::Export {
            name: Some("Default".into()),
            id: None,
            path: "".into(),
            include_scheduling: false,
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err_export, CliError::InvalidInput(_)));
    }
}
