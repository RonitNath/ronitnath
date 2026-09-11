# Private personal calendar

`/u/[user]/calendar` consumes checked-in `pnpm pack` artifacts from
`vendor/grid-calendar`. It provides day, week, month, and agenda views; tasks and
manual work blocks; occurrence edits; foreground device timezone reporting; and
ICS preview, apply, and download. The existing owner-or-platform-operator subject
gate protects every query, command, and export. Operator views cannot report a
timezone for the subject.

## Reproduce

```sh
docker run --rm --name ronitnath-calendar-test \
  -e POSTGRES_USER=ronitnath -e POSTGRES_PASSWORD=ronitnath \
  -e POSTGRES_DB=ronitnath -p 55440:5432 -d postgres:17
DATABASE_URL=postgresql://ronitnath:ronitnath@127.0.0.1:55440/ronitnath pnpm db:migrate
DATABASE_URL=postgresql://ronitnath:ronitnath@127.0.0.1:55440/ronitnath \
  AUTH_SECRET=calendar-test-secret-calendar-test-secret \
  ID_KEY=00112233445566778899aabbccddeeff \
  PUBLIC_ORIGIN=http://127.0.0.1:3142 E2E_PORT=3142 \
  pnpm exec playwright test e2e/calendar.spec.ts --workers=1
```

The test creates synthetic owner, outsider, and operator accounts. It covers event
and task writes, work blocks, occurrence edits, ICS round trips, two-device timezone
ordering, denied cross-account page/export access, operator access, and desktop and
mobile screenshots. No production data, provider account, or outbound delivery is
used; mail is captured under `.mail`.
