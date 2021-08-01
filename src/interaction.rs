use twilight_http::request::application::{InteractionError, UpdateOriginalResponse};
use twilight_model::application::{
	callback::{CallbackData, InteractionResponse},
	interaction::ApplicationCommand,
};

use crate::Bot;

/// Different types of [`InteractionResponse`]s
pub struct Response;

impl Response {
	const BASE: CallbackData = CallbackData {
		allowed_mentions: None,
		content: None,
		embeds: vec![],
		flags: None,
		tts: None,
	};

	pub const fn ack() -> InteractionResponse {
		InteractionResponse::DeferredChannelMessageWithSource(Self::BASE)
	}

	pub fn message(message: impl Into<String>) -> InteractionResponse {
		let message = message.into();
		InteractionResponse::ChannelMessageWithSource(Self::_message(message))
	}

	fn _message(message: String) -> CallbackData {
		if message.is_empty() {
			panic!("empty messages aren't allowed");
		}

		let mut data = Self::BASE;
		data.content = Some(message);
		data
	}
}

pub struct Interaction {
	pub bot: Bot,
	pub command: ApplicationCommand,
}
impl Interaction {
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

	/// Respond to the interaction, note
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
