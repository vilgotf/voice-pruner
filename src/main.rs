//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

mod commands;
mod prune;

use std::{
	env,
	ops::Deref,
	sync::{
		atomic::{AtomicBool, Ordering},
		OnceLock,
	},
};

use anyhow::Context;
use futures_util::stream::{self, StreamExt};
use tokio::signal;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{
	error::ReceiveMessageErrorType, EventTypeFlags, Shard, ShardId, StreamExt as _,
};
use twilight_http::Client;
use twilight_model::{
	application::interaction::InteractionType,
	channel::ChannelType,
	gateway::{
		event::Event,
		payload::incoming::{RoleDelete, RoleUpdate},
		CloseFrame, Intents,
	},
	guild::Permissions,
	id::{
		marker::{ApplicationMarker, ChannelMarker, GuildMarker, UserMarker},
		Id,
	},
};

/// Bot context, initialized by calling `init()`.
///
/// Derefs to [`BotRef`].
///
/// # Panics
///
/// Panics if accessed before `init()` was called.
static BOT: Bot = Bot(OnceLock::new());

/// [`BOT`] wrapper type required for [`Deref`].
#[repr(transparent)]
struct Bot(OnceLock<BotRef>);

impl Deref for Bot {
	type Target = BotRef;

	#[track_caller]
	fn deref(&self) -> &Self::Target {
		self.0.get().expect("initialized before accessed")
	}
}

/// Event types the bot requires, filtered from [`INTENTS`].
const EVENT_TYPES: EventTypeFlags = EventTypeFlags::CHANNEL_CREATE
	.union(EventTypeFlags::CHANNEL_DELETE)
	.union(EventTypeFlags::CHANNEL_UPDATE)
	.union(EventTypeFlags::GUILD_CREATE)
	.union(EventTypeFlags::GUILD_DELETE)
	.union(EventTypeFlags::GUILD_MEMBERS)
	.union(EventTypeFlags::GUILD_UPDATE)
	.union(EventTypeFlags::GUILD_VOICE_STATES)
	.union(EventTypeFlags::INTERACTION_CREATE)
	.union(EventTypeFlags::READY)
	.union(EventTypeFlags::ROLE_CREATE)
	.union(EventTypeFlags::ROLE_DELETE)
	.union(EventTypeFlags::ROLE_UPDATE);

/// [`Intents`] the bot requires.
const INTENTS: Intents = Intents::GUILDS
	.union(Intents::GUILD_MEMBERS)
	.union(Intents::GUILD_VOICE_STATES);

/// Resources the bot caches.
///
/// - `/list` requires `CHANNEL`.
/// - `BOT.is_monitored` requires `CHANNEL`, `MEMBER` & `ROLE`.
/// - pruning requires `VOICE_STATE`
const RESOURCES: ResourceType = ResourceType::CHANNEL
	.union(ResourceType::MEMBER)
	.union(ResourceType::ROLE)
	.union(ResourceType::VOICE_STATE);

/// Flag indicating bot should shut down.
///
/// Used by the shard, not by event handler tasks.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// [`ChannelType`]s the bot operates on.
///
/// Must only be voice channels.
const MONITORED_CHANNEL_TYPES: [ChannelType; 2] =
	[ChannelType::GuildVoice, ChannelType::GuildStageVoice];

#[tracing::instrument(name = "retrieve bot token")]
fn get_token() -> Result<String, anyhow::Error> {
	// https://systemd.io/CREDENTIALS/
	#[cfg(target_os = "linux")]
	if let Some(mut path) = env::var_os("CREDENTIALS_DIRECTORY") {
		tracing::debug!("using systemd credentials");
		path.push("/token");
		return std::fs::read_to_string(path)
			.map(|mut token| {
				if token.ends_with('\n') {
					token.truncate(token.len() - 1)
				}
				token
			})
			.context("unable to retrieve bot token from the \"token\" systemd credential");
	}

	tracing::debug!("using environment variable");
	#[cfg(target_os = "linux")]
	tracing::info!("prefer systemd credentials for improved security");
	env::var("TOKEN")
		.context("unable to retrieve bot token from the \"TOKEN\" environment variable")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
	tracing_subscriber::fmt::init();

	let token = get_token()?;

	let mut shard = init(token).await.context("unable to initialize bot")?;
	let sender = shard.sender();

	let handle = tokio::spawn(async move {
		while let Some(res) = shard.next_event(EVENT_TYPES).await {
			match res {
				Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => break,
				Ok(event) => {
					tokio::spawn(handle(event));
				}
				Err(error)
					if matches!(error.kind(), ReceiveMessageErrorType::WebSocket)
						&& SHUTDOWN.load(Ordering::Relaxed) =>
				{
					break;
				}
				Err(error) => {
					let _span = tracing::info_span!("shard", id = %shard.id()).entered();
					tracing::warn!(error = &error as &dyn std::error::Error);
				}
			}
		}
	});

	#[cfg(target_family = "unix")]
	{
		use signal::unix::*;

		let mut sigint =
			signal(SignalKind::interrupt()).context("unable to register SIGINT handler")?;
		let mut sigterm =
			signal(SignalKind::terminate()).context("unable to register SIGTERM handler")?;

		tokio::select! {
				_ = sigint.recv() => tracing::trace!("received SIGINT"),
				_ = sigterm.recv() => tracing::trace!("received SIGTERM"),
		}
	}

	#[cfg(not(target_family = "unix"))]
	signal::ctrl_c()
		.await
		.context("unable to register Ctrl+C handler")?;

	tracing::debug!("shutting down");

	SHUTDOWN.store(true, Ordering::Relaxed);
	_ = sender.close(CloseFrame::NORMAL);

	handle.await?;
	Ok(())
}

/// Handle a gateway [`Event`].
async fn handle(event: Event) {
	let skip = match &event {
		Event::ChannelUpdate(c) => BOT
			.cache
			.channel(c.id)
			.is_some_and(|cached| cached.permission_overwrites == c.permission_overwrites),
		Event::RoleUpdate(r) => BOT
			.cache
			.role(r.role.id)
			.is_some_and(|cached| cached.permissions == r.role.permissions),
		_ => false,
	};

	BOT.cache.update(&event);

	if skip {
		return;
	}

	match event {
		Event::ChannelUpdate(c) if BOT.auto_prune(c.guild_id.unwrap()) => {
			crate::prune::channel(c.id, c.guild_id.unwrap(), |_| true).await;
		}
		Event::MemberUpdate(m) if BOT.auto_prune(m.guild_id) => {
			crate::prune::user(m.guild_id, m.user.id).await;
		}
		Event::RoleDelete(RoleDelete { guild_id, .. })
		| Event::RoleUpdate(RoleUpdate { guild_id, .. })
			if BOT.auto_prune(guild_id) =>
		{
			crate::prune::guild(guild_id, |_| true).await;
		}
		Event::InteractionCreate(interaction) => match interaction.kind {
			InteractionType::ApplicationCommand => {
				crate::commands::interaction(interaction.0).await;
			}
			_ => tracing::info!(?interaction, "unhandled"),
		},
		Event::Ready(r) => {
			tracing::debug!(guilds = %r.guilds.len(), user = %r.user.name);
		}
		_ => {}
	}
}

/// "Real" [`BOT`] struct.
///
/// Contains required modules: a HTTP client, and cache and state: bot user ID,
/// and bot application ID.
#[derive(Debug)]
struct BotRef {
	application_id: Id<ApplicationMarker>,
	cache: InMemoryCache,
	http: Client,
	/// User ID of the bot
	id: Id<UserMarker>,
}

impl BotRef {
	/// Whether the guild has auto prune enabled.
	fn auto_prune(&self, guild: Id<GuildMarker>) -> bool {
		// event order isn't guarenteed, so this might not be cached yet
		self.cache.member(guild, self.id).is_some_and(|member| {
			!member
				.roles()
				.iter()
				.any(|&role| self.cache.role(role).unwrap().name == "no-auto-prune")
		})
	}

	/// Whether the voice channel is monitored.
	fn is_monitored(&self, channel: Id<ChannelMarker>) -> bool {
		self.cache
			.permissions()
			.in_channel(self.id, channel)
			.expect("resources are available")
			.contains(Permissions::MOVE_MEMBERS)
	}

	/// Removes users, logging on error.
	///
	/// Returns the number of users removed.
	async fn remove(
		&self,
		guild: Id<GuildMarker>,
		users: impl IntoIterator<Item = Id<UserMarker>>,
	) -> u16 {
		stream::iter(users)
			.map(|user| async move {
				tracing::debug!(user.id = %user, "kicking");
				match self
					.http
					.update_guild_member(guild, user)
					.channel_id(None)
					.await
				{
					Ok(_) => 1,
					Err(e) => {
						tracing::warn!(error = &e as &dyn std::error::Error);
						0
					}
				}
			})
			.fold(0, |a, b| async move { a + b.await })
			.await
	}
}

/// Initializes [`BOT`] and returns a shard.
///
/// # Panics
///
/// Panics if called multiple times.
#[tracing::instrument(skip_all)]
async fn init(token: String) -> Result<Shard, anyhow::Error> {
	let http = Client::new(token.clone());

	let (application_id, id) = tokio::try_join!(
		async {
			let application_id = http.current_user_application().await?.model().await?.id;
			http.interaction(application_id)
				.set_global_commands(&commands::get())
				.await?;
			Ok::<_, anyhow::Error>(application_id)
		},
		async { Ok(http.current_user().await?.model().await?.id) }
	)?;

	tracing::debug!(%application_id, user_id = %id);

	BOT.0
		.set(BotRef {
			application_id,
			cache: InMemoryCache::builder().resource_types(RESOURCES).build(),
			http,
			id,
		})
		.expect("only called once");

	Ok(Shard::new(ShardId::ONE, token, INTENTS))
}
