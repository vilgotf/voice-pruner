use twilight_model::{
	application::{
		command::{Command, CommandType},
		interaction::application_command::CommandOptionValue,
	},
	id::{marker::ChannelMarker, Id},
};
use twilight_util::builder::command::{CommandBuilder, StringBuilder};

use crate::{Permissions, BOT, MONITORED_CHANNEL_TYPES};

pub const NAME: &str = "list";

pub fn define() -> Command {
	CommandBuilder::new(NAME, "List visible voice channels", CommandType::ChatInput)
		.default_member_permissions(Permissions::ADMIN)
		.dm_permission(false)
		.option(
			StringBuilder::new("type", "Only monitored / unmonitored voice channels")
				.choices([("Monitored", "monitored"), ("Unmonitored", "unmonitored")]),
		)
		.build()
}

pub async fn run(ctx: super::Context) -> super::Result {
	let guild = ctx.interaction.guild_id.expect("command unavailable in dm");

	let maybe_type = ctx
		.data
		.options
		.first()
		.and_then(|option| match &option.value {
			CommandOptionValue::String(s) => Some(s),
			_ => None,
		});

	let channels = BOT.cache.guild_channels(guild).expect("cached");
	let channels = channels.iter().filter(|&&id| {
		MONITORED_CHANNEL_TYPES.contains(&BOT.cache.channel(id).expect("present").kind)
	});

	let format = |id: Id<ChannelMarker>| format!("• <#{id}>\n");

	let msg: String = match maybe_type {
		Some(r#type) => match r#type.as_str() {
			"monitored" => channels
				.filter_map(|&channel| BOT.is_monitored(channel).then(|| format(channel)))
				.collect(),
			"unmonitored" => channels
				.filter_map(|&channel| (!BOT.is_monitored(channel)).then(|| format(channel)))
				.collect(),
			_ => todo!(),
		},
		None => channels.map(|&channel| format(channel)).collect(),
	};

	let msg = if msg.is_empty() {
		"none".to_owned()
	} else {
		msg
	};

	ctx.reply(msg).await
}
