use clap::{ArgEnum, Parser};

#[derive(Parser)]
#[clap(about, author, version)]
pub struct Args {
	/// Update commands and exit
	#[clap(arg_enum)]
	pub commands: Option<Mode>,
}

impl Args {
	pub fn parse() -> Self {
		// to avoid importing `Parser` in main
		<Self as Parser>::parse()
	}
}

#[derive(Clone, ArgEnum)]
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
