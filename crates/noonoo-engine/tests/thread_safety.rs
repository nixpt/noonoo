//! A tab worker owns an `Engine` off the main thread, and a host cap may
//! share one, so it must stay `Send + Sync`. This fails to compile if it
//! stops being thread-safe.

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn the_engine_is_send_and_sync() {
    assert_send_sync::<noonoo_engine::Engine>();
}
