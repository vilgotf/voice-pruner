//! Contains all commands.

use twilight_model::{
	application::{
		command::Command,
		interaction::{application_command::CommandData, ApplicationCommand},
	},
	id::{marker::ChannelMarker, Id},
};

mod is_monitored;
mod list;
mod prune;

type Result = anyhow::Result<()>;

/// Match and run a command
pub async fn run(bot: crate::Bot, command: ApplicationCommand) -> Result {
	let ctx = crate::interaction::Interaction::new(bot, command);

	match ctx.command.data.name.as_str() {
		is_monitored::NAME => is_monitored::run(ctx).await,
		list::NAME => list::run(ctx).await,
		prune::NAME => prune::run(ctx).await,
		_ => {
			tracing::warn!("unregistered");
			Ok(())
		}
	}
}

/// Array with all command definitions.
pub fn get() -> [Command; 3] {
	[is_monitored::define(), list::define(), prune::define()]
}

fn specified_channel(data: &CommandData) -> Option<Id<ChannelMarker>> {
	data.resolved
		.as_ref()
		.and_then(|resolved| resolved.channels.keys().next())
		.copied()
}
