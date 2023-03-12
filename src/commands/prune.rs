use twilight_model::{
	application::{
		command::{Command, CommandType},
		interaction::application_command::CommandOptionValue,
	},
	guild::Permissions,
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder, RoleBuilder};

use crate::{BOT, MONITORED_CHANNEL_TYPES};

pub fn define() -> Command {
	CommandBuilder::new(
		"prune",
		"Prune users from voice channels",
		CommandType::ChatInput,
	)
	.default_member_permissions(Permissions::MOVE_MEMBERS)
	.dm_permission(false)
	.option(
		ChannelBuilder::new("channel", "Only from this voice channel")
			.channel_types(MONITORED_CHANNEL_TYPES),
	)
	.option(RoleBuilder::new("role", "Only users with this role"))
	.build()
}

pub async fn run(ctx: super::Context) -> super::Result {
	let guild = ctx.interaction.guild_id.expect("required");

	// await kicking all members before responding
	ctx.ack().await?;

	let mut channel = None;
	let mut role = None;

	for option in &ctx.data.options {
		match option.name.as_str() {
			"channel" => match option.value {
				CommandOptionValue::Channel(id) => channel = Some(id),
				_ => unreachable!("undefined"),
			},
			"role" => match option.value {
				CommandOptionValue::Role(id) => role = Some(id),
				_ => unreachable!("undefined"),
			},
			_ => unreachable!("undefined"),
		}
	}

	let users = match (channel, role) {
		(None, None) => crate::prune::guild(guild, |_| true).await,
		(None, Some(role)) => {
			crate::prune::guild(guild, |state| {
				BOT.cache
					.member(state.guild_id(), state.user_id())
					.map_or(false, |member| member.roles().contains(&role))
			})
			.await
		}
		(Some(channel), None) => crate::prune::channel(channel, guild, |_| true).await,
		(Some(channel), Some(role)) => {
			crate::prune::channel(channel, guild, |state| {
				BOT.cache
					.member(state.guild_id(), state.user_id())
					.map_or(false, |member| member.roles().contains(&role))
			})
			.await
		}
	};

	ctx.update_response(&(format!("{users} users pruned")))
		.await
}
