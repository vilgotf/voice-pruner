//! Contains all slash commands and notably the [`Command`] trait.

use anyhow::Result;
use async_trait::async_trait;
use twilight_model::application::{command::Command, interaction::ApplicationCommand};

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
	fn define() -> Command;

	/// Run the command, self should be an [`ApplicationCommand`].
	async fn run(self, ctx: Bot) -> Result<()>;
}

pub enum Commands {
	List(List),
	Prune(Prune),
}

impl Commands {
	pub fn r#match(command: ApplicationCommand) -> Option<Self> {
		match command.data.name.as_str() {
			List::NAME => Some(Self::List(List(command))),
			Prune::NAME => Some(Self::Prune(Prune(command))),
			_ => None,
		}
	}

	pub async fn run(self, ctx: Bot) -> Result<()> {
		match self {
			Commands::List(c) => c.run(ctx).await,
			Self::Prune(c) => c.run(ctx).await,
		}
	}
}

/// List of [`SlashCommand::define`]
pub fn commands() -> [Command; 2] {
	[List::define(), Prune::define()]
}
