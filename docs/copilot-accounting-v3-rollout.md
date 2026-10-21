# Copilot accounting v3: integration and maintainer rollout

PR #569 is integrated with upstream `4cb82c70` (including v1.4.7). This is not a
release or a record of a production deployment. Private OpenCode extension
adapters are not included.

## Why this still includes accounting changes

VS Code `customendpoint/*` chat usage proves aggregate prompt and completion
counts, but not the prompt's fresh/cache split. The parser stores that prompt
count in the disjoint `unclassified_input_tokens` column, includes it in token
volume, and never assigns it uncached-input pricing. Costs are partial:
`total_cost_usd: null`, with a provable subtotal in `known_cost_usd`.

A parser-only change that feeds the existing uncached/cache columns would
invent billing semantics. A separate discovery-only PR could discover these
files, but would not by itself deliver safe usage accounting. This integration
retains the full accounting path requested by the contributor; the maintainer
still owns the scope/product decision.

## Decision pending, not obsolete

On 2026-10-21, upstream `4cb82c70` still has no VS Code `customendpoint/*`
chat-session discovery/parser path. Existing Copilot OTEL/session-store readers
do not substitute for a persisted VS Code workspace chat session. The feature
gap therefore remains, but that does not make the full schema change a routine
maintenance merge.

If the maintainer wants smaller reviews, the first delivery can contain just
discovery, a raw numeric usage reader and isolated tests, without importing
those rows into the shared billing/upload queue. A later delivery must approve
the unknown-input/partial-cost contract and wire the real consumers. The first
delivery alone would NOT fix visible usage totals. Never replace unknown input
with uncached input, or silently omit it, just to reduce the diff. The existing
full implementation is retained until this scope decision is made.

Before production activation, a maintainer must explicitly accept the
accounting contract, assign a deployment owner, and confirm staging acceptance
and rollback responsibility. CI success is evidence for code review, not a
substitute for these approvals. None is recorded as granted by this document.

## Deployment order (maintainer action, not performed here)

1. Validate against an isolated copy of the current schema. Existing main
   migrations, including the compact/wire RPCs, Cloud ingest transaction and
   archive/restore and device-usage-reset functions, are prerequisites. Back up the schema and record
   pre-change account/leaderboard totals. Do not apply the superseded September
   migration from an older PR head: it has been replaced by
   `20261019120000_unclassified-input-tokens.sql` so fresh installs and upgrades
   use the same ordering after current main. If an experimental database already
   applied the old migration, reconcile its schema explicitly first; this is
   not an idempotent second application.
2. Apply the new migration atomically, using the migration runner's transaction.
   It extends hourly/session/rollup/snapshot storage, current compact/wire
   responses, transactional ingest and cold restore. Guarded rewrites abort on
   unexpected function definitions instead of silently replacing newer logic.
   Canonical weekend pricing, maintenance/reset read locks, membership checks, correction
   revisions and project-admin-only read permissions must remain intact.
3. Keep the server-side ingest environment setting
   `TOKENTRACKER_ACCOUNTING_V3_READY` absent or `0` while deploying all changed
   account, ingest, leaderboard/profile and SVG edge patches. The default is
   OFF: a migrated database alone must not advertise a ready rollout. Verify
   every deployed source and numeric response, not just deploy exit statuses.
   Complete legacy rows remain accepted; partial POSTs fail closed with 503.
   Request/query parameters cannot opt into the server-side setting.
4. Verify an isolated account: known + unknown rows, same-machine mirrors,
   session LWW corrections, cold/hot parity, restore (including an old pack
   without the field), leaderboard lifetime corrections and unauthorized reads.
   Confirm volume is intact and incomplete costs never look like full bills.
5. After all consumer versions and staging acceptance are verified, the
   maintainer explicitly sets `TOKENTRACKER_ACCOUNTING_V3_READY=1` on ingest.
   `GET tokentracker-ingest?capabilities=1` must now report v3. Both the setting
   AND the database capability RPC are required; a missing/unavailable RPC still
   reports v2. Check legacy complete traffic again before client rollout.
6. Only after backend acceptance and activation, release CLI and all bundled desktop clients
   through the normal upstream version/release workflow. This PR does not bump
   versions, trigger releases, install local patches, or change user history.

Client uploads negotiate v3 for both hourly and session-state batches before
advancing their local offset. Capability requests share the upload deadline,
and cancellation is rechecked before POST. If the backend is not ready, local
usage remains queued for a later retry. The database capability marker is
installed last inside the migration transaction, but it proves schema
readiness only. The explicit server-side setting is the separate final
admission gate for the coordinated rollout.

Rollback: first set `TOKENTRACKER_ACCOUNTING_V3_READY=0` to stop new partial
admission, then stop the new client rollout. Retain the additive schema, queued
history AND partial-aware account/leaderboard readers: once unknown-input rows
exist, reverting readers to v2 can make a subtotal look like a full bill. Do not
drop the columns, zero user history, or roll back those readers without a
separately verified data-safe plan.
The leaderboard wire keeps its original eleven row slots; unknown counts use
an additional parallel metadata array so old decoders keep working during the
migration-to-edge deployment interval.

Known limitation: fail-closed admission is not per-provider cloud isolation.
An already patched client can encounter a partial row before the backend is
enabled, or after the emergency switch is turned off. The shared queue retains
its offset, so complete records in the same batch or later in the queue also
wait. Local statistics and history remain intact, but do not describe this as
"only custom-endpoint uploads pause." A mixed batch is never partially
acknowledged. This operational tradeoff needs explicit maintainer acceptance;
independent deferred-provider queues would be a separate design, not an
unreviewed addition to this PR.

## Local verification

`test/cloud-unclassified-integration.test.js` runs unconditionally with PGlite
against the real current-main SQL functions and the actual new migration. It
does not use a deployed database, credentials, or fabricated archive RPCs.
`test/cloud-unclassified-input.test.js` separately exercises edge decoding and
partial billing semantics. `test/vscode-copilot-accounting.test.js` covers the
parser-to-local-API path, mixed costs and per-row premium preservation.
`test/sync-upload-batching.test.js` covers old-cloud rejection, cancellation,
offset retention and both hourly and session-state capability negotiation.

Run `npm test`, dashboard tests/typecheck/build, pricing parity, and the
repository validators before review. Local passes are not substitutes for
platform CI, maintainer review or production acceptance.

Integration verification (2026-10-19, Node 24.11.1): root suite 4,306 passed,
16 skipped, zero failures; dashboard 160 files / 1,566 tests passed. Typecheck,
TypeScript lint, production dashboard build, copy/locale/UI/architecture/version
and generated-frame validators passed. Loopback HTTP tests ran outside the
restricted sandbox after its port restrictions were identified. No native
cross-platform build or production deployment is inferred from these results.

Follow-up verification (2026-10-21, upstream `4cb82c70`): root suite 4,340
passed / 16 skipped / zero failures. Dashboard final full rerun: 161 files /
1,567 tests passed. Typecheck, TypeScript lint, build and repository validators
passed. One untouched gift-redemption focus assertion failed during the initial
concurrent run; its isolated 26-test file and the final full single-worker run
passed without changing assertions. The new default-off gate regressions cover
complete, partial and mixed batches even when the database already supports v3.
