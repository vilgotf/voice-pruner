use twilight_model::{
	application::{
		command::{Command, CommandType},
		interaction::application_command::CommandOptionValue,
	},
	id::{marker::ChannelMarker, Id},
};
use twilight_util::builder::command::{CommandBuilder, StringBuilder};

use crate::{Permissions, Symbol, MONITORED_CHANNEL_TYPES};

pub const NAME: &str = "list";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME.to_owned(),
		"List visible voice channels".to_owned(),
		CommandType::ChatInput,
	)
	.default_member_permissions(Permissions::ADMIN)
	.dm_permission(false)
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

pub async fn run(ctx: super::Context) -> super::Result {
	let guild = ctx.command.guild_id.expect("command unavailable in dm");

	let maybe_type = ctx
		.command
		.data
		.options
		.first()
		.and_then(|option| match &option.value {
			CommandOptionValue::String(s) => Some(s),
			_ => None,
		});

	let channels = ctx.bot.cache.guild_channels(guild).expect("cached");
	let voice_channels = channels.iter().filter_map(|&channel_id| {
		ctx.bot
			.cache
			.channel(channel_id)
			.and_then(|channel| (channel.kind == MONITORED_CHANNEL_TYPES).then(|| channel))
	});

	let format = |id: Id<ChannelMarker>| format!("{} <#{id}>\n", Symbol::BULLET_POINT);

	let msg: String = match maybe_type {
		Some(r#type) => match r#type.as_str() {
			"monitored" => voice_channels
				.filter_map(|channel| {
					(ctx.bot.is_monitored(channel.id)).then(|| format(channel.id))
				})
				.collect(),
			"unmonitored" => voice_channels
				.filter_map(|channel| {
					(!ctx.bot.is_monitored(channel.id)).then(|| format(channel.id))
				})
				.collect(),
			_ => todo!(),
		},
		None => voice_channels.map(|channel| format(channel.id)).collect(),
	};

	let msg = if msg.is_empty() {
		"none".to_owned()
	} else {
		msg
	};

	ctx.reply(msg).await
}
