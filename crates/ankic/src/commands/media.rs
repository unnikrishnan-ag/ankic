use clap::Subcommand;
use serde_json::{json, Map, Value};

use crate::error::CliError;

use super::helpers::call_plan;
use super::RpcPlan;

#[derive(Subcommand)]
pub enum MediaCmd {
    /// List media filenames in the collection
    List {
        /// Optional glob pattern (defaults to "*" on the server)
        #[arg(long)]
        pattern: Option<String>,
    },
    /// Retrieve a media file by filename (base64-encoded content)
    Get {
        /// Media filename
        filename: String,
    },
    /// Store a media file from base64 data, local path, or HTTP(S) URL
    Store {
        /// Target media filename
        #[arg(long)]
        filename: String,
        /// Base64-encoded file contents
        #[arg(long, visible_alias = "data-base64", conflicts_with_all = ["path", "source_url"])]
        data: Option<String>,
        /// Local filesystem path to read media bytes from
        #[arg(long, conflicts_with_all = ["data", "source_url"])]
        path: Option<String>,
        /// HTTP or HTTPS URL to fetch media bytes from
        #[arg(id = "source_url", long = "source-url", conflicts_with_all = ["data", "path"])]
        url: Option<String>,
        /// Collision policy when filename already exists (rename, replace, error)
        #[arg(long)]
        collision: Option<String>,
        /// Hash algorithm for skipIfHash (md5 or sha256)
        #[arg(long, requires = "skip_hash_digest")]
        skip_hash_alg: Option<String>,
        /// Expected hash digest for skipIfHash
        #[arg(long, requires = "skip_hash_alg")]
        skip_hash_digest: Option<String>,
        /// Return the OperationHandle immediately without polling operations.get
        #[arg(long)]
        no_wait: bool,
        /// Operation polling timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Delete a media file by filename
    Delete {
        /// Media filename
        filename: String,
    },
    /// Get the path to the collection media directory
    Directory,
}

/// Translate a `media` subcommand into an `RpcPlan` without performing network I/O.
pub fn to_rpc_plan(cmd: &MediaCmd) -> Result<RpcPlan, CliError> {
    match cmd {
        MediaCmd::List { pattern } => {
            let mut params = Map::new();
            if let Some(p) = pattern {
                params.insert("pattern".into(), json!(p));
            }
            Ok(call_plan("media.list", Value::Object(params)))
        }
        MediaCmd::Get { filename } => {
            Ok(call_plan("media.get", json!({ "filename": filename })))
        }
        MediaCmd::Store {
            filename,
            data,
            path,
            url,
            collision,
            skip_hash_alg,
            skip_hash_digest,
            no_wait,
            timeout,
        } => {
            let source = match (data.as_deref(), path.as_deref(), url.as_deref()) {
                (Some(d), None, None) => json!({ "data": d }),
                (None, Some(p), None) => json!({ "path": p }),
                (None, None, Some(u)) => json!({ "url": u }),
                _ => {
                    return Err(CliError::InvalidInput(
                        "Specify exactly one of --data, --path, or --source-url".into(),
                    ));
                }
            };

            let mut params = Map::new();
            params.insert("filename".into(), json!(filename));
            params.insert("source".into(), source);

            if let Some(c) = collision {
                if !matches!(c.as_str(), "rename" | "replace" | "error") {
                    return Err(CliError::InvalidInput(format!(
                        "Invalid collision policy '{c}': expected 'rename', 'replace', or 'error'"
                    )));
                }
                params.insert("collision".into(), json!(c));
            }

            match (skip_hash_alg.as_deref(), skip_hash_digest.as_deref()) {
                (Some(alg), Some(digest)) => {
                    if !matches!(alg, "md5" | "sha256") {
                        return Err(CliError::InvalidInput(format!(
                            "Invalid skip hash algorithm '{alg}': expected 'md5' or 'sha256'"
                        )));
                    }
                    if digest.is_empty() {
                        return Err(CliError::InvalidInput(
                            "--skip-hash-digest must not be empty".into(),
                        ));
                    }
                    params.insert(
                        "skipIfHash".into(),
                        json!({
                            "algorithm": alg,
                            "digest": digest,
                        }),
                    );
                }
                (None, None) => {}
                _ => {
                    return Err(CliError::InvalidInput(
                        "--skip-hash-alg and --skip-hash-digest must be provided together".into(),
                    ));
                }
            }

            Ok(RpcPlan::Call {
                method: "media.store".into(),
                params: Value::Object(params),
                wait: !no_wait,
                timeout_secs: *timeout,
            })
        }
        MediaCmd::Delete { filename } => {
            Ok(call_plan("media.delete", json!({ "filename": filename })))
        }
        MediaCmd::Directory => Ok(call_plan("media.getDirectory", json!({}))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn assert_valid_call(
        cmd: MediaCmd,
        expected_method: &str,
        expected_wait: bool,
    ) -> Value {
        let plan = to_rpc_plan(&cmd).expect("build media rpc plan");
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
                params
            }
            other => panic!("expected RpcPlan::Call, got {other:?}"),
        }
    }

    #[test]
    fn media_list_get_delete_directory_plans() {
        let p = assert_valid_call(MediaCmd::List { pattern: None }, "media.list", false);
        assert_eq!(p, json!({}));

        let p = assert_valid_call(
            MediaCmd::List {
                pattern: Some("*.mp3".into()),
            },
            "media.list",
            false,
        );
        assert_eq!(p, json!({ "pattern": "*.mp3" }));

        let p = assert_valid_call(
            MediaCmd::Get {
                filename: "audio.mp3".into(),
            },
            "media.get",
            false,
        );
        assert_eq!(p, json!({ "filename": "audio.mp3" }));

        let p = assert_valid_call(
            MediaCmd::Delete {
                filename: "audio.mp3".into(),
            },
            "media.delete",
            false,
        );
        assert_eq!(p, json!({ "filename": "audio.mp3" }));

        let p = assert_valid_call(MediaCmd::Directory, "media.getDirectory", false);
        assert_eq!(p, json!({}));
    }

    #[test]
    fn media_store_variants_produce_valid_plans() {
        let p = assert_valid_call(
            MediaCmd::Store {
                filename: "hello.txt".into(),
                data: Some("SGVsbG8=".into()),
                path: None,
                url: None,
                collision: Some("replace".into()),
                skip_hash_alg: Some("sha256".into()),
                skip_hash_digest: Some(
                    "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969".into(),
                ),
                no_wait: false,
                timeout: 300,
            },
            "media.store",
            true,
        );
        assert_eq!(
            p,
            json!({
                "filename": "hello.txt",
                "source": { "data": "SGVsbG8=" },
                "collision": "replace",
                "skipIfHash": {
                    "algorithm": "sha256",
                    "digest": "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969",
                },
            })
        );

        let p = assert_valid_call(
            MediaCmd::Store {
                filename: "pic.png".into(),
                data: None,
                path: Some("/tmp/pic.png".into()),
                url: None,
                collision: Some("rename".into()),
                skip_hash_alg: Some("md5".into()),
                skip_hash_digest: Some("d41d8cd98f00b204e9800998ecf8427e".into()),
                no_wait: true,
                timeout: 60,
            },
            "media.store",
            false,
        );
        assert_eq!(p["source"], json!({ "path": "/tmp/pic.png" }));
        assert_eq!(p["collision"], json!("rename"));

        let p = assert_valid_call(
            MediaCmd::Store {
                filename: "remote.jpg".into(),
                data: None,
                path: None,
                url: Some("https://example.com/remote.jpg".into()),
                collision: Some("error".into()),
                skip_hash_alg: None,
                skip_hash_digest: None,
                no_wait: false,
                timeout: 300,
            },
            "media.store",
            true,
        );
        assert_eq!(p["source"], json!({ "url": "https://example.com/remote.jpg" }));
        assert_eq!(p["collision"], json!("error"));
        assert!(p.get("skipIfHash").is_none());
    }

    #[test]
    fn media_store_rejects_invalid_sources_and_options() {
        // Missing source
        let err = to_rpc_plan(&MediaCmd::Store {
            filename: "a.png".into(),
            data: None,
            path: None,
            url: None,
            collision: None,
            skip_hash_alg: None,
            skip_hash_digest: None,
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));

        // Conflicting sources
        let err = to_rpc_plan(&MediaCmd::Store {
            filename: "a.png".into(),
            data: Some("AA==".into()),
            path: Some("/tmp/a.png".into()),
            url: None,
            collision: None,
            skip_hash_alg: None,
            skip_hash_digest: None,
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));

        // Invalid collision
        let err = to_rpc_plan(&MediaCmd::Store {
            filename: "a.png".into(),
            data: Some("AA==".into()),
            path: None,
            url: None,
            collision: Some("overwrite".into()),
            skip_hash_alg: None,
            skip_hash_digest: None,
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));

        // Partial skipIfHash
        let err = to_rpc_plan(&MediaCmd::Store {
            filename: "a.png".into(),
            data: Some("AA==".into()),
            path: None,
            url: None,
            collision: None,
            skip_hash_alg: Some("sha256".into()),
            skip_hash_digest: None,
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));

        // Invalid skipIfHash algorithm
        let err = to_rpc_plan(&MediaCmd::Store {
            filename: "a.png".into(),
            data: Some("AA==".into()),
            path: None,
            url: None,
            collision: None,
            skip_hash_alg: Some("blake3".into()),
            skip_hash_digest: Some("abc".into()),
            no_wait: false,
            timeout: 300,
        })
        .unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }
}
