use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;
use const_format::formatcp;
use twilight_model::{
	application::{
		command::{Command, CommandType},
		interaction::ApplicationCommand,
	},
	channel::ChannelType,
	id::GuildId,
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder, SubCommandBuilder};

use crate::{
	interaction::{Interaction, Response},
	response::{Emoji, Markdown},
	Bot, InMemoryCacheExt,
};

use super::SlashCommand;

pub struct List(pub(super) ApplicationCommand);

impl List {
	fn errorable(ctx: &Interaction, guild_id: GuildId) -> Option<Cow<'static, str>> {
		if !ctx.caller_is_admin() {
			return Some(Cow::Borrowed(formatcp!(
				"{} **Requires the `MOVE_MEMBERS` permission**",
				Emoji::WARNING
			)));
		}

		if let Some(&channel_id) = ctx
			.command
			.data
			.resolved
			.as_ref()
			.and_then(|resolved| resolved.channels.keys().next())
		{
			Some(if ctx.bot.is_monitored(channel_id) {
				Cow::Borrowed("`true`")
			} else {
				Cow::Borrowed("`false`")
			})
		} else {
			let channels = ctx.bot.cache.guild_channels(guild_id)?;
			let voice_channels = channels
				.iter()
				.filter_map(|&channel_id| ctx.bot.cache.voice_channel(channel_id));

			let format =
				|name: &str| -> String { format!("`{} {}`\n", Markdown::BULLET_POINT, name) };

			let channels: String = match ctx.command.data.options.first()?.name.as_str() {
				"monitored" => voice_channels
					.filter_map(|channel| {
						ctx.bot
							.is_monitored(channel.id)
							.then(|| format(&channel.name))
					})
					.collect(),
				"unmonitored" => voice_channels
					.filter_map(|channel| {
						(!ctx.bot.is_monitored(channel.id)).then(|| format(&channel.name))
					})
					.collect(),
				_ => unreachable!("undefined sub command name"),
			};

			Some(if channels.is_empty() {
				Cow::Borrowed("`None`")
			} else {
				Cow::Owned(channels)
			})
		}
	}
}

#[async_trait]
impl SlashCommand for List {
	const NAME: &'static str = "list";

	fn define() -> Command {
		CommandBuilder::new(
			Self::NAME.to_owned(),
			"List of monitored or unmonitored voice channels".to_owned(),
			CommandType::ChatInput,
		)
		.option(
			SubCommandBuilder::new(
				"monitored".to_owned(),
				"List monitored voice channels".to_owned(),
			)
			.option(
				ChannelBuilder::new(
					"channel".to_owned(),
					"Returns `true` if the voice channel is monitored".to_owned(),
				)
				.channel_types([ChannelType::GuildVoice]),
			),
		)
		.option(
			SubCommandBuilder::new(
				"unmonitored".to_owned(),
				"List unmonitored voice channels".to_owned(),
			)
			.option(
				ChannelBuilder::new(
					"channel".to_owned(),
					"Returns `true` if the voice channel is unmonitored".to_owned(),
				)
				.channel_types([ChannelType::GuildVoice]),
			),
		)
		.build()
	}

	async fn run(self, bot: Bot) -> Result<()> {
		let ctx = Interaction::new(bot, self.0);
		if let Some(guild_id) = ctx.command.guild_id {
			tracing::Span::current().record("guild_id", &guild_id.0);
			let content =
				Self::errorable(&ctx, guild_id).unwrap_or(Cow::Borrowed("**Internal error**"));

			ctx.response(&Response::message(content)).await?;
		} else {
			ctx.response(&Response::message(formatcp!(
				"{} **Unavailable in DMs**",
				Emoji::WARNING
			)))
			.await?;
		}
		Ok(())
	}
}
