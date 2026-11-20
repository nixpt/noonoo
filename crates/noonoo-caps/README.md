# noonoo-caps

Capability gating for the DOM: scopes, grants, a policy seam, and a gated document view.

## Where it sits

Depends on these noonoo crates: `noonoo-css`, `noonoo-dom`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
use noonoo_caps::{Action, Gate, GatedDocument, Grant, Policy, Scope, Subject};
use noonoo_dom::Document;

let mut document = Document::new();
let secret = document.create_element("secret");
let root = document.root();
document.mutate().append_child(root, secret).unwrap();

let mut gate = Gate::new();
gate.guard(secret, Scope::path("noonoo.dom", "#secret"));

let alice = Subject::new("did:exo:alice");
let policy = Policy::new().grant(Grant::read(alice.clone(), Scope::path("noonoo.dom", "#secret")));

let anonymous = Subject::anonymous();
let denied = GatedDocument::new(&document, &gate, &policy, &anonymous);
assert!(!denied.visible(secret));

let allowed = GatedDocument::new(&document, &gate, &policy, &alice);
assert!(allowed.can(Action::Read, secret));
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
