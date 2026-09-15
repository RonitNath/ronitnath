# Privacy and encrypted event content

ronitnath.com is the first production pilot for Grid personal-content privacy.
The application uses the **managed (Level 2)** profile for event copy. This is
a server-recoverable profile: PostgreSQL, WAL, replicas, dumps and storage
backups receive authenticated ciphertext, while the authorized application can
ask AWS KMS to unwrap a user's root key.

## Current claim

After an event row has `privacy_revision > 0`, these values are encrypted
together before persistence:

- title, summary and body;
- start/end instants and timezone;
- venue name and address;
- capacity, colour, poster URL and guest-list visibility.

The clear routing record contains the event ID, opaque host ID relationship,
an opaque random slug, publication timestamp, calendar sequence, privacy revision and ordinary
row timestamps. Object size, user/event counts, traffic timing and access
patterns remain observable. A public event page necessarily reveals its
decrypted content to guests who hold an authorized event link.

RSVP responses and plus-one counts are operational state. RSVP notes, uploaded
photos, invitee/contact data, audit history and records from other product
features remain Level 1 in this pilot. Do not describe ronitnath.com as wholly
encrypted or end-to-end encrypted. Legacy title-derived slugs and historical plaintext can remain in WAL,
replicas, backups, exports and prior telemetry until each retention period
expires.

## Authority and keys

Each person has one random managed root key for this application and key epoch.
AWS KMS wraps that root key. Each event revision then receives a fresh random
data key; the root key wraps it, and it encrypts the grouped event content with
AES-256-GCM. The authenticated envelope binds application, owner, event,
schema, revision and key epoch.

Production uses the customer-managed KMS key
`alias/isoastra/ronitnath/privacy/prod` in `us-west-1`. SFO and NYC exchange
separate fleet-CA certificates for one-hour IAM Roles Anywhere credentials.
The role can use only this key and only the `ronitnath-events` encryption
context. The web process uses the non-owner PostgreSQL role
`ronitnath_runtime`; the schema owner is reserved for migrations. Platform
operator status does not authorize opening another person's protected event.

KMS and the running server remain inside the Level 2 trust boundary. An
administrator who can change the deployed server, assume the KMS role, or
control an already-authorized process can disclose content. Level 3 is the
client-only mode for protection from operators.

## Migration and rollback

`0016_event_privacy_expand` adds the privacy tables and marks existing event
rows with revision zero. New writes create only encrypted event content.
`pnpm migrate:event-privacy` converts at most
`PRIVACY_MIGRATION_LIMIT` rows per run, locks and revision-checks each row,
and reports its cursor and remaining count. A legacy reader is available only
for revision-zero rows. Decryption, context or format failures never fall back
to plaintext.

The expand migration is compatible with the previous runtime only while its
old event columns remain present. A later contract release may remove them
only after the probe reports no revision-zero rows and the retention
disclosure has been accepted. A Level 3 scope cannot be downgraded in place.

## Observability

Umami is loaded only on the public landing and About pages. Event, auth,
account, user and organization surfaces load no Umami script. The qualified
restricted HyperDX configuration disables replay, console collection,
advanced network capture and visible text collection; no HyperDX SDK is loaded
today. Any future vendor initialization must go through
`@isoastra/privacy-observability` and its allowlisted operational event API.
Event titles, slugs, query strings, request bodies, SQL parameters, exceptions
and decrypted content are forbidden telemetry fields.

Run `pnpm qualify:event-privacy` against a migrated database to create an
isolated canary transaction, verify that SQL contains no canary plaintext,
round-trip the event, deny the operator bypass, and roll the fixture back.

## Production qualification

The Level 2 claim became active on 2026-09-14 after release
`f35a1594-f6a7-47a1-b1d2-f5cc93c95da1` deployed source
`bcf54a7278ed8df40ea32c0a90c0e138c30a0e43` and runtime image
`sha256:15a11e34b583e9e677d7c22807993f83a9e0363c98c65ab523ddca592e561cf1`
to SFO and NYC. The migration image was
`sha256:db2d47677bdff16487f944da9ca4bffe3cc1984d485e2ca906b43f195491d22f`.

Production qualification established that:

- the one legacy event was migrated and the completion probe reported zero
  revision-zero rows;
- all classified legacy columns on protected event rows were null;
- the deployed migration image completed a real KMS encrypt/decrypt canary,
  found no canary plaintext in its visible SQL rows, rejected an operator
  bypass, and rolled the transaction back;
- the migrated public event decrypted successfully through each replica;
- both web containers run as the non-owner `ronitnath_runtime` role and both
  IAM Roles Anywhere credential sidecars use separate host certificates;
- the public landing page loads Umami while event pages load neither Umami nor
  HyperDX, and the canary string was absent from the external event response.

This qualification supports only the covered event fields and current release.
It does not erase earlier plaintext copies or upgrade the uncovered fields
listed above.



