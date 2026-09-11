/* The identity core, exactly as better-auth 1.7.2 defines it.
 *
 * The shapes here are not a design: they are `getAuthTables()` for core +
 * genericOAuth + organization, transcribed into Drizzle so that drizzle-kit
 * owns the migration and the library owns the columns. Read the library's
 * table definitions before changing anything in this file; a column this app
 * invents is a column better-auth will not write.
 *
 * They live in their own Postgres schema, `auth`, so that `person`, `event`
 * and the rest of the kernel model stay recognisably ours and a future
 * `@isoastra/identity` can be dropped in over the same namespace
 * (fleet-conventions §2).
 *
 * Four columns are ours, declared to better-auth as additional fields so it
 * selects them but never lets a client set them: `user.role`,
 * `user.disabled_at`, `session.acting_operator_id`, `session.reauthenticated_at`.
 * The last two are what makes sign-in-as and the ten-minute re-authentication
 * window session facts rather than a second cookie. */

import { bigint, boolean, integer, pgSchema, text, timestamp } from 'drizzle-orm/pg-core';

export const authSchema = pgSchema('auth');

export const user = authSchema.table('user', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  email: text('email').notNull().unique(),
  emailVerified: boolean('email_verified').notNull().default(false),
  image: text('image'),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
  /* ours */
  role: text('role'),
  disabledAt: timestamp('disabled_at', { withTimezone: true }),
});

export const session = authSchema.table('session', {
  id: text('id').primaryKey(),
  expiresAt: timestamp('expires_at', { withTimezone: true }).notNull(),
  token: text('token').notNull().unique(),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
  ipAddress: text('ip_address'),
  userAgent: text('user_agent'),
  userId: text('user_id')
    .notNull()
    .references(() => user.id, { onDelete: 'cascade' }),
  activeOrganizationId: text('active_organization_id'),
  /* ours */
  actingOperatorId: text('acting_operator_id'),
  reauthenticatedAt: timestamp('reauthenticated_at', { withTimezone: true }),
});

/* `issuer` is not optional and not ours to drop: 1.7.0 scoped an account by
 * the issuer that vouched for it, which is why the version is pinned and the
 * changelog is read before it moves. */
export const account = authSchema.table('account', {
  id: text('id').primaryKey(),
  issuer: text('issuer').notNull(),
  accountId: text('account_id').notNull(),
  providerId: text('provider_id').notNull(),
  userId: text('user_id')
    .notNull()
    .references(() => user.id, { onDelete: 'cascade' }),
  accessToken: text('access_token'),
  refreshToken: text('refresh_token'),
  idToken: text('id_token'),
  accessTokenExpiresAt: timestamp('access_token_expires_at', { withTimezone: true }),
  refreshTokenExpiresAt: timestamp('refresh_token_expires_at', { withTimezone: true }),
  scope: text('scope'),
  /* The PHC string. A local identity's existing argon2id hash is copied in
   * here verbatim by scripts/migrate-identity.ts and verified by the override
   * in features/auth/auth.ts, which reads its parameters from the string. */
  password: text('password'),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
});

export const verification = authSchema.table('verification', {
  id: text('id').primaryKey(),
  identifier: text('identifier').notNull(),
  value: text('value').notNull(),
  expiresAt: timestamp('expires_at', { withTimezone: true }).notNull(),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp('updated_at', { withTimezone: true }).notNull().defaultNow(),
});

/* The organization plugin's three tables. This site's sharing model is its own
 * `relation` table and stays that way (contracts.md: org-plugin adoption for
 * rn ReBAC is out of scope); the tables exist so that the identity core is the
 * same shape on every site in the fleet and adopting them later is a data
 * move, not a migration. */
export const organization = authSchema.table('organization', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  slug: text('slug').notNull().unique(),
  logo: text('logo'),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  metadata: text('metadata'),
});

export const member = authSchema.table('member', {
  id: text('id').primaryKey(),
  organizationId: text('organization_id')
    .notNull()
    .references(() => organization.id, { onDelete: 'cascade' }),
  userId: text('user_id')
    .notNull()
    .references(() => user.id, { onDelete: 'cascade' }),
  role: text('role').notNull().default('member'),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
});

export const invitation = authSchema.table('invitation', {
  id: text('id').primaryKey(),
  organizationId: text('organization_id')
    .notNull()
    .references(() => organization.id, { onDelete: 'cascade' }),
  email: text('email').notNull(),
  role: text('role'),
  status: text('status').notNull().default('pending'),
  expiresAt: timestamp('expires_at', { withTimezone: true }).notNull(),
  createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  inviterId: text('inviter_id')
    .notNull()
    .references(() => user.id, { onDelete: 'cascade' }),
});

/* Rate limits, counted in the database because there are two replicas and a
 * per-process counter is a limit of twice what it says. better-auth only ever
 * applies these to its own client-initiated endpoints — a direct `auth.api`
 * call bypasses them — and only in production, which is exactly why a missing
 * table shows up in the image rather than on a development machine.
 *
 * `last_request` is a Unix millisecond count, not a timestamp: that is the
 * shape the library reads and writes, and storing it as anything else means it
 * cannot. */
export const rateLimit = authSchema.table('rateLimit', {
  id: text('id').primaryKey(),
  key: text('key').notNull(),
  count: integer('count').notNull(),
  lastRequest: bigint('last_request', { mode: 'number' }).notNull(),
});
