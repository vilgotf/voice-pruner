# Voice pruner

Discord admin bot to monitor and prune voice channels. Supports auto pruning users without the `CONNECT` permission. Assigning the bot a role named "no-auto-prune" disables auto pruning.

[Invite link] to an instance of this bot running the latest released version.

## Commands

* `/is-monitored` - Whether the voice channel is monitored
  * channel - Voice channel to check
* `/list` - Lists visible voice channels
  * type? - Type of visible channels to list
    * Monitored - Lists visible monitored voice channels
    * Unmonitored - Lists visible unmonitored voice channels
* `/prune` - Prune voice channels
  * channel? - Prune only this voice channel
  * role? - Prune only this role

## Required bot permissions

* `MOVE_MEMBERS` -  Required for pruning

## Self hosting

A statically compiled binary of the bot may easily be created by running `cargo build --release` (the `--release` flags optimizes the binary). It's possible, through feature flags, to configure the TLS' certificate root store, defaulting to `native-roots`. Available feature flags:

* `native-roots` - The platform's certificate root store
* `webpki-roots` - Mozilla's certificate root store.

The bot tries to, on start-up, read its token from systemd's [credential storage] (a credential named `token`) or the `TOKEN` environment variable. Use the [voice-pruner.service](voice-pruner.service) unit as a starting point for running the bot with systemd.

### Privileged intents

The bot requires the `GUILD_MEMBERS` priviledged intent to monitor the updates of users' roles, but does otherwise function without it.

[credential storage]: https://systemd.io/CREDENTIALS/
[Invite link]: https://discord.com/api/oauth2/authorize?client_id=861223160905072640&permissions=16777216&scope=bot%20applications.commands
