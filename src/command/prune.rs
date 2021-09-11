use anyhow::Result;
use async_trait::async_trait;
use const_format::formatcp;
use twilight_model::{
	application::{
		command::{BaseCommandOptionData, Command, CommandOption},
		interaction::ApplicationCommand,
	},
	guild::Permissions,
	id::GuildId,
};

use crate::{
	interaction::{Interaction, Response},
	response::Emoji,
	Bot,
};

use super::SlashCommand;

pub struct Prune(pub(super) ApplicationCommand);

impl Prune {
	async fn errorable(ctx: &Interaction, guild_id: GuildId) -> Option<&str> {
		if !ctx
			.command
			.member
			.as_ref()
			.expect("is interactions")
			.permissions
			.expect("is interaction")
			.contains(Permissions::MOVE_MEMBERS)
		{
			return Some(formatcp!(
				"{} **Requires the `MOVE_MEMBERS` permission**",
				Emoji::WARNING
			));
		}

		let search = ctx.bot.search(guild_id);
		if let Some(resolved) = &ctx.command.data.resolved {
			if let Some(channel) = resolved.channels.first() {
				return match search.channel(channel.id, None) {
					Ok(users) => {
						ctx.bot.remove(guild_id, users).await;
						Some("command successful")
					}
					Err(e) => Some(e.msg()?),
				};
			}
		}
		match search.guild(None) {
			Ok(users) => {
				ctx.bot.remove(guild_id, users).await;
				Some("command successful")
			}
			Err(e) => Some(e.msg()?),
		}
	}
}

#[async_trait]
impl SlashCommand for Prune {
	const NAME: &'static str = "prune";

	fn define() -> Command {
		Command {
			application_id: None,
			default_permission: None,
			description: "Prune users from voice channels".to_owned(),
			guild_id: None,
			id: None,
			name: Self::NAME.to_owned(),
			options: vec![CommandOption::Channel(BaseCommandOptionData {
				description: "Only from this voice channel".to_owned(),
				name: "channel".to_owned(),
				required: false,
			})],
		}
	}

	async fn run(self, bot: Bot) -> Result<()> {
		let ctx = Interaction::new(bot, self.0);
		if let Some(guild_id) = ctx.command.guild_id {
			tracing::Span::current().record("guild_id", &guild_id.0);
			// await kicking all members before responding
			ctx.ack().await?;
			let content = Self::errorable(&ctx, guild_id)
				.await
				.unwrap_or("**Internal error**");

			ctx.update_response()?
				.content(Some(content))?
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
