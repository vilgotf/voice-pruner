use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(about, author, version)]
pub struct Args {
	/// Update commands and exit.
	#[arg(value_enum)]
	pub commands: Option<Mode>,
	/// Use verbose output.
	#[arg(action = clap::ArgAction::Count, conflicts_with = "quiet", long, short)]
	pub verbose: u8,
	/// Suppress output.
	#[arg(action = clap::ArgAction::SetTrue, conflicts_with = "verbose", long, short)]
	pub quiet: bool,
}

impl Args {
	pub fn parse() -> Self {
		// to avoid importing `Parser` in main
		<Self as Parser>::parse()
	}
}

#[derive(Clone, ValueEnum)]
pub enum Mode {
	Register,
	Unregister,
}

#[cfg(test)]
mod tests {
	#[test]
	fn verify_app() {
		use clap::CommandFactory;

		super::Args::command().debug_assert()
	}
}
