use anyhow::Result;
use async_trait::async_trait;
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
	async fn errorable(ctx: &Interaction, guild_id: GuildId) -> Option<String> {
		if !ctx
			.command
			.member
			.as_ref()
			.expect("included in guild interactions")
			.permissions
			.expect("is interaction")
			.contains(Permissions::MOVE_MEMBERS)
		{
			return Some(format!(
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
						Some(String::from("command successful"))
					}
					Err(e) => Some(e.msg()?),
				};
			}
		}
		match search.guild(None) {
			Ok(users) => {
				ctx.bot.remove(guild_id, users).await;
				Some(String::from("command successful"))
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
			description: String::from("Prune users from voice channels"),
			guild_id: None,
			id: None,
			name: String::from(Self::NAME),
			options: vec![CommandOption::Channel(BaseCommandOptionData {
				description: String::from("Only from this voice channel"),
				name: String::from("channel"),
				required: false,
			})],
		}
	}

	async fn run(self, ctx: Bot) -> Result<()> {
		let interaction = ctx.interaction(self.0);
		if let Some(guild_id) = interaction.command.guild_id {
			tracing::Span::current().record("guild_id", &guild_id.0);
			// await kicking all members before responding
			interaction.ack().await?;
			let content = Self::errorable(&interaction, guild_id)
				.await
				.unwrap_or_else(|| String::from("**Internal error**"));

			interaction
				.update_response()?
				.content(Some(&content))?
				.exec()
				.await?;
		} else {
			interaction
				.response(&Response::message(format!(
					"{} **Unavailable in DMs**",
					Emoji::WARNING
				)))
				.await?;
		}
		Ok(())
	}
}
