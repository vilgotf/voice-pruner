use twilight_model::{
	application::interaction::ApplicationCommand,
	guild::Permissions,
	http::interaction::{InteractionResponse, InteractionResponseType},
};
use twilight_util::builder::InteractionResponseDataBuilder;

use crate::Bot;

/// Different types of [`InteractionResponse`]s.
pub struct Response;

impl Response {
	pub fn ack() -> InteractionResponse {
		InteractionResponse {
			kind: InteractionResponseType::DeferredChannelMessageWithSource,
			data: Some(InteractionResponseDataBuilder::new().build()),
		}
	}

	pub fn message(message: String) -> InteractionResponse {
		assert!(!message.is_empty(), "empty message is disallowed");

		InteractionResponse {
			kind: InteractionResponseType::ChannelMessageWithSource,
			data: Some(
				InteractionResponseDataBuilder::new()
					.content(message)
					.build(),
			),
		}
	}
}

pub struct Interaction {
	pub bot: Bot,
	pub command: ApplicationCommand,
}

impl Interaction {
	pub const fn new(bot: Bot, command: ApplicationCommand) -> Self {
		Self { bot, command }
	}

	/// Respond to the interaction.
	pub async fn response(
		&self,
		response: &InteractionResponse,
	) -> Result<(), twilight_http::Error> {
		self.bot
			.as_interaction()
			.create_response(self.command.id, &self.command.token, response)
			.exec()
			.await?;
		Ok(())
	}

	pub fn caller_permissions(&self) -> Permissions {
		self.command
			.member
			.as_ref()
			.expect("is interaction")
			.permissions
			.expect("is interaction")
	}
}
