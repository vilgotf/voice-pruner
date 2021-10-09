use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;
use const_format::formatcp;
use twilight_model::{
	application::{
		command::{Command, CommandType},
		interaction::ApplicationCommand,
	},
	channel::ChannelType,
	guild::Permissions,
	id::GuildId,
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder};

use crate::{
	interaction::{Interaction, Response},
	response::Emoji,
	Bot,
};

use super::SlashCommand;

pub struct Prune(pub(super) ApplicationCommand);

impl Prune {
	async fn errorable(ctx: &Interaction, guild_id: GuildId) -> Option<Cow<'static, str>> {
		if !ctx
			.command
			.member
			.as_ref()
			.expect("is interactions")
			.permissions
			.expect("is interaction")
			.contains(Permissions::MOVE_MEMBERS)
		{
			return Some(Cow::Borrowed(formatcp!(
				"{} **Requires the `MOVE_MEMBERS` permission**",
				Emoji::WARNING
			)));
		}

		let search = ctx.bot.search(guild_id);
		if let Some(channel) = ctx
			.command
			.data
			.resolved
			.as_ref()
			.and_then(|resolved| resolved.channels.first())
		{
			match search.channel(channel.id, None) {
				Ok(users) => Some(Cow::Owned(format!(
					"`{}` members pruned",
					ctx.bot.remove(guild_id, users).await
				))),
				Err(e) => Some(Cow::Borrowed(e.msg()?)),
			}
		} else {
			match search.guild(None) {
				Ok(users) => Some(Cow::Owned(format!(
					"`{}` members pruned",
					ctx.bot.remove(guild_id, users).await
				))),
				Err(e) => Some(Cow::Borrowed(e.msg()?)),
			}
		}
	}
}

#[async_trait]
impl SlashCommand for Prune {
	const NAME: &'static str = "prune";

	fn define() -> Command {
		CommandBuilder::new(
			Self::NAME.to_owned(),
			"Prune users from voice channels".to_owned(),
			CommandType::ChatInput,
		)
		.option(
			ChannelBuilder::new(
				"channel".to_owned(),
				"Only from this voice channel".to_owned(),
			)
			.channel_types([ChannelType::GuildVoice]),
		)
		.build()
	}

	async fn run(self, bot: Bot) -> Result<()> {
		let ctx = Interaction::new(bot, self.0);
		if let Some(guild_id) = ctx.command.guild_id {
			tracing::Span::current().record("guild_id", &guild_id.0);
			// await kicking all members before responding
			ctx.ack().await?;
			let content = Self::errorable(&ctx, guild_id)
				.await
				.unwrap_or(Cow::Borrowed("**Internal error**"));

			ctx.update_response()?
				.content(Some(&content))?
				.exec()
				.await?;
		} else {
			ctx.response(&Response::message(formatcp!(
				"{} **Unavailable in DMs**",
				Emoji::WARNING
			)))
			.await?;
		}
		Ok(())
	}
}
