use twilight_model::application::command::{Command, CommandType};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{Permissions, Search, BOT, MONITORED_CHANNEL_TYPES};

pub const NAME: &str = "prune";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME,
		"Prune users from voice channels",
		CommandType::ChatInput,
	)
	.default_member_permissions(Permissions::ADMIN)
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
		Some(channel) => BOT
			.is_monitored(channel)
			.then(|| Search::channel(channel))
			.unwrap_or_default(),
		None => Search::guild(guild),
	};

	let msg = format!("{} users pruned", BOT.remove(guild, users).await);

	ctx.update_response(&msg).await
}
