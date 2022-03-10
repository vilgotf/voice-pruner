use std::borrow::Cow;

use const_format::formatcp;
use twilight_model::{
	application::command::{Command, CommandType},
	id::{marker::GuildMarker, Id},
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{
	interaction::{Interaction, Response},
	Permissions, Symbol,
};
pub const NAME: &str = "prune";

pub fn define() -> Command {
	CommandBuilder::new(
		NAME.to_owned(),
		"Prune users from voice channels".to_owned(),
		CommandType::ChatInput,
	)
	.option(
		ChannelBuilder::new(
			"channel".to_owned(),
			"Only from this voice channel".to_owned(),
		)
		.channel_types([crate::MONITORED_CHANNEL_TYPES]),
	)
	.build()
}

pub async fn run(ctx: Interaction) -> super::Result {
	if let Some(guild_id) = ctx.command.guild_id {
		// await kicking all members before responding
		ctx.bot
			.as_interaction()
			.create_response(ctx.command.id, &ctx.command.token, &Response::ack())
			.exec()
			.await?;

		let content = errorable(&ctx, guild_id)
			.await
			.unwrap_or(Cow::Borrowed("**Internal error**"));

		ctx.bot
			.as_interaction()
			.update_response(&ctx.command.token)
			.content(Some(&content))?
			.exec()
			.await?;
	} else {
		ctx.response(&Response::message(format!(
			"{} **Unavailable in DMs**",
			Symbol::WARNING
		)))
		.await?;
	}
	Ok(())
}

async fn errorable(ctx: &Interaction, guild_id: Id<GuildMarker>) -> Option<Cow<'static, str>> {
	if !ctx.caller_permissions().contains(Permissions::ADMIN) {
		return Some(Cow::Borrowed(formatcp!(
			"{} **Requires the `MOVE_MEMBERS` permission**",
			Symbol::WARNING
		)));
	}

	match if let Some(channel_id) = super::specified_channel(&ctx.command.data) {
		ctx.bot.search(guild_id).channel(channel_id, None)
	} else {
		ctx.bot.search(guild_id).guild(None)
	} {
		Ok(users) => Some(Cow::Owned(format!(
			"`{}` members pruned",
			ctx.bot.remove(guild_id, users.into_iter()).await
		))),
		Err(e) => Some(Cow::Borrowed(e.msg()?)),
	}
}
