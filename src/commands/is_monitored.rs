use twilight_model::application::command::{Command, CommandType};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{
	interaction::{Interaction, Response},
	Symbol,
};

pub const NAME: &str = "is-monitored";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME.to_owned(),
		"Checks if a voice channel is monitored".to_owned(),
		CommandType::ChatInput,
	)
	.option(
		ChannelBuilder::new(
			"channel".to_owned(),
			"Returns `true` if the voice channel is monitored".to_owned(),
		)
		.channel_types([crate::MONITORED_CHANNEL_TYPES])
		.required(true),
	)
	.build()
}

pub async fn run(ctx: Interaction) -> super::Result {
	if ctx.command.guild_id.is_some() {
		let content = errorable(&ctx).unwrap_or_else(|| "**Internal error**".to_owned());

		ctx.response(&Response::message(content)).await?;
	} else {
		ctx.response(&Response::message(format!(
			"{} **Unavailable in DMs**",
			Symbol::WARNING
		)))
		.await?;
	}
	Ok(())
}

fn errorable(ctx: &Interaction) -> Option<String> {
	super::specified_channel(&ctx.command.data).map(|channel_id| {
		if ctx.bot.is_monitored(channel_id) {
			"`true`"
		} else {
			"`false`"
		}
		.to_owned()
	})
}
