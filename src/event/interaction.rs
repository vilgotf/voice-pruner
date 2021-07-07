use tracing::{event, Level};
use twilight_model::application::interaction::ApplicationCommand;

use crate::{
	commands::{Command, Commands},
	response, Bot,
};

pub struct Handler {
	bot: &'static Bot,
}

impl Handler {
	pub const fn new(bot: &'static Bot) -> Self {
		Self { bot }
	}

	pub async fn process(&self, command: ApplicationCommand) {
		if let Some(c) = Commands::r#match(&command.data.name) {
			let response = c
				.run(self.bot, &command)
				.unwrap_or_else(|| response("Error running command"));
			if let Err(e) = self
				.bot
				.http
				.interaction_callback(command.id, command.token, response)
				.await
			{
				// Workaround tracing::Value being weird tokio-rs/tracing#1308
				event!(Level::ERROR, error = &e as &dyn std::error::Error);
			}
		} else {
			event!(Level::WARN, %command.data.name, "received unregistered command");
		}
	}
}
