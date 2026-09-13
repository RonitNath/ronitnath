# Local DBOS runtime validation

Measured with Docker 29.2.1 on the local Colima VM. The baseline was measured
2026-09-11 from an exact archive of `edc1d367`; the final DBOS image was
measured 2026-09-13 by `scripts/test-container-local.sh` from the checked-in
tarballs and application workspace. Host latency measurements across dates are
directional because machine load was not controlled.

| Measure                       |      baseline |  DBOS runtime |   change |
| ----------------------------- | ------------: | ------------: | -------: |
| Image size                    | 137,490,695 B | 328,107,761 B | +138.6% |
| Ready startup                 |      2,574 ms |      2,456 ms |   -4.6% |
| Idle container memory         |     122.7 MiB |     512.1 MiB | +317.4% |
| `/healthz`, 20 requests, mean |      6.949 ms |      4.440 ms | -36.1%* |

The image-size increase is the cost of shipping production dependencies for a
custom Next server instead of the incompatible standalone trace. Removing curl
and runtime source maps reduced the earlier DBOS image by 4,132,630 bytes
(1.3%). The earlier process sample attributed about 316 MiB RSS to HTTP, 283 MiB
to the DBOS worker and 53 MiB to the supervisor; shared pages make their sum
larger than the container reading. Memory remains a material regression.
*Latency runs occurred on different days, so that delta is directional rather
than an application-only measurement.

The final local checks cover:

- grid formal checks, all builds, types, lint, 209 unit tests and its PostgreSQL
  accounting, inference, calendar and DBOS crash/reconnect suites;
- application build, types, lint and 378 unit tests, with database suites run
  inside the isolated runtime harness rather than silently skipped;
- signup, persisted intent, worker delivery, worker `SIGKILL`, recovery with the
  same workflow version, link verification, incompatible-version isolation,
  permanent failure, expiry and two-replica behavior;
- the actual image with a PostgreSQL container: migrations, both-child
  readiness, worker-crash exit 1, one DBOS SDK installation, no sibling-worktree
  paths, a canonical-origin redirect, and final `docker stop` exit 0 within one second
  without remaining processes;
- 62 supervised production browser scenarios with a fresh server, including
  auth, documents, publication, RSVP, presence, configuration and sky assets;
- development worker-only replacement while the Next child remains supervised.

Real provider acceptance, production failover and deployment were not tested.
