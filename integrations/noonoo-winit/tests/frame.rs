//! Frame composition: host layers, then the texture overlay, then scene
//! overlays, then the effects hook. Checked through the scene's encoding,
//! so no GPU is needed.

use std::sync::{Arc, Mutex};

use noonoo_engine::Engine;
use noonoo_geometry::{Rect, Size};
use noonoo_shell::Host;
use noonoo_text::MetricShaper;
use noonoo_winit::frame::{FrameHooks, TexturePaint, compose};
use vello::Scene;
use vello::kurbo::{Affine, Rect as KRect};
use vello::peniko::{Blob, Color, Fill, ImageAlphaType, ImageData, ImageFormat};

const SIZE: (u32, u32) = (200, 100);

fn host() -> Host {
    let mut host = Host::new(Size::new(200.0, 100.0));
    let page =
        Engine::from_html(r#"<div style="width: 50px; height: 20px; background: red"></div>"#);
    host.add_content(page, Rect::new(0.0, 0.0, 200.0, 100.0));
    host.layout(&MetricShaper);
    host
}

fn rect_scene() -> Scene {
    let mut scene = Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 255, 255),
        None,
        &KRect::new(0.0, 0.0, 10.0, 10.0),
    );
    scene
}

fn image() -> ImageData {
    ImageData {
        data: Blob::new(Arc::new(vec![255u8; 4 * 4 * 4])),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 4,
        height: 4,
    }
}

fn paths(scene: &Scene) -> u32 {
    scene.encoding().n_paths
}

#[test]
fn no_hooks_composes_only_the_host_layers() {
    let host = host();
    let scene = compose(&host, 1.0, SIZE, &FrameHooks::default(), None);
    assert!(paths(&scene) > 0, "the red div paints");
}

#[test]
fn a_scene_overlay_adds_its_paths_inside_a_clip() {
    let host = host();
    let base = compose(&host, 1.0, SIZE, &FrameHooks::default(), None);

    let hooks = FrameHooks::default();
    hooks.set_overlay_scene(
        Arc::new(rect_scene()),
        Affine::translate((5.0, 5.0)),
        Some(KRect::new(0.0, 0.0, 100.0, 100.0)),
    );
    let scene = compose(&host, 1.0, SIZE, &hooks, None);
    // One path for the rect; a clip layer encodes one path on push and one
    // on pop.
    assert_eq!(paths(&scene), paths(&base) + 3);
    assert_eq!(scene.encoding().n_clips, base.encoding().n_clips + 2);
}

#[test]
fn setting_a_scene_overlay_replaces_the_previous_one() {
    let host = host();
    let base = compose(&host, 1.0, SIZE, &FrameHooks::default(), None);
    let hooks = FrameHooks::default();
    hooks.set_overlay_scene(Arc::new(rect_scene()), Affine::IDENTITY, None);
    hooks.set_overlay_scene(Arc::new(rect_scene()), Affine::IDENTITY, None);
    assert_eq!(
        paths(&compose(&host, 1.0, SIZE, &hooks, None)),
        paths(&base) + 1
    );

    hooks.clear_overlay_scenes();
    assert_eq!(
        paths(&compose(&host, 1.0, SIZE, &hooks, None)),
        paths(&base)
    );
}

#[test]
fn a_texture_overlay_is_one_image_fill() {
    let host = host();
    let base = compose(&host, 1.0, SIZE, &FrameHooks::default(), None);
    let texture = TexturePaint {
        image: image(),
        transform: Affine::IDENTITY,
        clip: None,
    };
    let scene = compose(&host, 1.0, SIZE, &FrameHooks::default(), Some(&texture));
    assert_eq!(paths(&scene), paths(&base) + 1);
}

#[test]
fn effects_run_last_with_the_surface_size() {
    let host = host();
    let hooks = FrameHooks::default();
    hooks.set_overlay_scene(Arc::new(rect_scene()), Affine::IDENTITY, None);
    let texture = TexturePaint {
        image: image(),
        transform: Affine::IDENTITY,
        clip: None,
    };
    let before_effects = paths(&compose(&host, 1.0, SIZE, &hooks, Some(&texture)));

    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    hooks.set_scene_effects(move |scene, width, height| {
        record.lock().unwrap().push((paths(scene), width, height));
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 0, 255),
            None,
            &KRect::new(0.0, 0.0, 1.0, 1.0),
        );
    });
    let scene = compose(&host, 1.0, SIZE, &hooks, Some(&texture));

    assert_eq!(
        *seen.lock().unwrap(),
        vec![(before_effects, SIZE.0, SIZE.1)]
    );
    assert_eq!(paths(&scene), before_effects + 1);

    hooks.clear_scene_effects();
    compose(&host, 1.0, SIZE, &hooks, Some(&texture));
    assert_eq!(seen.lock().unwrap().len(), 1, "a cleared hook doesn't run");
}

#[test]
fn hooks_are_shared_between_clones() {
    let host = host();
    let base = compose(&host, 1.0, SIZE, &FrameHooks::default(), None);
    let hooks = FrameHooks::default();
    let other = hooks.clone();
    other.set_overlay_scene(Arc::new(rect_scene()), Affine::IDENTITY, None);
    assert_eq!(
        paths(&compose(&host, 1.0, SIZE, &hooks, None)),
        paths(&base) + 1
    );
}

#[test]
fn no_device_until_a_renderer_exists() {
    let hooks = FrameHooks::default();
    assert!(hooks.device().is_none());
}

#[test]
fn an_effects_hook_may_set_overlays_without_deadlocking() {
    let host = host();
    let hooks = FrameHooks::default();
    let inside = hooks.clone();
    hooks.set_scene_effects(move |_, _, _| {
        inside.set_overlay_scene(Arc::new(rect_scene()), Affine::IDENTITY, None);
    });
    let base = paths(&compose(&host, 1.0, SIZE, &hooks, None));
    // The overlay set during the first frame shows in the next.
    assert_eq!(paths(&compose(&host, 1.0, SIZE, &hooks, None)), base + 1);
}

#[test]
fn an_effects_hook_that_clears_itself_stays_cleared() {
    let host = host();
    let hooks = FrameHooks::default();
    let inside = hooks.clone();
    let runs = Arc::new(Mutex::new(0));
    let count = runs.clone();
    hooks.set_scene_effects(move |_, _, _| {
        *count.lock().unwrap() += 1;
        inside.clear_scene_effects();
    });
    compose(&host, 1.0, SIZE, &hooks, None);
    compose(&host, 1.0, SIZE, &hooks, None);
    assert_eq!(*runs.lock().unwrap(), 1);
}

#[test]
fn an_effects_hook_that_replaces_itself_is_replaced() {
    let host = host();
    let hooks = FrameHooks::default();
    let inside = hooks.clone();
    let log = Arc::new(Mutex::new(Vec::new()));
    let first = log.clone();
    hooks.set_scene_effects(move |_, _, _| {
        first.lock().unwrap().push("first");
        let second = first.clone();
        inside.set_scene_effects(move |_, _, _| second.lock().unwrap().push("second"));
    });
    compose(&host, 1.0, SIZE, &hooks, None);
    compose(&host, 1.0, SIZE, &hooks, None);
    compose(&host, 1.0, SIZE, &hooks, None);
    assert_eq!(*log.lock().unwrap(), vec!["first", "second", "second"]);
}

#[test]
fn scene_overlays_draw_over_the_texture_overlay() {
    // vello doesn't re-export `DrawTag`; these are its `IMAGE` and `COLOR`.
    const IMAGE: u32 = 0x28C;
    const COLOR: u32 = 0x44;
    // A devtools highlight must sit on top of the page content it marks.
    let host = Host::new(Size::new(200.0, 100.0));
    let hooks = FrameHooks::default();
    hooks.set_overlay_scene(Arc::new(rect_scene()), Affine::IDENTITY, None);
    let texture = TexturePaint {
        image: image(),
        transform: Affine::IDENTITY,
        clip: None,
    };
    let scene = compose(&host, 1.0, SIZE, &hooks, Some(&texture));
    let tags = &scene.encoding().draw_tags;
    let image = tags.iter().position(|tag| tag.0 == IMAGE);
    let color = tags.iter().position(|tag| tag.0 == COLOR);
    assert!(
        image < color,
        "image {image:?} before the overlay's colour {color:?}"
    );
}
