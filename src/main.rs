//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

use std::{env, fs, ops::Deref};

use anyhow::Context;
use futures_util::{stream::FuturesUnordered, StreamExt};
use search::Search;
use tokio::signal::unix::{signal, SignalKind};
use tracing_subscriber::EnvFilter;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{cluster::Events, Cluster, EventTypeFlags, Intents};
use twilight_http::{client::InteractionClient, Client as HttpClient};
use twilight_model::{
	channel::ChannelType,
	guild::Permissions as TwilightPermissions,
	id::{
		marker::{ApplicationMarker, ChannelMarker, GuildMarker, UserMarker},
		Id,
	},
};

mod commands;
mod event;
mod interaction;
mod search;

struct Config {
	remove_commands: bool,
	token: String,
}

/// [`ChannelType`]s the bot operates on.
///
/// Must only be voice channels.
const MONITORED_CHANNEL_TYPES: ChannelType = ChannelType::GuildVoice;

/// Discord permissions for various actions.
struct Permissions;

impl Permissions {
	/// Required permission to monitor / manage channel.
	const ADMIN: TwilightPermissions = TwilightPermissions::MOVE_MEMBERS;
	/// Required permission to remain connected (avoid being kicked).
	const CONNECT: TwilightPermissions = TwilightPermissions::CONNECT;
}

pub struct Symbol;

impl Symbol {
	/// <https://emojipedia.org/warning/>
	pub const WARNING: &'static str = "\u{26A0}\u{FE0F}";
	pub const BULLET_POINT: &'static str = "\u{2022}";
}

fn app() -> clap::Command<'static> {
	clap::command!()
		.arg(clap::arg!(--"remove-commands" "Remove commands and exit").env("REMOVE_COMMANDS"))
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let matches = app().get_matches();

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
			tracing::debug!("using systemd credentials");
			path.push("/token");
			let mut string = fs::read_to_string(path)?;
			string.truncate(string.trim_end().len());
			string
		}
		_ => match env::var("TOKEN").context("missing Discord bot token") {
			Ok(token) => token,
			Err(e) => {
				tracing::error!(error = &*e as &dyn std::error::Error);
				std::process::exit(1);
			}
		},
	};

	span.exit();

	let config = Config {
		remove_commands: matches.is_present("remove-commands"),
		token,
	};

	let (bot, events) = Bot::new(config).await.context("startup failed")?;

	tokio::spawn(bot.connect());

	let mut sigint = signal(SignalKind::interrupt())?;
	let mut sigterm = signal(SignalKind::terminate())?;

	tokio::select! {
		_ = bot.process(events) => tracing::warn!("event stream unexpectedly exhausted"),
		_ = sigint.recv() => tracing::info!("received SIGINT"),
		_ = sigterm.recv() => tracing::info!("received SIGTERM"),
	};

	tracing::info!("shutting down");

	bot.shutdown();
	Ok(())
}

pub struct BotRef {
	pub application_id: Id<ApplicationMarker>,
	pub cache: InMemoryCache,
	pub cluster: Cluster,
	pub http: HttpClient,
	/// User ID of the bot
	pub id: Id<UserMarker>,
}

#[derive(Clone, Copy)]
pub struct Bot(&'static BotRef);

impl Bot {
	/// Creates a [`Bot`] and an [`Events`] stream from [`Config`].
	async fn new(config: Config) -> Result<(Self, Events), anyhow::Error> {
		let http = HttpClient::new(config.token.clone());

		let application_id = http
			.current_user_application()
			.exec()
			.await?
			.model()
			.await?
			.id;

		let interaction = http.interaction(application_id);

		// run before starting cluster
		if config.remove_commands {
			tracing::info!("removing slash commands");
			interaction.set_global_commands(&[]).exec().await?;

			std::process::exit(0);
		};

		tracing::info!("setting slash commands");
		interaction
			.set_global_commands(&commands::get())
			.exec()
			.await?;

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

		let (cluster, events) = {
			let intents = Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_VOICE_STATES;
			let events = EventTypeFlags::GUILDS ^ EventTypeFlags::CHANNEL_PINS_UPDATE
				| EventTypeFlags::GUILD_MEMBERS
				| EventTypeFlags::INTERACTION_CREATE
				| EventTypeFlags::READY
				| EventTypeFlags::GUILD_VOICE_STATES;
			Cluster::builder(config.token, intents)
				.event_types(events)
				.build()
				.await?
		};

		let id = http.current_user().exec().await?.model().await?.id;

		Ok((
			// Arc is a slower alternative since a new task is spawned for
			// incomming events (requiring clone).
			Self(Box::leak(Box::new(BotRef {
				application_id,
				cache,
				cluster,
				http,
				id,
			}))),
			events,
		))
	}

	/// Connects to the Discord gateway.
	async fn connect(self) {
		self.cluster.up().await;
		tracing::info!("all shards connected");
	}

	fn as_interaction(&self) -> InteractionClient {
		self.http.interaction(self.application_id)
	}

	/// Returns `true` if the voice channel is monitored.
	fn is_monitored(self, channel_id: Id<ChannelMarker>) -> bool {
		self.cache
			.permissions()
			.in_channel(self.id, channel_id)
			.expect("resources are available")
			.contains(Permissions::ADMIN)
	}

	/// Listens and processes each [`Event`] from the [`Events`] stream in a new task.
	///
	/// [`Event`]: twilight_model::gateway::event::Event
	async fn process(self, mut events: Events) {
		tracing::info!("started event stream loop");
		while let Some((_, event)) = events.next().await {
			tokio::spawn(event::process(self, event));
		}
	}

	/// Removes users, logging on error.
	///
	/// Returns the number of users removed.
	async fn remove(
		self,
		guild_id: Id<GuildMarker>,
		users: impl Iterator<Item = Id<UserMarker>>,
	) -> usize {
		async fn remove(bot: Bot, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>) {
			tracing::info!(user.id = %user_id, "kicking");
			if let Err(e) = bot
				.http
				.update_guild_member(guild_id, user_id)
				.channel_id(None)
				.exec()
				.await
			{
				tracing::error!(error = &e as &dyn std::error::Error);
			}
		}

		let mut futures = users
			.map(|user_id| remove(self, guild_id, user_id))
			.collect::<FuturesUnordered<_>>();
		let mut processed = 0;
		while futures.next().await.is_some() {
			processed += 1;
		}
		processed
	}

	/// Conveniant constructor for [`Search`].
	fn search(self, guild_id: Id<GuildMarker>) -> Search {
		Search::new(self, guild_id)
	}

	fn shutdown(self) {
		self.cluster.down();
	}
}

impl Deref for Bot {
	type Target = BotRef;

	fn deref(&self) -> &Self::Target {
		self.0
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn verify_app() {
		super::app().debug_assert()
	}
}
