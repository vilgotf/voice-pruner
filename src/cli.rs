use clap::{ArgEnum, Parser};

#[derive(Parser)]
#[clap(about, author, version)]
pub struct Args {
	/// Modify registered commands and exit
	#[clap(arg_enum)]
	pub modify_commands: Option<Mode>,
}

impl Args {
	pub fn parse() -> Self {
		// to avoid importing `Parser` in main
		<Args as Parser>::parse()
	}
}

#[derive(Clone, ArgEnum)]
pub enum Mode {
	Remove,
	Set,
}

#[cfg(test)]
mod tests {
	#[test]
	fn verify_app() {
		use clap::CommandFactory;

		super::Args::command().debug_assert()
	}
}
