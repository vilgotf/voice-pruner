use twilight_model::{
	application::command::{Command, CommandType},
	channel::ChannelType,
	id::{marker::GuildMarker, Id},
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{
	interaction::{Interaction, Response},
	InMemoryCacheExt, Permissions, Symbol,
};

pub const NAME: &str = "unmonitored";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME.to_owned(),
		"List unmonitored voice channels".to_owned(),
		CommandType::ChatInput,
	)
	.option(
		ChannelBuilder::new(
			"channel".to_owned(),
			"Returns `true` if the voice channel is unmonitored".to_owned(),
		)
		.channel_types([ChannelType::GuildVoice]),
	)
	.build()
}

pub async fn run(ctx: Interaction) -> super::Result {
	if let Some(guild_id) = ctx.command.guild_id {
		let content = errorable(&ctx, guild_id).unwrap_or_else(|| "**Internal error**".to_owned());

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

fn errorable(ctx: &Interaction, guild_id: Id<GuildMarker>) -> Option<String> {
	if !ctx.caller_permissions().contains(Permissions::ADMIN) {
		return Some(format!(
			"{} **Requires the `MOVE_MEMBERS` permission**",
			Symbol::WARNING
		));
	}

	if let Some(channel_id) = super::specified_channel(&ctx.command.data) {
		Some(
			if !ctx.bot.is_monitored(channel_id) {
				"`true`"
			} else {
				"`false`"
			}
			.to_owned(),
		)
	} else {
		let channels = ctx
			.bot
			.cache
			.guild_channels(guild_id)?
			.iter()
			.filter_map(|&channel_id| ctx.bot.cache.voice_channel(channel_id))
			.filter_map(|channel| {
				(!ctx.bot.is_monitored(channel.id))
					.then(|| format!("`{} {}`\n", channel.name, Symbol::BULLET_POINT))
			})
			.collect::<String>();

		Some(if channels.is_empty() {
			"`None`".to_owned()
		} else {
			channels
		})
	}
}
