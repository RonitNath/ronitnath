---
id: EV-004
date: 2026-08-03
provenance: ground-truth
source: owner ruling after benchmark (platform-scope.md §ID discipline)
---
ID discipline ruled: encrypted ids (AES-128-ECB over table-tag‖rowid, 22-char base64url) are the default wire id; stored short codes for human-copyable handles; uuidv4 only for deliberate external interop. Criterion: trivial CPU at 10k/s justified deleting the uuid+LRU subsystem. Accepted trade-offs: key leak → retroactive enumerability; rotation breaks shared URLs. Sessions/capability links stay random bearer tokens.
