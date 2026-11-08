//! The asynchronous loading seam: a [`Loader`] starts a request and
//! answers it later through its [`Completion`], from any thread.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use noonoo_net::{Completion, Completions, FileLoader, LoadError, Loader, Request, resolve};

#[test]
fn urls_resolve_against_their_base() {
    let base = "https://a.example/docs/guide/page.html?x=1#top";
    for (reference, expected) in [
        ("img.png", "https://a.example/docs/guide/img.png"),
        ("./img.png", "https://a.example/docs/guide/img.png"),
        ("../img.png", "https://a.example/docs/img.png"),
        ("../../../../img.png", "https://a.example/img.png"),
        ("/root.png", "https://a.example/root.png"),
        ("//b.example/x.png", "https://b.example/x.png"),
        ("?y=2", "https://a.example/docs/guide/page.html?y=2"),
        ("#end", "https://a.example/docs/guide/page.html?x=1#end"),
        ("", "https://a.example/docs/guide/page.html?x=1"),
        ("data:,hi", "data:,hi"),
        ("HTTP://c.example/", "HTTP://c.example/"),
        ("a/./b/../c.png", "https://a.example/docs/guide/a/c.png"),
    ] {
        assert_eq!(resolve(base, reference), expected, "{reference}");
    }
}

#[test]
fn trailing_dot_segments_leave_a_directory() {
    let base = "https://a.example/docs/guide/page.html";
    assert_eq!(resolve(base, "."), "https://a.example/docs/guide/");
    assert_eq!(resolve(base, ".."), "https://a.example/docs/");
    assert_eq!(resolve(base, "../.."), "https://a.example/");
}

#[test]
fn only_a_well_formed_scheme_makes_a_reference_absolute() {
    let base = "https://a.example/d/page.html";
    assert_eq!(
        resolve(base, "img/a:b.png"),
        "https://a.example/d/img/a:b.png"
    );
    assert_eq!(resolve(base, "1x:y"), "https://a.example/d/1x:y");
    assert_eq!(resolve(base, "a+b.c-d:x"), "a+b.c-d:x");
}

#[test]
fn a_base_without_a_path_resolves_from_the_root() {
    assert_eq!(
        resolve("https://a.example", "img.png"),
        "https://a.example/img.png"
    );
    assert_eq!(
        resolve("https://a.example?q=1", "img.png"),
        "https://a.example/img.png"
    );
    assert_eq!(resolve("https://a.example#f", "?x"), "https://a.example?x");
}

#[test]
fn file_urls_and_bare_paths_resolve_too() {
    assert_eq!(
        resolve("file:///home/u/site/index.html", "img/a.png"),
        "file:///home/u/site/img/a.png"
    );
    assert_eq!(
        resolve("file:///home/u/site/index.html", "/b.png"),
        "file:///b.png"
    );
    assert_eq!(
        resolve("/home/u/site/index.html", "a.png"),
        "/home/u/site/a.png"
    );
    assert_eq!(resolve("index.html", "a.png"), "a.png");
    // A data: base has no path to resolve against.
    assert_eq!(resolve("data:text/html,<p>", "a.png"), "a.png");
}

#[test]
fn the_file_loader_answers_through_the_queue() {
    let path = std::env::temp_dir().join("noonoo-net-loader-test.bin");
    std::fs::write(&path, [0x89, b'P', b'N', b'G']).unwrap();
    let completions = Completions::new();
    let loader = FileLoader;
    let url = format!("file://{}", path.display());
    let first = completions.request(url.clone());
    let second = completions.request("file:///no/such/file".to_owned());
    let (first_id, second_id) = (first.0.id, second.0.id);
    assert_ne!(first_id, second_id);
    loader.start(first.0, first.1);
    loader.start(second.0, second.1);
    let mut done = completions.drain();
    done.sort_by_key(|loaded| loaded.id);
    assert_eq!(done.len(), 2);
    assert_eq!(done[0].id, first_id);
    assert_eq!(done[0].url, url);
    assert_eq!(done[0].result.as_deref(), Ok(&[0x89, b'P', b'N', b'G'][..]));
    assert!(matches!(done[1].result, Err(LoadError::Io(_))));
    assert!(completions.drain().is_empty());
}

/// A loader that answers from another thread, as a network one would.
struct Threaded;

impl Loader for Threaded {
    fn start(&self, request: Request, completion: Completion) {
        std::thread::spawn(move || {
            completion.complete(Ok(request.url.into_bytes()));
        });
    }
}

#[test]
fn completions_can_arrive_from_another_thread_and_wake_the_host() {
    let completions = Completions::new();
    let woken = Arc::new(AtomicUsize::new(0));
    let counter = woken.clone();
    completions.set_waker(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let (request, completion) = completions.request("data:,x".to_owned());
    Threaded.start(request, completion);
    let mut done = Vec::new();
    for _ in 0..1000 {
        done.extend(completions.drain());
        if !done.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].result.as_deref(), Ok(&b"data:,x"[..]));
    assert_eq!(woken.load(Ordering::SeqCst), 1);
}

#[test]
fn a_dropped_completion_reports_a_failed_load() {
    let completions = Completions::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    completions.set_waker(move || log.lock().unwrap().push(()));
    drop(completions.request("x.png".to_owned()));
    let done = completions.drain();
    assert_eq!(done.len(), 1);
    assert!(done[0].result.is_err());
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(completions.pending(), 0);
}

#[test]
fn pending_counts_unanswered_requests() {
    let completions = Completions::new();
    let (request, completion) = completions.request("data:,a".to_owned());
    assert_eq!(completions.pending(), 1);
    FileLoader.start(request, completion);
    assert_eq!(completions.pending(), 0);
    assert_eq!(completions.drain().len(), 1);
}

#[test]
fn percent_encoded_data_keeps_its_bytes() {
    assert_eq!(
        noonoo_net::load("data:,%89PNG%FF").unwrap(),
        vec![0x89, b'P', b'N', b'G', 0xff]
    );
}
