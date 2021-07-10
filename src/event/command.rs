//! Command called by interaction event

use tracing::{event, instrument, Level};
use twilight_model::{
	application::{
		callback::InteractionResponse,
		interaction::{application_command::CommandData, ApplicationCommand},
	},
	guild::PartialMember,
	id::{ChannelId, GuildId, InteractionId},
	user::User,
};

use crate::{commands::Commands, response::Response, Bot};

struct Interaction {
	bot: &'static Bot,
	id: InteractionId,
	token: String,
}

fn log_err<T, E: std::error::Error + 'static>(res: Result<T, E>) {
	if let Err(e) = res {
		event!(Level::ERROR, error = &e as &dyn std::error::Error);
	}
}

impl Interaction {
	/// Acknowledge that we're handling the interaction.
	/// Useful on commands that take a while to finish.
	async fn ack(&self) {
		log_err(
			self.bot
				.http
				.interaction_callback(self.id, self.token.as_str(), Response::ack())
				.await,
		)
	}

	async fn responde(&self, response: InteractionResponse) {
		log_err(
			self.bot
				.http
				.interaction_callback(self.id, self.token.as_str(), response)
				.await,
		)
	}
}

pub struct PartialApplicationCommand {
	pub channel_id: ChannelId,
	pub data: CommandData,
	pub guild_id: Option<GuildId>,
	pub member: Option<PartialMember>,
	pub user: Option<User>,
}

/// Act upon the command
#[instrument(skip(bot, command), fields(command.name = %command.data.name, command.guild_id))]
pub async fn act(bot: &'static Bot, command: ApplicationCommand) {
	let interaction = Interaction {
		bot,
		id: command.id,
		token: command.token,
	};

	let partial_command = PartialApplicationCommand {
		channel_id: command.channel_id,
		data: command.data,
		guild_id: command.guild_id,
		member: command.member,
		user: command.user,
	};

	if let Some(cmd) = Commands::r#match(partial_command) {
		if cmd.is_long() {
			interaction.ack().await;
		}
		let response = cmd
			.run(bot)
			.await
			.unwrap_or_else(|_| Response::message("Error running command"));
		interaction.responde(response).await;
	} else {
		event!(Level::WARN, "received unregistered command");
	}
}
