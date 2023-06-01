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
use twilight_gateway::{error::ReceiveMessageErrorType, Config, EventTypeFlags, Shard, ShardId};
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
			.map(|token| token.replace('\n', ""))
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
		loop {
			match shard.next_event().await {
				Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => return Ok(()),
				Ok(event) => {
					tokio::spawn(handle(event));
				}
				Err(error)
					if matches!(error.kind(), ReceiveMessageErrorType::Io)
						&& SHUTDOWN.load(Ordering::Relaxed) =>
				{
					return Ok(())
				}
				Err(error) if error.is_fatal() => {
					return Err(
						anyhow::anyhow!(error).context(format!("shard {} fatal error", shard.id()))
					);
				}
				Err(error) => {
					let _span = tracing::info_span!("shard", id = %shard.id()).entered();
					tracing::warn!(error = &*anyhow::anyhow!(error));
					continue;
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

	handle.await?
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
	let cache = {
		// `/list` requires `CHANNEL`.
		// `BOT.is_monitored` requires `CHANNEL`, `MEMBER` & `ROLE`.
		// pruning requires `VOICE_STATE`
		let resource_types = ResourceType::CHANNEL
			| ResourceType::MEMBER
			| ResourceType::ROLE
			| ResourceType::VOICE_STATE;
		InMemoryCache::builder()
			.resource_types(resource_types)
			.build()
	};

	let http = Client::new(token.clone());

	let shard = {
		let event_types = EventTypeFlags::CHANNEL_CREATE
			| EventTypeFlags::CHANNEL_DELETE
			| EventTypeFlags::CHANNEL_UPDATE
			| EventTypeFlags::GUILD_CREATE
			| EventTypeFlags::GUILD_DELETE
			| EventTypeFlags::GUILD_MEMBERS
			| EventTypeFlags::GUILD_UPDATE
			| EventTypeFlags::GUILD_VOICE_STATES
			| EventTypeFlags::INTERACTION_CREATE
			| EventTypeFlags::READY
			| EventTypeFlags::ROLE_CREATE
			| EventTypeFlags::ROLE_DELETE
			| EventTypeFlags::ROLE_UPDATE;
		let intents = Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_VOICE_STATES;
		let config = Config::builder(token.clone(), intents)
			.event_types(event_types)
			.build();
		Shard::with_config(ShardId::ONE, config)
	};

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
			cache,
			http,
			id,
		})
		.expect("only called once");

	Ok(shard)
}
