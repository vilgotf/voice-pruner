//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

mod cli;
mod commands;
mod prune;

use std::{convert::Infallible, env, fs, ops::Deref};

use anyhow::Context;
use futures_util::future::join_all;
use once_cell::sync::OnceCell;
use tokio::signal::unix::{signal, SignalKind};
use tracing::Level;
use tracing_subscriber::{filter, prelude::*};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{CloseFrame, Config, Event, EventTypeFlags, Intents, Shard, ShardId};
use twilight_http::Client;
use twilight_model::{
	application::interaction::InteractionType,
	channel::ChannelType,
	gateway::payload::incoming::{RoleDelete, RoleUpdate},
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
static BOT: Bot = Bot(OnceCell::new());

/// [`BOT`] wrapper type required for [`Deref`].
#[repr(transparent)]
struct Bot(OnceCell<BotRef>);

impl Deref for Bot {
	type Target = BotRef;

	#[track_caller]
	fn deref(&self) -> &Self::Target {
		self.0.get().expect("initialized before accessed")
	}
}

/// [`ChannelType`]s the bot operates on.
///
/// Must only be voice channels.
const MONITORED_CHANNEL_TYPES: [ChannelType; 2] =
	[ChannelType::GuildVoice, ChannelType::GuildStageVoice];

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
	let args = cli::Args::parse();

	tracing_subscriber::registry()
		.with(
			tracing_subscriber::fmt::layer().with_filter(filter::filter_fn(move |metadata| {
				let level = *metadata.level();
				let target = metadata.target();

				level <= Level::DEBUG
					&& (!target.contains("h2") || level < Level::DEBUG)
					&& (!target.contains("rustls") || level < Level::DEBUG)
			})),
		)
		.init();

	let span = tracing::info_span!("retrieving Discord bot token").entered();
	// https://systemd.io/CREDENTIALS/
	let token = match env::var_os("CREDENTIALS_DIRECTORY") {
		Some(mut path) if cfg!(target_os = "linux") => {
			tracing::info!("using systemd credentials");
			path.push("/token");
			let mut string = fs::read_to_string(path)?;
			string.truncate(string.trim_end().len());
			string
		}
		_ => {
			tracing::info!("using env variable");
			if cfg!(target_os = "linux") {
				tracing::info!("prefer systemd credentials for improved security");
			}
			env::var("TOKEN")?
		}
	};

	span.exit();

	let mut shard = init(args, token)
		.await
		.context("unable to initialize bot")?;

	let mut sigint =
		signal(SignalKind::interrupt()).context("unable to register SIGINT handler")?;
	let mut sigterm =
		signal(SignalKind::terminate()).context("unable to register SIGTERM handler")?;

	tokio::select! {
		Err(e) = BotRef::process(&mut shard) => return Err(anyhow::anyhow!(e)),
		_ = sigint.recv() => tracing::info!("received SIGINT"),
		_ = sigterm.recv() => tracing::info!("received SIGTERM"),
	};

	tracing::info!("shutting down");

	shard.close(CloseFrame::NORMAL).await?;

	Ok(())
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
		self.cache.member(guild, self.id).map_or(false, |member| {
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

	/// Spawn a new task for each recieved [`Event`] from the gateway for processing.
	///
	/// Returns on fatal errors.
	async fn process(shard: &mut Shard) -> Result<Infallible, anyhow::Error> {
		loop {
			match shard.next_event().await {
				Ok(event) => {
					tokio::spawn(async move {
						let skip = match &event {
							// skip if permission did not change
							Event::ChannelUpdate(c) => {
								BOT.cache.channel(c.id).map_or(false, |cached| {
									cached.permission_overwrites != c.permission_overwrites
								})
							}
							// skip if permissions did not change
							Event::RoleUpdate(r) => {
								BOT.cache.role(r.role.id).map(|r| r.permissions)
									!= Some(r.role.permissions)
							}
							_ => false,
						};

						BOT.cache.update(&event);

						if skip {
							return;
						}

						match event {
							Event::ChannelUpdate(c) if BOT.auto_prune(c.guild_id.unwrap()) => {
								crate::prune::channel(c.id, c.guild_id.unwrap()).await;
							}
							Event::MemberUpdate(m) if BOT.auto_prune(m.guild_id) => {
								crate::prune::user(m.guild_id, m.user.id).await;
							}
							Event::RoleDelete(RoleDelete { guild_id, .. })
							| Event::RoleUpdate(RoleUpdate { guild_id, .. })
								if BOT.auto_prune(guild_id) =>
							{
								crate::prune::guild(guild_id).await;
							}
							Event::InteractionCreate(interaction) => match interaction.kind {
								InteractionType::ApplicationCommand => {
									crate::commands::interaction(interaction.0).await;
								}
								_ => tracing::warn!(?interaction, "unhandled"),
							},
							Event::Ready(r) => {
								tracing::info!(guilds = %r.guilds.len(), user = %r.user.name);
							}
							_ => {}
						}
					});
				}
				Err(error) if error.is_fatal() => {
					return Err(
						anyhow::anyhow!(error).context(format!("shard {} fatal error", shard.id()))
					);
				}
				Err(error) => {
					let _span = tracing::info_span!("shard", id = %shard.id()).entered();
					tracing::error!(error = &*anyhow::anyhow!(error));
					continue;
				}
			}
		}
	}

	/// Removes users, logging on error.
	///
	/// Returns the number of users removed.
	async fn remove(
		&self,
		guild: Id<GuildMarker>,
		users: impl IntoIterator<Item = Id<UserMarker>>,
	) -> u16 {
		join_all(users.into_iter().map(|user| async move {
			tracing::info!(user.id = %user, "kicking");
			match self
				.http
				.update_guild_member(guild, user)
				.channel_id(None)
				.await
			{
				Ok(_) => 1,
				Err(e) => {
					tracing::error!(error = &e as &dyn std::error::Error);
					0
				}
			}
		}))
		.await
		.iter()
		.sum()
	}
}

/// Initializes [`BOT`] and returns a shard.
///
/// # Panics
///
/// Panics if called multiple times.
#[tracing::instrument(skip_all)]
#[track_caller]
async fn init(args: cli::Args, token: String) -> Result<Shard, anyhow::Error> {
	let http = Client::new(token.clone());

	let application_id_fut =
		async { Ok::<_, anyhow::Error>(http.current_user_application().await?.model().await?.id) };

	if let Some(mode) = args.commands {
		let interaction = http.interaction(application_id_fut.await?);
		match mode {
			cli::Mode::Register => interaction.set_global_commands(&commands::get()).await?,
			cli::Mode::Unregister => interaction.set_global_commands(&[]).await?,
		};
		std::process::exit(0);
	}

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

	let id_fut = async { Ok(http.current_user().await?.model().await?.id) };

	let (application_id, id) = tokio::try_join!(application_id_fut, id_fut)?;

	tracing::info!(%application_id, user_id = %id);

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
