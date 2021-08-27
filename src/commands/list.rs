use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;
use const_format::formatcp;
use twilight_model::{
	application::{
		command::{BaseCommandOptionData, Command, CommandOption, OptionsCommandOptionData},
		interaction::{application_command::CommandDataOption, ApplicationCommand},
	},
	channel::ChannelType,
	guild::Permissions,
	id::GuildId,
};

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
			.expect("included in guild interactions")
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
			let channel_id = match channel.kind {
				ChannelType::GuildVoice => channel.id,
				_ => {
					return Some(Cow::Borrowed(formatcp!(
						"{} **Not a voice channel**",
						Emoji::WARNING
					)))
				}
			};

			Some(if ctx.bot.monitored(channel_id) {
				Cow::Borrowed("`true`")
			} else {
				Cow::Borrowed("`false`")
			})
		} else {
			let format =
				|name: &str| -> String { format!("`{} {}`\n", Markdown::BULLET_POINT, name) };

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
		Command {
			application_id: None,
			default_permission: None,
			description: String::from("List monitored or unmonitored voice channels"),
			guild_id: None,
			id: None,
			name: String::from(Self::NAME),
			options: vec![
				CommandOption::SubCommand(OptionsCommandOptionData {
					description: String::from("List monitored voice channels"),
					name: String::from("monitored"),
					options: vec![CommandOption::Channel(BaseCommandOptionData {
						description: String::from(
							"Returns `true` if the voice channel is monitored",
						),
						name: String::from("channel"),
						required: false,
					})],
					required: false,
				}),
				CommandOption::SubCommand(OptionsCommandOptionData {
					description: String::from("List unmonitored voice channels"),
					name: String::from("unmonitored"),
					options: vec![CommandOption::Channel(BaseCommandOptionData {
						description: String::from(
							"Returns `true` if the voice channel is unmonitored",
						),
						name: String::from("channel"),
						required: false,
					})],
					required: false,
				}),
			],
		}
	}

	async fn run(self, ctx: Bot) -> Result<()> {
		let interaction = ctx.interaction(self.0);
		if let Some(guild_id) = interaction.command.guild_id {
			tracing::Span::current().record("guild_id", &guild_id.0);
			let content = Self::errorable(&interaction, guild_id)
				.unwrap_or(Cow::Borrowed("**Internal error**"));

			interaction.response(&Response::message(content)).await?;
		} else {
			interaction
				.response(&Response::message(formatcp!(
					"{} **Unavailable in DMs**",
					Emoji::WARNING
				)))
				.await?;
		}
		Ok(())
	}
}
