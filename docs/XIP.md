# Noonoo — Capability gating (XIP)

The original Bliss engine advertised *"DOM segments are cryptographically gated,
ensuring agents only see or mutate what they are authorized to."* It never built
it. Noonoo does — and splits the promise cleanly in two.

**Own the policy and the seam; borrow the cryptography.**

- **`noonoo-caps`** (in the workspace, dependency-free) owns the model: scopes,
  grants, actions, the `Authorizer` seam, and a gate over the DOM. A host can
  gate a document today with an in-memory policy and no keys.
- **`noonoo-xip`** (opt-in, *outside* the workspace) verifies a signed
  **Exosphere Identity Protocol** capability token via `xip-core` and admits the
  grants it carries. Only signature verification is borrowed.

The engine's default build never sees Ed25519, SHA-2, `chrono` or `bs58`.

## The model (`noonoo-caps`)

| Type | Meaning |
|---|---|
| `Subject` | who is acting — a DID, a uid, a role, or `Subject::anonymous()` |
| `Scope` | a *capability* name (`"noonoo.dom"`, `"*"`) over a `Resource` |
| `Resource` | `Any` / `Named(..)` / `Path(..)` — the three levels XIP distinguishes |
| `Action` | `Read` or `Write` |
| `Actions` | a small bitset of actions |
| `Grant` | `subject` may do `actions` within `scope` |
| `Authorizer` | **the seam** — one method: `authorize(subject, action, scope) -> Decision` |
| `Policy` | an owned `Authorizer`: a flat list of grants, deny by default |
| `Gate` | maps `NodeId`s to the `Scope` they require (covers the subtree) |
| `GatedDocument` / `GatedMutator` | read/write views that refuse what the subject may not do |

A guard on a node protects **that node and its whole subtree**; the *nearest*
guard at or above a node is the one that applies, so a nested guard can tighten
a branch.

```rust
use noonoo_caps::{Gate, GatedDocument, Grant, Policy, Scope, Subject};

let mut gate = Gate::new();
gate.guard(secret_node, Scope::path("noonoo.dom", "#secret"));

let agent = Subject::new("did:key:z…");
let policy = Policy::new().grant(Grant::read(agent.clone(), Scope::path("noonoo.dom", "#secret")));

let view = GatedDocument::new(&document, &gate, &policy, &agent);
view.visible(secret_node);              // true
view.text(secret_node);                 // Ok("…")
view.visible_text(document.root());     // denied branches elided

// Writes go through the same gate:
let mut writer = GatedMutator::new(&mut document, &gate, &policy, &agent);
writer.set_attribute(secret_node, "class", "revealed")?; // Err(Denied) for a read-only grant
```

Default deny: an unguarded node is open; a guarded node is open only to a
subject an `Authorizer` explicitly allows. There is no implicit "owner" — pass
`AllowAll` if you are *choosing* not to gate.

## The bridge (`noonoo-xip`)

`XipAuthorizer` implements `noonoo-caps`' `Authorizer`. Admitting a token:

1. derive the issuer's public key from its `did:key` (`verifying_key_from_did`),
2. verify the token signature (`CapabilityToken::verify`),
3. check the `nbf`/`exp` window (`CapabilityToken::is_valid`),
4. map the token to a grant and store it.

| XIP (`xip-core`) | Noonoo |
|---|---|
| `token.aud` | grant `subject` |
| `token.capability` | `Scope.capability` |
| `CapabilityScope::Global` | `Resource::Any` |
| `CapabilityScope::Service` / `Resource` | `Resource::Named` |
| `CapabilityScope::Path` | `Resource::Path` |
| `CapabilityScope::Network` | `Resource::Named(host)` |
| `signature`, `nbf`, `exp` | the precondition for admitting the grant |
| metadata `noonoo.actions: ["read"]` | narrows the grant to read-only (default: read+write) |

A token that fails verification is **not** admitted — there is no fallback path.

## The honest caveat

Read `xip-core`'s threat model. On a single box agents usually run as the **same
uid with no isolation**, so a co-resident process can read the signing key and
mint tokens that verify. Verification buys **integrity** and **future
federation**; it is **not** authenticity against a co-resident agent. Noonoo
makes no stronger claim.

## Where it lives, and how to run it

- `noonoo-caps` is a normal workspace member (zero dependencies). `cargo test -p
  noonoo-caps`.
- `noonoo-xip` is **excluded** from the workspace (see the root `exclude`), so
  the engine's lockfile stays free of the crypto stack. Build and test it on
  demand:

  ```sh
  cargo test --manifest-path integrations/noonoo-xip/Cargo.toml
  ```

  Its `xip-core` path dependency is local wiring: the crate is unpublished and
  private, so a real checkout would pin it by git or a vendored version.

## Out of scope for now

Matching `xip-core`'s own scope discipline: **revocation, delegation chains,
and freshness windows** are not implemented here. Delegation would be the next
step — `xip-core` already carries `TokenDelegation` and `validate_chain`, so a
future slice could fold a validated chain into the same `Grant` list.
