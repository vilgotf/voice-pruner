# Voice pruner

A very simple discord bot which monitors voice channels and on channel, member and role update removes members lacking `CONNECT` permission.

It also includes two slash commands:
* About - information about the bot
* List
  * Monitored - lists monitored channels
  * Unmonitored - lists unmonitored channels

I host a free to use instance of this bot [here][bot_invite_link].

## Permissions
The bot monitors all channels it has the `MOVE_MEMBERS` permission in.
Use channel overwrites for a denylist or remove the `MOVE_MEMBERS` global permission for a allowlist.

### Privileged intents
Requires the `GUILD_MEMBERS` intent for the removal of members' roles.

## Use case
This bot is primarily meant as a template or showcase of twilight, but it could be combined with some permission manager to immediately remove from a voice channel on permission changes.

## Self hosting
A container image is availiable: [github.io/vilgotf/voice-pruner][container_link] (built from [Containerfile](Containerfile))

[bot_invite_link]: https://discord.com/api/oauth2/authorize?client_id=861223160905072640&permissions=16777216&scope=bot%20applications.commands
[container_link]: aoea