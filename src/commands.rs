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
	channel::ChannelType,
	guild::Permissions,
	id::{ChannelId, GuildId},
};

use crate::{
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
			let channel_id = match resolved.channels.first()?.kind {
				ChannelType::GuildVoice => resolved.channels.first()?.id,
				_ => {
					return Some(Response::message(format!(
						"{} **Not a voice channel**",
						Emoji::WARNING
					)))
				}
			};

			Some(
				if ctx
					.cache
					.permissions()
					.in_channel(ctx.id, channel_id)
					.unwrap()
					.contains(Permissions::MOVE_MEMBERS)
				{
					Response::message("`true`")
				} else {
					Response::message("`false`")
				},
			)
		} else {
			let voice_channels = ctx
				.cache
				.guild_channels(guild_id)?
				.into_iter()
				.filter_map(|channel_id| ctx.cache.voice_channel(channel_id));

			let managed = |channel_id: ChannelId| -> bool {
				ctx.cache
					.permissions()
					.in_channel(ctx.id, channel_id)
					.expect("cache contains the required info")
					.contains(Permissions::MOVE_MEMBERS)
			};

			let format =
				|name: String| -> String { format!("`{} {}`\n", Markdown::BULLET_POINT, name) };

			let channels: String = match name? {
				"monitored" => voice_channels
					.filter_map(|channel| managed(channel.id).then(|| format(channel.name)))
					.collect(),
				"unmonitored" => voice_channels
					.filter_map(|channel| (!managed(channel.id)).then(|| format(channel.name)))
					.collect(),
				_ => unreachable!("unexpected input"),
			};

			Some(if channels.is_empty() {
				Response::message("`None`")
			} else {
				Response::message(channels)
			})
		}
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
