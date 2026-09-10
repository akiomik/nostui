use clap::Parser;

use crate::utils::version;

// nostui takes no options of its own. `--tick-rate` was the last one, and #527 removed
// the tick it configured. The type stays because `--help` and `--version` are worth
// having and an unknown argument is worth rejecting.
//
// Deliberately not a doc comment: clap would use it as the `about` text and print it to
// anyone running `--help`.
#[derive(Parser, Debug)]
#[command(author, version = version(), about)]
pub struct Cli {}
