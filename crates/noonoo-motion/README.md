# noonoo-motion

Spring timing: a critically-damped-or-bouncy spring whose target can change mid-flight without losing velocity.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
use noonoo_motion::{Spring, SpringState};

let spring = Spring::from_duration_bounce(0.4, 0.0);
let mut state = SpringState::new(0.0, 0.0);
for _ in 0..240 {
    state.step(&spring, 1.0, 1.0 / 60.0);
}
assert!(state.is_settled(1.0, 0.01));
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
