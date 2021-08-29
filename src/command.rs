//! Contains all slash commands and notably the [`Command`] trait.

use anyhow::Result;
use async_trait::async_trait;
use twilight_model::application::{
	command::Command as TwilightCommand, interaction::ApplicationCommand,
};

use crate::Bot;

mod list;
mod prune;

pub use {list::List, prune::Prune};

#[async_trait]
trait SlashCommand {
	/// Name of the command.
	/// Required to match incoming interactions.
	const NAME: &'static str;

	/// Command definition
	fn define() -> TwilightCommand;

	/// Run the command, self should be an [`ApplicationCommand`].
	async fn run(self, bot: Bot) -> Result<()>;
}

pub enum Command {
	List(List),
	Prune(Prune),
}

impl Command {
	pub fn get(cmd: ApplicationCommand) -> Option<Self> {
		match cmd.data.name.as_str() {
			List::NAME => Some(Self::List(List(cmd))),
			Prune::NAME => Some(Self::Prune(Prune(cmd))),
			_ => None,
		}
	}

	pub async fn run(self, bot: Bot) -> Result<()> {
		match self {
			Self::List(c) => c.run(bot).await,
			Self::Prune(c) => c.run(bot).await,
		}
	}
}

/// List of [`SlashCommand::define`]
pub fn commands() -> [TwilightCommand; 2] {
	[List::define(), Prune::define()]
}
