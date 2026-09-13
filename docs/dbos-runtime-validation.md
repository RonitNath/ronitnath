# Local DBOS runtime validation

Measured with Docker 29.2.1 on the local Colima VM. The baseline was measured
2026-09-11 from an exact archive of `edc1d367`; the final DBOS image was
measured 2026-09-13 by `scripts/test-container-local.sh` from the checked-in
tarballs and application workspace. The notification column uses the final
`@isoastra/fleet-notifications` artifact. Host measurements are directional
because machine load was not controlled.

| Measure                       |      baseline |  DBOS runtime | notifications | vs. DBOS |
| ----------------------------- | ------------: | ------------: | ------------: | -------: |
| Image size                    | 137,490,695 B | 328,107,761 B | 328,378,345 B |    +0.1% |
| Ready startup                 |      2,574 ms |      2,456 ms |      3,824 ms |  +55.7%* |
| Idle container memory         |     122.7 MiB |     512.1 MiB |     514.9 MiB |   +0.5%* |
| `/healthz`, 20 requests, mean |      6.949 ms |      4.440 ms |      6.266 ms |  +41.1%* |

The image-size increase is the cost of shipping production dependencies for a
custom Next server instead of the incompatible standalone trace. Removing curl
and runtime source maps reduced the earlier DBOS image by 4,132,630 bytes
(1.3%). The earlier process sample attributed about 316 MiB RSS to HTTP, 283 MiB
to the DBOS worker and 53 MiB to the supervisor; shared pages make their sum
larger than the container reading. Memory remains a material regression.
*Latency samples occurred under different host load, so that delta is directional rather
than an application-only measurement.

The final local checks cover:

- grid formal checks, all builds, types, lint, 295 unit tests and its PostgreSQL
  accounting, inference, calendar and DBOS crash/reconnect suites;
- application build, types, lint and 378 unit tests, with database suites run
  inside the isolated runtime harness rather than silently skipped;
- signup, shared persisted intent/outbox, worker delivery, worker `SIGKILL`,
  recovery with the same key and workflow version, link verification,
  incompatible-version isolation, permanent failure, legacy-intent handoff,
  expiry and two-replica behavior;
- the actual image with a PostgreSQL container: migrations, both-child
  readiness, worker-crash exit 1, one DBOS SDK installation, no sibling-worktree
  paths, a canonical-origin redirect, and final `docker stop` exit 0 within one second
  without remaining processes;
- 61 of 62 supervised production browser scenarios passed in one fresh-server
  run, including every auth, document, publication, RSVP, presence and
  configuration scenario. The moving-sky sampling assertion failed once due to
  a callout substitution and passed on an immediate fresh isolated rerun;
- development worker-only replacement while the Next child remains supervised.

Real provider acceptance, production failover and deployment were not tested.
