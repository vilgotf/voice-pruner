use twilight_model::application::command::{Command, CommandType};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{Permissions, MONITORED_CHANNEL_TYPES};

pub const NAME: &str = "is-monitored";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME,
		"Checks if a voice channel is monitored",
		CommandType::ChatInput,
	)
	.default_member_permissions(Permissions::ADMIN)
	.dm_permission(false)
	.option(
		ChannelBuilder::new(
			"channel",
			"Returns `true` if the voice channel is monitored",
		)
		.channel_types(MONITORED_CHANNEL_TYPES)
		.required(true),
	)
	.build()
}

pub async fn run(ctx: super::Context) -> super::Result {
	let channel = super::resolved_channel(&ctx.data).expect("required option");

	ctx.reply(ctx.bot.is_monitored(channel).to_string()).await
}
