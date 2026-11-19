use std::process::ExitCode;

use clap::Parser;

use anki_skill::client::AnkiClient;
use anki_skill::commands::dispatch;
use anki_skill::output;
use anki_skill::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = cli.output_format();

    let result = (|| {
        let client = AnkiClient::new(cli.url, cli.api_key, cli.session)?
            .with_validation(!cli.no_validate);
        dispatch(&client, cli.command)
    })();

    match result {
        Ok(val) => match output::format_value(&val, format) {
            Ok(s) => {
                println!("{s}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                let code = err.exit_code() as u8;
                eprintln!("{}", output::format_error(&err, format));
                ExitCode::from(code)
            }
        },
        Err(err) => {
            let code = err.exit_code() as u8;
            eprintln!("{}", output::format_error(&err, format));
            ExitCode::from(code)
        }
    }
}
