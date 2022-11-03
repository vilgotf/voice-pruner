//! Contains all commands.
//!
//! Commands are defined in submodules. Required options are  `fn define() -> Command` (registering)
//! and `async fn run(ctx: &Interaction) -> Result` (execute incomming) interaction.
//!
//! This module also contain shared helper code.

mod is_monitored;
mod list;
mod prune;

use twilight_model::{
	application::{
		command::Command,
		interaction::{application_command::CommandData, Interaction, InteractionData},
	},
	channel::message::MessageFlags,
	http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
	id::{marker::ChannelMarker, Id},
};

use crate::BOT;

type Result = anyhow::Result<()>;

pub struct Context {
	data: Box<CommandData>,
	interaction: Interaction,
}

impl Context {
	/// Acknowledge the interaction and signal that a message will be provided later.
	async fn ack(&self) -> Result {
		BOT.http
			.interaction(BOT.application_id)
			.create_response(
				self.interaction.id,
				&self.interaction.token,
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
		BOT.http
			.interaction(BOT.application_id)
			.create_response(
				self.interaction.id,
				&self.interaction.token,
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
		BOT.http
			.interaction(BOT.application_id)
			.update_response(&self.interaction.token)
			.content(Some(message))
			.expect("valid length")
			.exec()
			.await?;
		Ok(())
	}
}

/// Match the interaction to a command and run it.
#[tracing::instrument(skip(interaction), fields(guild, name))]
pub async fn interaction(mut interaction: Interaction) {
	let Some(InteractionData::ApplicationCommand(data)) = interaction.data.take() else {
		return
	};

	if let Some(guild) = interaction.guild_id {
		tracing::Span::current().record("guild", &guild.get());
	}

	tracing::Span::current().record("name", &data.name);

	let ctx = Context { data, interaction };

	let res = match ctx.data.name.as_str() {
		"is-monitored" => is_monitored::run(ctx).await,
		"list" => list::run(ctx).await,
		"prune" => prune::run(ctx).await,
		_ => {
			tracing::warn!("unregistered");
			return;
		}
	};

	match res {
		Ok(_) => tracing::info!("successfully ran"),
		Err(e) => tracing::error!(error = &*e, "error running command"),
	}
}

/// Array with all command definitions.
pub fn get() -> [Command; 3] {
	[is_monitored::define(), list::define(), prune::define()]
}

fn resolved_channel(data: &CommandData) -> Option<Id<ChannelMarker>> {
	data.resolved
		.as_ref()
		.and_then(|resolved| resolved.channels.keys().next())
		.copied()
}
