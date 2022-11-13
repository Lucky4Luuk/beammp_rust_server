# BeamMP Protocol Documentation
WIP of course, but hope it helps others

## Communication flow
Upon client connection:
1. Authentication
2. Sync with client?

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
