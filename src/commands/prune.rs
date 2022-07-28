use twilight_model::{
	application::command::{Command, CommandType},
	guild::Permissions,
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{prune, MONITORED_CHANNEL_TYPES};

pub const NAME: &str = "prune";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME,
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
		Some(channel) => prune::channel(channel, guild).await,
		None => prune::guild(guild).await,
	};

	ctx.update_response(&(format!("{users} users pruned")))
		.await
}
