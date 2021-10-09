use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;
use const_format::formatcp;
use twilight_model::{
	application::{
		command::{Command, CommandType},
		interaction::{application_command::CommandDataOption, ApplicationCommand},
	},
	channel::ChannelType,
	guild::Permissions,
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
		if !ctx
			.command
			.member
			.as_ref()
			.expect("is interactions")
			.permissions
			.expect("is interaction")
			.contains(Permissions::MOVE_MEMBERS)
		{
			return Some(Cow::Borrowed(formatcp!(
				"{} **Requires the `MOVE_MEMBERS` permission**",
				Emoji::WARNING
			)));
		}

		if let Some(channel) = ctx
			.command
			.data
			.resolved
			.as_ref()
			.and_then(|resolved| resolved.channels.first())
		{
			Some(if ctx.bot.monitored(channel.id) {
				Cow::Borrowed("`true`")
			} else {
				Cow::Borrowed("`false`")
			})
		} else {
			let voice_channels = ctx
				.bot
				.cache
				.guild_channels(guild_id)?
				.into_iter()
				.filter_map(|channel_id| ctx.bot.cache.voice_channel(channel_id));

			let sub_command = match ctx.command.data.options.first()? {
				CommandDataOption::SubCommand { name, options: _ } => Some(name.as_str()),
				_ => None,
			}?;

			let format =
				|name: &str| -> String { format!("`{} {}`\n", Markdown::BULLET_POINT, name) };

			let channels: String = match sub_command {
				"monitored" => voice_channels
					.filter_map(|channel| {
						ctx.bot.monitored(channel.id).then(|| format(&channel.name))
					})
					.collect(),
				"unmonitored" => voice_channels
					.filter_map(|channel| {
						(!ctx.bot.monitored(channel.id)).then(|| format(&channel.name))
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
