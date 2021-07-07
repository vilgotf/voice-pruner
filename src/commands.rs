use strum::{EnumIter, IntoEnumIterator};
#[allow(unused_imports)]
use tracing::{event, Level};
use twilight_model::{
	application::{
		callback::InteractionResponse,
		command::{
			BaseCommandOptionData, Command as SlashCommand, CommandOption, OptionsCommandOptionData,
		},
		interaction::{application_command::CommandDataOption, ApplicationCommand},
	},
	channel::{ChannelType, GuildChannel},
	guild::Permissions,
};

use crate::{permission, response, Bot, InMemoryCacheExt};

struct Emoji;

impl Emoji {
	const WARNING: &'static str = "\u{26A0}\u{FE0F}";
}

struct Markdown;

impl Markdown {
	const BULLET_POINT: &'static str = "\u{2022}";
}

pub trait Command {
	fn name(&self) -> &'static str;

	fn run(&self, ctx: &Bot, command: &ApplicationCommand) -> Option<InteractionResponse>;

	fn command(&self) -> SlashCommand;
}

#[derive(Clone, Copy, Debug, EnumIter)]
pub enum Commands {
	About(About),
	List(List),
	Ping(Ping),
}

impl Command for Commands {
	fn command(&self) -> SlashCommand {
		match self {
			Commands::About(c) => c.command(),
			Commands::List(c) => c.command(),
			Commands::Ping(c) => c.command(),
		}
	}

	fn name(&self) -> &'static str {
		match self {
			Commands::About(c) => c.name(),
			Commands::List(c) => c.name(),
			Commands::Ping(c) => c.name(),
		}
	}

	fn run(&self, ctx: &Bot, command: &ApplicationCommand) -> Option<InteractionResponse> {
		match self {
			Commands::About(c) => c.run(ctx, command),
			Commands::List(c) => c.run(ctx, command),
			Commands::Ping(c) => c.run(ctx, command),
		}
	}
}

impl Commands {
	pub fn r#match(name: &str) -> Option<Self> {
		Commands::iter().find(|command| command.name() == name)
	}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct About;

impl Command for About {
	fn command(&self) -> SlashCommand {
		SlashCommand {
			application_id: None,
			default_permission: None,
			description: String::from("Show information about the bot"),
			guild_id: None,
			id: None,
			name: String::from(self.name()),
			options: vec![],
		}
	}

	fn name(&self) -> &'static str {
		"about"
	}

	fn run(&self, _: &Bot, _: &ApplicationCommand) -> Option<InteractionResponse> {
		todo!()
	}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct List;

impl Command for List {
	fn command(&self) -> SlashCommand {
		SlashCommand {
			application_id: None,
			default_permission: None,
			description: String::from("List of monitored or unmonitored voice channels"),
			guild_id: None,
			id: None,
			name: String::from(self.name()),
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

	fn name(&self) -> &'static str {
		"list"
	}

	fn run(&self, ctx: &Bot, command: &ApplicationCommand) -> Option<InteractionResponse> {
		if let Some(guild_id) = command.guild_id {
			let name = match command.data.options.first()? {
				CommandDataOption::SubCommand { name, options: _ } => Some(name.as_str()),
				_ => None,
			};
			if let Some(resolved) = &command.data.resolved {
				match resolved.channels.first()?.kind {
					ChannelType::GuildVoice => (),
					_ => {
						return Some(response(format!(
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
						response("true")
					} else {
						response("false")
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
				response("None")
			} else {
				response(channels)
			})
		} else {
			Some(response(format!(
				"{} **This command is only sound inside of guilds**",
				Emoji::WARNING
			)))
		}
	}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Ping;

impl Command for Ping {
	fn command(&self) -> SlashCommand {
		SlashCommand {
			application_id: None,
			default_permission: None,
			description: String::from("Latency between the gateway and discord"),
			guild_id: None,
			id: None,
			name: String::from(self.name()),
			options: vec![],
		}
	}
	fn name(&self) -> &'static str {
		"ping"
	}

	fn run(&self, _: &Bot, _: &ApplicationCommand) -> Option<InteractionResponse> {
		Some(response("Pong"))
	}
}

/// Returns all [`Commands::command`]
pub fn commands() -> Vec<SlashCommand> {
	Commands::iter().map(|c| c.command()).collect()
}
