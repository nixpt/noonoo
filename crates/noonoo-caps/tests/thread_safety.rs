//! A host cap (`arniko-crush`'s `dom.*`) holds these across threads, so they
//! must stay `Send + Sync`. The bliss engine they replace was `!Send`, which
//! forced surfer into a `thread_local!` workaround. This fails to compile if
//! any of them stops being thread-safe.

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn documents_gates_and_policies_are_send_and_sync() {
    assert_send_sync::<noonoo_dom::Document>();
    assert_send_sync::<noonoo_caps::Gate>();
    assert_send_sync::<noonoo_caps::Policy>();
    assert_send_sync::<noonoo_caps::Subject>();
}
