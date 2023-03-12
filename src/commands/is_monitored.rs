use twilight_model::application::{
	command::{Command, CommandType},
	interaction::application_command::CommandOptionValue,
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{BOT, MONITORED_CHANNEL_TYPES};

pub fn define() -> Command {
	CommandBuilder::new(
		"is-monitored",
		"Checks if a voice channel is monitored",
		CommandType::ChatInput,
	)
	.dm_permission(false)
	.option(
		ChannelBuilder::new("channel", "Whether the voice channel is monitored")
			.channel_types(MONITORED_CHANNEL_TYPES)
			.required(true),
	)
	.build()
}

pub async fn run(ctx: super::Context) -> super::Result {
	let CommandOptionValue::Channel(channel) = ctx.data.options[0].value else {
		unreachable!("undefined");
	};

	ctx.reply(BOT.is_monitored(channel).to_string()).await
}
