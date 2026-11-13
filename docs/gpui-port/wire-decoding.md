# Reading protocol data safely

The GPUI client reads each summary independently. A malformed identity drops
that row; malformed fields keep the other fields. `ThreadList::apply` permits
missing-thread cleanup only for a nonempty list with every row readable and
no duplicate identities. Failed, empty and partial lists preserve previous
rows and cannot stop their Terminal shells. Explicit deletion notifications
still remove threads normally.

The audit covers every serde decode in `desktop/client/src` and
`desktop/core/src`:

| Data | Reader and recovery |
| --- | --- |
| Thread summaries, asks, project memberships, background tasks | `inbox.rs`: tolerant fields, nested entries, required summary identity; `ThreadList` guards reconciliation. |
| Accounts, models, pools, defaults | `logins.rs`: fields and nested arrays recover separately; defaults maps keep readable values. |
| Projects, links, schedules, last runs, panels, actions, hooks and list panel items | `highlights.rs`: tolerant fields and per-entry lists, including nested Convex deployment data. |
| Origins, wake words, dependencies and subagents | `sidebar.rs`: tolerant typed fields, independent entries. |
| Reply buttons and their payloads, slash commands, panel requests | `reply_buttons.rs`, `slash.rs`, `panel_requests.rs`: tolerant fields and independent entries. Positional ask choices and their notes drop the whole malformed field rather than shifting another choice into its place. |
| Convex deployment choices and login state | `convex_panel.rs`: tolerant fields and deployment entries. Deploy keys remain required and fail closed. |
| BrowserBear host state and tabs | `browserbear.rs`: tolerant fields and independent tab entries, preserving the streaming cap and file size limit. |
| Remote threads and runners | `remote.rs`: independent manual reads; empty or partial thread indexes cannot remove previously seen remote threads. Remote summaries retain `keywords` and `createdAt`. |
| Events, cards, reviews, connectors, usage, files and settings | Existing `Value` readers recover at the field/entry boundary; unknown event types are ignored. |
| Client hello, RPC replies, notifications, restart state and runner configuration | `desktop/client`: existing `Value` readers, no typed collection deserialization. Authentication frames and malformed JSON remain errors. |
| Local build metadata, crash/hang records, saved preferences, script consent | Local files rather than protocol collections. Unreadable individual files/records retain their existing defaults or fail closed; consent is never reconstructed from corrupt data. |

`wire.rs` diagnostics contain type names and list positions. Serde errors,
values, map keys, ids and message text never enter those diagnostics. The
source location identifies the field, and each location and type is reported
once per app session, so rendering cannot flood the log. The installed app
routes diagnostics to its normal rotating desktop log.

Patch helpers and grants remain strict: the Trust prompt cannot offer a
patch with an incomplete disclosure. Sign-in is unavailable for a secondary
Claude or Codex login without its configuration folder, or a provider with
no sign-in command. Readable archive rows still stop their shells in a
partial list; missing rows need a complete list or an explicit deletion.

The fixtures in `desktop/core/tests/wire_recovery.rs` cover the failure cases,
including nested lists, strict credentials, duplicate identities, private
values in diagnostics, provider commands and old hellos.
