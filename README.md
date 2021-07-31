# Voice pruner

Discord bot that monitors voice channels it has the `MOVE_MEMBERS` permission in.
Users in monitored channels are pruned on permission updates (unless the bot is assigned the "no-auto-prune" role) and the bot disconnects users without the `CONNECT` permission.

I host a free to use instance of this bot [here][bot_invite_link].

## Commands
(Only available to users with the `MOVE_MEMBERS` permission.)
* List
  * Monitored - lists monitored channels
    * Channel - Returns `true` if the channel is monitored
  * Unmonitored - lists unmonitored channels
    * Channel - Returns `true` if the channel is unmonitored
* Prune - Manually prune voice channels
  * Channel - Only prune this voice channel
  * Role - Only prune users with this role

## Permissions
The bot monitors all channels it has the `MOVE_MEMBERS` permission in.

### Privileged intents
Requires the `GUILD_MEMBERS` intent for monitoring the removal of members roles.

## Self hosting
Compiled binaries can be found in the actions page, under artifacts.

### Systemd (prefered)
The bot first tries to use systemd's credential storage (v247+) to read the bot token and falls back to using the `TOKEN` environment variable.
For an example of how to use credintial storage, look at the provided [systemd service file](voice-pruner.service).

### Container
A container image is availiable at [`ghcr.io/vilgotf/voice-pruner`][container].
It is built from scratch with the musl binary (see [publish.yml](.github/workflows/publish.yml)).

[bot_invite_link]: https://discord.com/api/oauth2/authorize?client_id=861223160905072640&permissions=16777216&scope=bot%20applications.commands
[container]: https://github.com/users/vilgotf/packages/container/package/voice-pruner