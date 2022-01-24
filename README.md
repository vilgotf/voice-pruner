# Voice pruner

Discord admin bot for monitoring and pruning voice channels.
Users in monitored channels without the `CONNECT` permission are auto pruned, assigning "no-auto-prune" disables this.

[Invite link] to an instance of this bot (latest released version).

## Commands
* list - Lists visible voice channels
  * type? - Only monitored / unmonitored voice channels
    * Monitored - List monitored voice channels
    * Unmonitored - List unmonitored voice channels
* is-monitored - Returns `true` if the channel is monitored
* prune - Prune voice channels
  * channel? - Prune this voice channel

## Permissions

### Server permissions
* `MOVE_MEMBERS` -  required for pruning

### Privileged intents
`GUILD_MEMBERS` to monitor removal of users roles.

## Self hosting
Bot token is read from systemd's credential storage (`token`) or the `TOKEN` env variable.
Use [voice-pruner.service] as a starting point.

[Invite link]: https://discord.com/api/oauth2/authorize?client_id=861223160905072640&permissions=16777216&scope=bot%20applications.commands
[voice-pruner.service]: voice-pruner.service
