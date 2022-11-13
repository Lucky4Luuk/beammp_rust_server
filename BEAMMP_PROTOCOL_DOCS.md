# BeamMP Protocol Documentation
WIP of course, but hope it helps others

## Communication flow
Upon client connection:
1. Authentication
2. Sync with client?

### Authentication
1. S->C - Client sends the server a single byte. The BeamMP source checks for 3 letters, but I only saw C
2. S->C - Client sends a packet containing the client version
3. C->S - Server sends the server a regular packet containing only 1 byte: `S` (the ascii character)
4. S->C - Client sends the server its public key

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
| `P` | S->C | No idea, official server seems to just respond with "P" |
