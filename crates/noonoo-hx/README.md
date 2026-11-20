# noonoo-hx

An htmx-like layer: `hx-get` style attributes drive a fetch and a DOM swap.

## Where it sits

Depends on these noonoo crates: `noonoo-dom`, `noonoo-events`, `noonoo-html`, `noonoo-net`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
use noonoo_events::{ElementState, Event, Modifiers, MouseButton, PointerEvent};
use noonoo_hx::{Hx, StaticFetcher};

let mut doc = noonoo_html::parse(
    r##"<div id="out">old</div><button hx-get="/x" hx-target="#out" hx-swap="innerHTML">go</button>"##,
);
let fetcher = StaticFetcher::new().with("/x", "<p>new</p>");
let hx = Hx::new(&fetcher);
let button = doc.query_selector("button").unwrap().unwrap();
let click = Event::Click(PointerEvent {
    x: 0.0,
    y: 0.0,
    button: MouseButton::Primary,
    state: ElementState::Released,
    modifiers: Modifiers::default(),
});
hx.handle(&mut doc, &click, button).unwrap().unwrap();
let out = doc.get_element_by_id("out").unwrap();
assert_eq!(doc.text_content(out), "new");
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
