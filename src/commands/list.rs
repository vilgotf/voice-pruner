use tracing::{event, Level};
use twilight_model::{
	application::{
		command::{Command, CommandType},
		interaction::application_command::CommandOptionValue,
	},
	id::{marker::GuildMarker, Id},
};
use twilight_util::builder::command::{CommandBuilder, StringBuilder};

use crate::{
	interaction::{Interaction, Response},
	InMemoryCacheExt, Symbol,
};

pub const NAME: &str = "list";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME.to_owned(),
		"List visible voice channels".to_owned(),
		CommandType::ChatInput,
	)
	.option(
		StringBuilder::new(
			"type".to_owned(),
			"Only monitored / unmonitored voice channels".to_owned(),
		)
		.choices([
			("Monitored".to_owned(), "monitored".to_owned()),
			("Unmonitored".to_owned(), "unmonitored".to_owned()),
		]),
	)
	.build()
}

pub async fn run(ctx: Interaction) -> super::Result {
	if let Some(guild_id) = ctx.command.guild_id {
		let content = channels(&ctx, guild_id);

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

fn channels(ctx: &Interaction, guild_id: Id<GuildMarker>) -> String {
	let guild_channels = ctx
		.bot
		.cache
		.guild_channels(guild_id)
		.expect("channel is cached");

	let voice_channels = guild_channels
		.iter()
		.filter_map(|&channel_id| ctx.bot.cache.voice_channel(channel_id));

	let format = |name: &str| format!("`{} {}`\n", Symbol::BULLET_POINT, name);

	let channels: String = if let Some(r#type) =
		ctx.command
			.data
			.options
			.first()
			.and_then(|o| match &o.value {
				CommandOptionValue::String(s) => Some(s),
				_ => None,
			}) {
		if r#type == "monitored" {
			voice_channels
				.filter_map(|channel| {
					(ctx.bot.is_monitored(channel.id)).then(|| format(&channel.name))
				})
				.collect()
		} else if r#type == "unmonitored" {
			voice_channels
				.filter_map(|channel| {
					(!ctx.bot.is_monitored(channel.id)).then(|| format(&channel.name))
				})
				.collect()
		} else {
			event!(Level::ERROR, %r#type);
			"**Internal error**".to_owned()
		}
	} else {
		voice_channels
			.map(|channel| format(&channel.name))
			.collect()
	};

	if channels.is_empty() {
		"`None`".to_owned()
	} else {
		channels
	}
}
