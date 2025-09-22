# Application Protocol

Communication between client and server consists of both WebSocket messages and UDP packets. The client first establishes a WebSocket connection, which the server uses to send important but less frequent updates. The client and server then trade UDP packets with the frame-by-frame data to sync state.

# WebSocket Scheme

All messages are in JSON format, with a `type` field to indicate its purpose, as well as any other fields specific to that message type. For example, a `Connected` packet (described below) might look like this:

```json
{
  "type": "Connected",
  "id": 1188764670,
  "players": [
    578885259,
    846393187,
    3840177655
  ]
}
```

## Client to Server Messages

### `Connect`

The `Connect` message is sent after the WebSocket connection is established. This message has no additional fields, but may be updated in the future to validate the connection with a password or set a name for the player.

## Server to Client Messages

### `Connected`

The `Connected` message is sent in response to the `Connect` message to signal a successful connection.

| Field | Type | Description |
| --- | --- | --- |
| `id` | unsigned 32-bit integer | The id assigned to the player |
| `players` | array of unsigned 32-bit integers | The ids of all other currently connected players |

### `PlayerJoined`

The `PlayerJoined` message is sent when a new player connects to the server.

| Field | Type | Description |
| --- | --- | --- |
| `id` | unsigned 32-bit integer | The id of the player that just joined |

### `PlayerLeft`

The `PlayerLeft` message is sent when a connected player disconnects from the server.

| Field | Type | Description |
| --- | --- | --- |
| `id` | unsigned 32-bit integer | The id of the player that just left |

# UDP Scheme

## Client to Server Packets

After establishing a WebSocket connection and receiving a `Connected` packet, clients send a UDP packet every frame to inform the server of their current state. The update is 52 bytes long and has the following format:

* Update number (unsigned 32-bit integer, 4 bytes): since UDP packets can arrive out of order, this is used by the server to determine whether to accept an update. A value of 0 is used by the server to indicate an update has not arrived yet for this player, so the first update the client sends has an update number of 1. Each time the client sends a packet, it increments the update number by 1.
* Player id (unsigned 32-bit integer, 4 bytes): the id of the player that was received in the `Connected` packet. The server rejects the packet if the id does not match a connected player.
* Zone (unsigned 64-bit integer, 8 bytes): a hash of the zone the player is in. The hash is calculated client-side and used by the client to determine whether another player is in the same zone.
* Transform: 
  * Location: the location component of the player's transform, represented by three 32-bit floating point numbers.
    * x coordinate (32-bit IEEE 754 floating point number, 4 bytes)
    * y coordinate (32-bit IEEE 754 floating point number, 4 bytes)
    * z coordinate (32-bit IEEE 754 floating point number, 4 bytes)
  * Rotation: the rotation component of the player's transform. This has the same representation as location (12 bytes).
  * Scale: the scale component of the player's transform. This has the same representation as location (12 bytes).

Notes:

* Each number in the update is in big endian format.
* After update number and player id, the server doesn't do anything with the data except store it and pass it along to other players.

## Server to Client Packets

Once an update is accepted by the server, the server sends one or more UDP packets with the current state of other connected players. An update is `4 + 48 * num_updates` bytes long and has the following format:

* Update number (unsigned 32-bit integer, 4 bytes): when the server responds to an accepted update, it uses the same update number in the response.
* Player updates (48 bytes each): Each contiguous set of 48 bytes after the first 4 contain the state for another connected player. These 48 bytes are in the same format as the player update, from player id to transform.

Notes:

* `num_updates` will always be between 1 and 10, inclusive. So an update will have minimum length 52 and maximum length 484, and the length of an update mod 48 will always be 4.
* If the server has to send more than 10 players' worth of updates, it will send multiple packets.

The client keeps track of the highest update number received for each connected player. Similar to the server, the client only accepts an update for a player if the update number is greater than the one stored for that player. The update number in the packet applies to each player in the update.
