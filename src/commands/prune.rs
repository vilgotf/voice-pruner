use twilight_model::application::command::{Command, CommandType};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{Permissions, MONITORED_CHANNEL_TYPES};

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

	let to_remove = match super::resolved_channel(&ctx.data) {
		Some(channel) => ctx
			.bot
			.is_monitored(channel)
			.then(|| ctx.bot.search(guild).channel(channel))
			.unwrap_or_default(),
		None => ctx.bot.search(guild).guild(),
	};

	let msg = format!("{} users pruned", ctx.bot.remove(guild, to_remove).await);

	ctx.update_response(&msg).await
}
