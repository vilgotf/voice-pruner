//! Contains all commands.
//!
//! Commands are defined in submodules. Required options are `const NAME: &str` (matching incoming),
//! `fn define() -> Command` (registering) and `async fn run(ctx: &Interaction) -> Result` (execute
//! incomming) interaction.
//!
//! This module also contain shared helper code.

use twilight_model::{
	application::{
		command::Command,
		interaction::{application_command::CommandData, ApplicationCommand},
	},
	channel::message::MessageFlags,
	http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
	id::{marker::ChannelMarker, Id},
};

use crate::Bot;

mod is_monitored;
mod list;
mod prune;

type Result = anyhow::Result<()>;

pub struct Context {
	bot: Bot,
	command: ApplicationCommand,
}

impl Context {
	/// Acknowledge the interaction and signal that a message will be provided later.
	async fn ack(&self) -> Result {
		self.bot
			.to_interaction()
			.create_response(
				self.command.id,
				&self.command.token,
				&InteractionResponse {
					kind: InteractionResponseType::DeferredChannelMessageWithSource,
					data: Some(InteractionResponseData {
						flags: Some(MessageFlags::EPHEMERAL),
						..InteractionResponseData::default()
					}),
				},
			)
			.exec()
			.await?;
		Ok(())
	}

	/// Respond to the interaction with a message.
	async fn reply(&self, message: String) -> Result {
		self.bot
			.to_interaction()
			.create_response(
				self.command.id,
				&self.command.token,
				&InteractionResponse {
					kind: InteractionResponseType::ChannelMessageWithSource,
					data: Some(InteractionResponseData {
						content: Some(message),
						flags: Some(MessageFlags::EPHEMERAL),
						..InteractionResponseData::default()
					}),
				},
			)
			.exec()
			.await?;
		Ok(())
	}

	/// Update an existing response with a message.
	async fn update_response(&self, message: &str) -> Result {
		self.bot
			.to_interaction()
			.update_response(&self.command.token)
			.content(Some(message))
			.expect("valid length")
			.exec()
			.await?;
		Ok(())
	}
}

/// Match the interaction to a command and run it.
#[tracing::instrument(skip(bot, command), fields(guild_id, command.name = %command.data.name))]
pub async fn run(bot: Bot, command: ApplicationCommand) {
	if let Some(guild_id) = command.guild_id {
		tracing::Span::current().record("guild_id", &guild_id.get());
	}

	let ctx = Context { bot, command };

	let res = match ctx.command.data.name.as_str() {
		is_monitored::NAME => is_monitored::run(ctx).await,
		list::NAME => list::run(ctx).await,
		prune::NAME => prune::run(ctx).await,
		_ => {
			tracing::warn!("unregistered");
			Ok(())
		}
	};

	match res {
		Ok(_) => tracing::info!("successfully ran"),
		Err(e) => tracing::error!(
			error = &*e as &dyn std::error::Error,
			"error running command"
		),
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
