//! A `View` driven by an embedder's own event loop: the embedder forwards
//! window events and owns the decision to exit. No window or GPU is needed for
//! the event and layout paths checked here.

use std::cell::RefCell;
use std::rc::Rc;

use noonoo_dom::NodeId;
use noonoo_engine::Engine;
use noonoo_events::{Event, Propagation};
use noonoo_geometry::{Rect, Size};
use noonoo_shell::Host;
use noonoo_winit::view::{Flow, View};
use winit::dpi::PhysicalPosition;
use winit::event::{ButtonSource, ElementState, MouseButton, WindowEvent};

type Log = Rc<RefCell<Vec<(NodeId, &'static str)>>>;

fn view() -> (View, Log) {
    let mut host = Host::new(Size::new(200.0, 100.0));
    host.add_chrome(
        Engine::from_html(
            r#"<a id="go" href="shell://reload" style="display: block; height: 40px">Reload</a>"#,
        ),
        Rect::new(0.0, 0.0, 200.0, 100.0),
    );
    let log: Log = Rc::default();
    let record = log.clone();
    let view = View::new("test", host, move |node, event, _phase| {
        if let Event::Click(_) = event {
            record.borrow_mut().push((node, "click"));
        }
        Propagation::Continue
    });
    (view, log)
}

fn button(state: ElementState) -> WindowEvent {
    WindowEvent::PointerButton {
        device_id: None,
        state,
        position: PhysicalPosition::new(20.0, 20.0),
        primary: true,
        button: ButtonSource::Mouse(MouseButton::Left),
    }
}

/// The text the layer's boxes carry.
fn texts(view: &View) -> Vec<String> {
    view.host().layers()[0]
        .layout()
        .boxes()
        .iter()
        .filter_map(|b| b.text.clone())
        .collect()
}

#[test]
fn a_close_request_is_handed_to_the_embedder() {
    let (mut view, _) = view();
    assert_eq!(view.window_event(WindowEvent::CloseRequested), Flow::Close);
    assert_eq!(
        view.window_event(WindowEvent::Focused(true)),
        Flow::Continue
    );
}

#[test]
fn a_click_reaches_the_listener() {
    let (mut view, log) = view();
    view.update();
    view.window_event(button(ElementState::Pressed));
    view.window_event(button(ElementState::Released));
    let go = view.host().layers()[0]
        .engine()
        .dom()
        .get_element_by_id("go")
        .unwrap();
    // The click lands on the link's text and bubbles through the link.
    assert!(
        log.borrow()
            .iter()
            .any(|&(node, kind)| node == go && kind == "click"),
        "{:?}",
        log.borrow()
    );
}

#[test]
fn the_embedder_changes_the_document_and_the_view_lays_it_out() {
    let (mut view, _) = view();
    view.update();
    let engine = view.host_mut().layers_mut()[0].engine_mut();
    let go = engine.dom().get_element_by_id("go").unwrap();
    let text = engine.dom().tree().children(go)[0];
    engine.dom_mut().mutate().set_text(text, "Stop").unwrap();

    // Nothing is laid out until the embedder says the document changed.
    assert!(!view.update());
    view.invalidate();
    assert!(view.update());
    assert!(!view.update(), "a second update has nothing to do");
    assert!(
        texts(&view).contains(&"Stop".to_owned()),
        "{:?}",
        texts(&view)
    );
}

#[test]
fn the_view_hands_out_its_frame_hooks() {
    let (view, _) = view();
    // No window yet, so no device.
    assert!(view.frame_hooks().device().is_none());
    assert!(view.window().is_none());
}
