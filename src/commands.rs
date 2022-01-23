//! Contains all commands.

use tracing::{event, Level};
use twilight_model::{
	application::{
		command::Command,
		interaction::{application_command::CommandData, ApplicationCommand},
	},
	id::{marker::ChannelMarker, Id},
};

mod monitored;
mod prune;
mod unmonitored;

type Result = anyhow::Result<()>;

/// Match and run a command
pub async fn run(bot: crate::Bot, command: ApplicationCommand) -> Result {
	let ctx = crate::interaction::Interaction::new(bot, command);

	match ctx.command.data.name.as_str() {
		monitored::NAME => monitored::run(ctx).await,
		prune::NAME => prune::run(ctx).await,
		unmonitored::NAME => unmonitored::run(ctx).await,
		_ => {
			event!(Level::WARN, "unregistered");
			Ok(())
		}
	}
}

/// Array with all command definitions.
pub fn get() -> [Command; 3] {
	[monitored::define(), prune::define(), unmonitored::define()]
}

fn specified_channel(data: &CommandData) -> Option<Id<ChannelMarker>> {
	data.resolved
		.as_ref()
		.and_then(|resolved| resolved.channels.keys().next())
		.copied()
}
