# BeamMP Protocol Documentation
WIP of course, but hope it helps others

## Communication flow
Upon client connection:
1. Authentication
2. Sync with client?

### Authentication
1. C->S - Client sends the server a single byte. The BeamMP source checks for 3 letters, but I only saw C
2. C->S - Client sends a packet containing the client version
3. S->C - Server sends the server a regular packet containing only 1 byte: `S` (the ascii character)
4. C->S - Client sends the server its public key
5. S->C - Server sends the client their assigned server ID

### Syncing resources
1. C->S - Client sends the server a packet with the code `SR`
2. S->C - Server sends the client a packet containing a filelist containing the name and size of files to download, or "-" if it's empty

## Packets
All packets start with a single character, denoted as "Packet Code" or simply "Code" down below.
Example:
| Code | Dir | Explanation |
| ---- | --- | ----------- |
| `C` | S->C | Client version during authentication only |

### Authentication
| Code | Dir | Explanation |
| ---- | --- | ----------- |
| `C` | S->C | Client version |
| `D` | S->C | Start download phase |
| `P` | S->C | Send client their assigned ID |
