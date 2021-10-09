use twilight_http::request::application::{interaction::UpdateOriginalResponse, InteractionError};
use twilight_model::application::{
	callback::{CallbackData, InteractionResponse},
	interaction::ApplicationCommand,
};
use twilight_util::builder::CallbackDataBuilder;

use crate::Bot;

/// Different types of [`InteractionResponse`]s.
pub struct Response;

impl Response {
	pub fn ack() -> InteractionResponse {
		InteractionResponse::DeferredChannelMessageWithSource(CallbackDataBuilder::new().build())
	}

	pub fn message(message: impl Into<String>) -> InteractionResponse {
		let message = message.into();
		InteractionResponse::ChannelMessageWithSource(Self::_message(message))
	}

	fn _message(message: String) -> CallbackData {
		if message.is_empty() {
			panic!("empty message is disallowed");
		}

		CallbackDataBuilder::new().content(message).build()
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
	/// Acknowledge the interaction, useful on commands that take a while to finish.
	///
	/// After calling this, use [`Interaction::update_response`] to add the finished response.
	///
	/// <https://discord.com/developers/docs/interactions/slash-commands#interaction-response-object>
	pub async fn ack(&self) -> Result<(), twilight_http::Error> {
		self.bot
			.http
			.interaction_callback(self.command.id, &self.command.token, &Response::ack())
			.exec()
			.await?;
		Ok(())
	}

	/// Respond to the interaction.
	pub async fn response(
		&self,
		response: &InteractionResponse,
	) -> Result<(), twilight_http::Error> {
		self.bot
			.http
			.interaction_callback(self.command.id, &self.command.token, response)
			.exec()
			.await?;
		Ok(())
	}

	/// Update a existing response, usually called after [`Interaction::ack`].
	pub fn update_response(&self) -> Result<UpdateOriginalResponse<'_>, InteractionError> {
		self.bot
			.http
			.update_interaction_original(self.command.token.as_str())
	}
}
