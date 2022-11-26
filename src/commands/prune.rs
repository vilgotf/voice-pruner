use twilight_model::{
	application::command::{Command, CommandType},
	guild::Permissions,
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::MONITORED_CHANNEL_TYPES;

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
	.build()
}

pub async fn run(ctx: super::Context) -> super::Result {
	let guild = ctx.interaction.guild_id.expect("command unavailable in dm");

	// await kicking all members before responding
	ctx.ack().await?;

	let users = match super::resolved_channel(&ctx.data) {
		Some(channel) => crate::prune::channel(channel, guild).await,
		None => crate::prune::guild(guild).await,
	};

	ctx.update_response(&(format!("{users} users pruned")))
		.await
}
