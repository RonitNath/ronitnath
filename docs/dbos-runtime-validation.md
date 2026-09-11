# Local DBOS runtime validation

Measured 2026-09-11 with Docker 29.2.1 on the local Colima VM. The baseline
image was built from an exact archive of `edc1d367`; the DBOS image used the
checked-in tarballs and application workspace.

| Measure | baseline | DBOS runtime | change |
| --- | ---: | ---: | ---: |
| Image size | 137,490,695 B | 332,240,391 B | +141.6% |
| Ready startup | 2,574 ms | 5,895 ms | +129.0% |
| Idle container memory | 122.7 MiB | 513.3 MiB | +318.3% |
| `/healthz`, 20 requests, mean | 6.949 ms | 5.808 ms | -16.4% |
| `/healthz`, min / max | 4.227 / 13.180 ms | 3.554 / 19.412 ms | — |

The image-size increase is the cost of shipping production dependencies for a
custom Next server instead of the incompatible standalone trace. The process
sample attributed about 316 MiB RSS to HTTP, 283 MiB to the DBOS worker and 53
MiB to the supervisor; shared pages make their sum larger than the 513 MiB
container reading. Startup and memory are material regressions. Request latency
was within local measurement noise.

The final local checks cover:

- grid build, types, lint, 79 unit tests and the PostgreSQL DBOS crash/reconnect
  integration suite;
- application build, types, lint, 400 unit tests, including database suites in
  an isolated PostgreSQL instance;
- signup, persisted intent, worker delivery, worker `SIGKILL`, recovery with the
  same workflow version, link verification, incompatible-version isolation,
  permanent failure, expiry and two-replica behavior;
- the actual image with a PostgreSQL container: migrations, both-child
  readiness, worker-crash exit 1, one DBOS SDK installation, no sibling-worktree
  paths, a canonical-origin redirect, and final `docker stop` exit 0 in 352 ms
  without remaining processes;
- supervised production browser flows for password reset, resend, anonymous
  redirect and realtime delivery. Captured authentication and realtime frames
  were inspected for layout regressions.

Real provider acceptance, production failover and deployment were not tested.
