# P2 — products at runtime

`three-node-transcript.txt` is B5's binding acceptance, run on this machine
against `tools/cluster.sh` (three real voters, release build, ports 3161-3163):
the product's route is 404 on all three, `enable-product` is posted to **node 1
only**, all three answer 200 within 2 s, `pgrep` shows the same three pids
before and after, and a `disable-product` on **node 3** returns all three to
404. Nothing restarted.

The screenshots this directory is also meant to carry — `/platform/products`
at 1440 and 390, both themes — are **not here yet**: the session was paused
before the visual pass. They are the one outstanding item of the leg.
