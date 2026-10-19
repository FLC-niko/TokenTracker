# Copilot accounting v3: integration and maintainer rollout

PR #569 is integrated with upstream `ad65a585` (v1.4.6). This is not a
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
3. Deploy the changed account, ingest, leaderboard/profile and SVG edge patches
   together. Verify the deployed source and numeric responses, not just a deploy
   command's exit status. `GET tokentracker-ingest?capabilities=1` must return v3
   only when the database capability RPC is installed. Missing/unavailable
   capability RPCs report v2; partial POSTs are rejected rather than silently
   truncated by an old transaction. Historical zero-unknown records stay valid.
4. Verify an isolated account: known + unknown rows, same-machine mirrors,
   session LWW corrections, cold/hot parity, restore (including an old pack
   without the field), leaderboard lifetime corrections and unauthorized reads.
   Confirm volume is intact and incomplete costs never look like full bills.
5. Only after backend acceptance, release CLI and all bundled desktop clients
   through the normal upstream version/release workflow. This PR does not bump
   versions, trigger releases, install local patches, or change user history.

Client uploads negotiate v3 for both hourly and session-state batches before
advancing their local offset. Capability requests share the upload deadline,
and cancellation is rechecked before POST. If the backend is not ready, local
usage remains queued for a later retry. The capability marker is installed last
inside the migration transaction.

Rollback: disable/roll back new client uploads and edge functions as needed;
retain the additive schema and queued history. Do not drop unknown-input
columns, zero user history or present a subtotal as a total to force a rollback.
The leaderboard wire keeps its original eleven row slots; unknown counts use
an additional parallel metadata array so old decoders keep working during the
migration-to-edge deployment interval.

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
