//! Command called by interaction event

use tracing::{event, instrument, Level};
use twilight_model::application::interaction::ApplicationCommand;

use crate::{commands::Commands, Bot};

/// Act upon the command
#[instrument(skip(bot, command), fields(guild_id, command.name = %command.data.name))]
pub async fn act(bot: Bot, command: ApplicationCommand) {
	if let Some(cmd) = Commands::r#match(command) {
		if let Err(e) = cmd.run(bot).await {
			event!(
				Level::ERROR,
				error = &*e as &dyn std::error::Error,
				"error running command"
			);
		}
	} else {
		event!(Level::WARN, "received unregistered command");
	}
}
