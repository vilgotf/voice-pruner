//! Incoming Discord events.

use std::iter::once;

use tracing::{event, instrument, Level};
use twilight_gateway::Event;
use twilight_model::{
	application::interaction::{ApplicationCommand, Interaction},
	id::{
		marker::{ChannelMarker, GuildMarker, RoleMarker, UserMarker},
		Id,
	},
};

use crate::Bot;

/// Process an event.
pub async fn process(bot: Bot, event: Event) {
	let skip = match &event {
		// channel updates requires recheking its connected users, so skip if channel
		// `permission_overwrites` didn't change
		Event::ChannelUpdate(c) => bot.cache.channel(c.id).map_or(false, |cached| {
			cached.permission_overwrites != c.permission_overwrites
		}),
		// role updates requires rechecking all the guilds connected users, so skip if role
		// permissions didn't change
		Event::RoleUpdate(r) => {
			bot.cache.role(r.role.id).map(|r| r.permissions) != Some(r.role.permissions)
		}
		_ => false,
	};

	bot.cache.update(&event);

	if skip {
		return;
	}

	match event {
		Event::ChannelUpdate(c) => {
			auto_prune(bot, c.guild_id.expect("present"), Mode::Channel(c.id)).await
		}
		Event::MemberUpdate(m) => {
			auto_prune(bot, m.guild_id, Mode::Member(m.user.id)).await;
		}
		Event::RoleDelete(r) => {
			auto_prune(bot, r.guild_id, Mode::Role(None)).await;
		}
		Event::RoleUpdate(r) => {
			auto_prune(bot, r.guild_id, Mode::Role(Some(r.role.id))).await;
		}
		Event::InteractionCreate(i) => match i.0 {
			Interaction::ApplicationCommand(cmd) => command(bot, *cmd).await,
			interaction => event!(Level::WARN, ?interaction, "unhandled"),
		},
		Event::Ready(r) => event!(Level::INFO, guilds = %r.guilds.len(), user = %r.user.name),
		_ => (),
	}
}

#[derive(Debug)]
enum Mode {
	Channel(Id<ChannelMarker>),
	Member(Id<UserMarker>),
	Role(Option<Id<RoleMarker>>),
}

#[instrument(skip(bot), fields(%guild_id, ?mode))]
async fn auto_prune(bot: Bot, guild_id: Id<GuildMarker>, mode: Mode) {
	/// Returns `true` if bot has the "no-auto-prune" role.
	fn is_disabled(bot: Bot, guild_id: Id<GuildMarker>) -> bool {
		if let Some(member) = bot.cache.member(guild_id, bot.id) {
			member.roles().iter().any(|&role_id| {
				bot.cache
					.role(role_id)
					.map_or(false, |role| role.name == "no-auto-prune")
			})
		} else {
			// Ordering isn't guarenteed, GuildCreate might be sent after others.
			true
		}
	}

	if is_disabled(bot, guild_id) {
		return;
	}

	let search = bot.search(guild_id);

	let users = match mode {
		Mode::Channel(c) => search.channel(c, None).unwrap_or_default(),
		Mode::Member(user_id) => {
			return if search.user(user_id).unwrap_or_default() {
				bot.remove(guild_id, once(user_id)).await;
			}
		}
		Mode::Role(role_id) => search.guild(role_id).unwrap_or_default(),
	};

	bot.remove(guild_id, users.into_iter()).await;
}

#[instrument(skip(bot, command), fields(guild_id, command.name = %command.data.name))]
async fn command(bot: Bot, command: ApplicationCommand) {
	if let Some(guild_id) = command.guild_id {
		tracing::Span::current().record("guild_id", &guild_id.get());
	}

	if let Err(e) = crate::commands::run(bot, command).await {
		event!(
			Level::ERROR,
			error = &*e as &dyn std::error::Error,
			"error running command"
		);
	}
}
