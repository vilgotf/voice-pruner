//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

use std::{env, fs, ops::Deref};

use anyhow::Context as _;
use futures_util::{future::join_all, StreamExt};
use once_cell::sync::OnceCell;
use tokio::signal::unix::{signal, SignalKind};
use tracing_subscriber::EnvFilter;
use twilight_cache_inmemory::{model::CachedVoiceState, InMemoryCache, ResourceType};
use twilight_gateway::{shard::Events, EventTypeFlags, Intents, Shard};
use twilight_http::Client;
use twilight_model::{
	channel::ChannelType,
	guild::Permissions as TwilightPermissions,
	id::{
		marker::{ApplicationMarker, ChannelMarker, GuildMarker, UserMarker},
		Id,
	},
};

use self::cli::Mode;

mod cli;
mod commands;
mod event;

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

struct Config {
	update_commands: Option<Mode>,
	token: String,
}

/// [`ChannelType`]s the bot operates on.
///
/// Must only be voice channels.
const MONITORED_CHANNEL_TYPES: [ChannelType; 2] =
	[ChannelType::GuildVoice, ChannelType::GuildStageVoice];

/// Discord permissions for various actions.
struct Permissions;

impl Permissions {
	/// Required permission to monitor / manage channel.
	const ADMIN: TwilightPermissions = TwilightPermissions::MOVE_MEMBERS;
	/// Required permission to remain connected (avoid being kicked).
	const CONNECT: TwilightPermissions = TwilightPermissions::CONNECT;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
	let args = cli::Args::parse();

	// prefer RUST_LOG with `info` as fallback.
	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
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

	let config = Config {
		update_commands: args.commands,
		token,
	};

	let events = init(config).await.context("startup errord")?;

	// the gateway takes a while to be fully ready (all shards connected), so blocking delays event
	// processing needlessly
	tokio::spawn(BOT.connect());

	let mut sigint = signal(SignalKind::interrupt())?;
	let mut sigterm = signal(SignalKind::terminate())?;

	tokio::select! {
		_ = BOT.process(events) => tracing::warn!("event stream unexpectedly exhausted"),
		_ = sigint.recv() => tracing::info!("received SIGINT"),
		_ = sigterm.recv() => tracing::info!("received SIGTERM"),
	};

	tracing::info!("shutting down");

	BOT.shutdown();
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
	gateway: Shard,
	http: Client,
	/// User ID of the bot
	id: Id<UserMarker>,
}

impl BotRef {
	/// Connects to the Discord gateway.
	#[tracing::instrument(skip(self))]
	async fn connect(&self) {
		match self.gateway.start().await {
			Ok(()) => tracing::info!("gateway ready"),
			Err(e) => tracing::error!(error = &e as &dyn std::error::Error),
		}
	}

	/// Returns `true` if the voice channel is monitored.
	fn is_monitored(&self, channel: Id<ChannelMarker>) -> bool {
		self.cache
			.permissions()
			.in_channel(BOT.id, channel)
			.expect("resources are available")
			.contains(Permissions::ADMIN)
	}

	/// Spawns a new task for each recieved [`Event`] from the [`Events`] stream for processing.
	///
	/// [`Event`]: twilight_model::gateway::event::Event
	async fn process(&self, mut events: Events) {
		tracing::info!("started gateway event stream loop");
		while let Some(event) = events.next().await {
			tokio::spawn(event::process(event));
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
				.exec()
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

	fn shutdown(&self) {
		self.gateway.shutdown();
	}
}

/// Search for users in voice channels that should be removed.
struct Search;

impl Search {
	/// Returns `true` if the user is permitted to be in the voice channel.
	fn is_permitted(state: &CachedVoiceState) -> bool {
		BOT.cache
			.permissions()
			.in_channel(state.user_id(), state.channel_id())
			.expect("resources are available")
			.contains(Permissions::CONNECT)
	}

	/// Returns a list of users to be removed.
	fn channel(channel: Id<ChannelMarker>) -> Vec<Id<UserMarker>> {
		match BOT.cache.voice_channel_states(channel) {
			Some(state) => state
				.into_iter()
				.filter_map(|state| (!Self::is_permitted(&state)).then(|| state.user_id()))
				.collect(),
			None => Vec::new(),
		}
	}

	/// Returns a list of users to be removed.
	fn guild(guild: Id<GuildMarker>) -> Vec<Id<UserMarker>> {
		let channels = BOT.cache.guild_channels(guild).expect("cached");

		channels
			.iter()
			.filter_map(|&id| {
				(MONITORED_CHANNEL_TYPES.contains(&BOT.cache.channel(id).expect("cached").kind)
					&& BOT.is_monitored(id))
				.then(|| Self::channel(id))
			})
			.flatten()
			.collect()
	}

	/// Returns `true` if the user should be removed.
	fn user(guild: Id<GuildMarker>, user: Id<UserMarker>) -> bool {
		matches!(BOT.cache.voice_state(user, guild), Some(s) if !Self::is_permitted(&s))
	}
}

/// Initialize [`BOT`] and return the gateway event stream.
///
/// # Panics
///
/// Panics if called multiple times.
#[tracing::instrument(level = "info", name = "startup", skip(config))]
#[track_caller]
async fn init(config: Config) -> Result<Events, anyhow::Error> {
	let http = Client::new(config.token.clone());

	let application_id_fut = async {
		Ok::<_, anyhow::Error>(
			http.current_user_application()
				.exec()
				.await?
				.model()
				.await?
				.id,
		)
	};

	if let Some(commands) = config.update_commands {
		let interaction = http.interaction(application_id_fut.await?);
		match commands {
			Mode::Register => interaction.set_global_commands(&commands::get()).exec(),
			Mode::Unregister => interaction.set_global_commands(&[]).exec(),
		}
		.await?;
		std::process::exit(0);
	}

	let cache = {
		let resource_types = ResourceType::CHANNEL
			| ResourceType::GUILD
			| ResourceType::MEMBER
			| ResourceType::ROLE
			| ResourceType::VOICE_STATE;
		InMemoryCache::builder()
			.resource_types(resource_types)
			.build()
	};

	let (gateway, events) = {
		let intents = Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_VOICE_STATES;
		let events = EventTypeFlags::GUILDS ^ EventTypeFlags::CHANNEL_PINS_UPDATE
			| EventTypeFlags::GUILD_MEMBERS
			| EventTypeFlags::INTERACTION_CREATE
			| EventTypeFlags::READY
			| EventTypeFlags::GUILD_VOICE_STATES;
		Shard::builder(config.token, intents)
			.event_types(events)
			.build()
	};

	let id_fut = async { Ok(http.current_user().exec().await?.model().await?.id) };

	let (application_id, id) = tokio::try_join!(application_id_fut, id_fut)?;

	tracing::info!(%application_id, user_id = %id);

	BOT.0
		.set(BotRef {
			application_id,
			cache,
			gateway,
			http,
			id,
		})
		.expect("only called once");

	Ok(events)
}
