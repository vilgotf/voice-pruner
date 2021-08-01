use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;
use twilight_model::{
	application::{
		command::{BaseCommandOptionData, Command, CommandOption},
		interaction::ApplicationCommand,
	},
	guild::Permissions,
	id::{ChannelId, GuildId, RoleId},
};

use crate::{
	interaction::{Interaction, Response},
	response::Emoji,
	Bot,
};

use super::SlashCommand;

pub struct Prune(pub(super) ApplicationCommand);

impl Prune {
	async fn channel(
		ctx: &Interaction,
		guild_id: GuildId,
		channel_id: ChannelId,
		role_id: Option<RoleId>,
	) -> Option<Cow<'_, str>> {
		let search = ctx.bot.search(guild_id);
		match search.channel(channel_id, role_id) {
			Ok(users) => {
				ctx.bot.remove_mul(guild_id, users).await;
				Some(Cow::Borrowed("command successful"))
			}
			Err(e) => Some(e.msg()?.into()),
		}
	}

	async fn guild(
		ctx: &Interaction,
		guild_id: GuildId,
		role_id: Option<RoleId>,
	) -> Option<Cow<'_, str>> {
		let search = ctx.bot.search(guild_id);
		match search.guild(role_id) {
			Ok(users) => {
				ctx.bot.remove_mul(guild_id, users).await;
				Some(Cow::Borrowed("command successful"))
			}
			Err(e) => Some(e.msg()?.into()),
		}
	}

	async fn errorable(ctx: &Interaction, guild_id: GuildId) -> Option<Cow<'_, str>> {
		if !ctx
			.command
			.member
			.as_ref()?
			.permissions?
			.contains(Permissions::MOVE_MEMBERS)
		{
			return Some(Cow::Owned(format!(
				"{} **Requires the `MOVE_MEMBERS` permission**",
				Emoji::WARNING
			)));
		}

		match &ctx.command.data.resolved {
			Some(resolved) => match (resolved.channels.first(), resolved.roles.first()) {
				(None, None) => Self::guild(ctx, guild_id, None).await,
				(None, Some(r)) => Self::guild(ctx, guild_id, Some(r.id)).await,
				(Some(c), None) => Self::channel(ctx, guild_id, c.id, None).await,
				(Some(c), Some(r)) => Self::channel(ctx, guild_id, c.id, Some(r.id)).await,
			},
			None => Self::guild(ctx, guild_id, None).await,
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
			description: String::from(
				"Prune users without the required permissions from their calls",
			),
			guild_id: None,
			id: None,
			name: String::from(Self::NAME),
			options: vec![
				CommandOption::Channel(BaseCommandOptionData {
					description: String::from("Only this voice channel"),
					name: String::from("channel"),
					required: false,
				}),
				CommandOption::Role(BaseCommandOptionData {
					description: String::from("Only this role"),
					name: String::from("role"),
					required: false,
				}),
			],
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
				.unwrap_or(Cow::Borrowed("**Internal error**"));

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
