use async_trait::async_trait;
#[allow(unused_imports)]
use tracing::{event, Level};
use twilight_model::{
	application::{
		callback::InteractionResponse,
		command::{
			BaseCommandOptionData, Command as SlashCommand, CommandOption, OptionsCommandOptionData,
		},
		interaction::application_command::CommandDataOption,
	},
	channel::{ChannelType, GuildChannel},
	guild::Permissions,
	id::GuildId,
};

use crate::{
	permission,
	response::{Emoji, Markdown, Response},
	Bot, InMemoryCacheExt, PartialApplicationCommand,
};

#[async_trait]
pub trait Command {
	/// Required for matching an incoming interaction
	const NAME: &'static str;

	/// Run the command
	async fn run(&self, ctx: &Bot) -> Result<InteractionResponse, ()>;

	/// Define the command, arguments etcetera
	fn define() -> SlashCommand;
}

pub enum Commands {
	List(List),
}

impl Commands {
	pub fn r#match(command: PartialApplicationCommand) -> Option<Self> {
		match command.data.name.as_str() {
			List::NAME => Some(Self::List(List(command))),
			_ => None,
		}
	}

	pub async fn run(&self, ctx: &Bot) -> Result<InteractionResponse, ()> {
		match self {
			Commands::List(c) => c.run(ctx).await,
		}
	}

	pub fn is_long(&self) -> bool {
		match self {
			Commands::List(_) => false,
		}
	}
}

pub struct List(PartialApplicationCommand);

impl List {
	fn errorable(&self, ctx: &Bot, guild_id: GuildId) -> Option<InteractionResponse> {
		let name = match self.0.data.options.first()? {
			CommandDataOption::SubCommand { name, options: _ } => Some(name.as_str()),
			_ => None,
		};
		if let Some(resolved) = &self.0.data.resolved {
			match resolved.channels.first()?.kind {
				ChannelType::GuildVoice => (),
				_ => {
					return Some(Response::message(format!(
						"{} **Not a voice channel**",
						Emoji::WARNING
					)))
				}
			}
			let id = resolved.channels.first()?.id;
			let channel = match ctx.cache.guild_channel(id)? {
				GuildChannel::Voice(c) => c,
				_ => unreachable!("already checked for guild voice channel"),
			};
			return Some(
				if permission(
					&channel,
					guild_id,
					ctx.id,
					&ctx.cache,
					Permissions::MOVE_MEMBERS,
				)? {
					Response::message("`true`")
				} else {
					Response::message("`false`")
				},
			);
		}
		let channels = ctx.cache.voice_channels(guild_id)?.into_iter();
		let channels: Vec<_> = match name? {
			"monitored" => channels
				.filter(move |channel| {
					permission(
						channel,
						guild_id,
						ctx.id,
						&ctx.cache,
						Permissions::MOVE_MEMBERS,
					)
					.unwrap_or(false)
				})
				.collect(),
			"unmonitored" => channels
				.filter(move |channel| {
					!permission(
						channel,
						guild_id,
						ctx.id,
						&ctx.cache,
						Permissions::MOVE_MEMBERS,
					)
					.unwrap_or(true)
				})
				.collect(),
			_ => unreachable!("unexpected input"),
		};
		let channels: String = channels
			.into_iter()
			.map(|channel| format!("`{} {}`\n", Markdown::BULLET_POINT, channel.name))
			.collect();
		Some(if channels.is_empty() {
			Response::message("`None`")
		} else {
			Response::message(channels)
		})
	}
}

#[async_trait]
impl Command for List {
	const NAME: &'static str = "list";

	fn define() -> SlashCommand {
		SlashCommand {
			application_id: None,
			default_permission: None,
			description: String::from("List of monitored or unmonitored voice channels"),
			guild_id: None,
			id: None,
			name: String::from(Self::NAME),
			options: vec![
				CommandOption::SubCommand(OptionsCommandOptionData {
					description: String::from("List monitored voice channels"),
					name: String::from("monitored"),
					options: vec![CommandOption::Channel(BaseCommandOptionData {
						description: String::from("Checks if this voice channel is monitored"),
						name: String::from("channel"),
						required: false,
					})],
					required: false,
				}),
				CommandOption::SubCommand(OptionsCommandOptionData {
					description: String::from("List unmonitored voice channels"),
					name: String::from("unmonitored"),
					options: vec![CommandOption::Channel(BaseCommandOptionData {
						description: String::from("Checks if this voice channel is unmonitored"),
						name: String::from("channel"),
						required: false,
					})],
					required: false,
				}),
			],
		}
	}

	async fn run(&self, ctx: &Bot) -> Result<InteractionResponse, ()> {
		if let Some(guild_id) = self.0.guild_id {
			self.errorable(ctx, guild_id).ok_or(())
		} else {
			Ok(Response::message(format!(
				"{} **This command is only sound inside of guilds**",
				Emoji::WARNING
			)))
		}
	}
}

/// List of [`Command::define`]
pub fn commands() -> Vec<SlashCommand> {
	vec![List::define()]
}
