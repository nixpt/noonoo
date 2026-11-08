#!/usr/bin/env python3
"""Fetch public pages for noonoo-bliss-diff into OUT_DIR, self-contained.

Neither engine loads <link rel=stylesheet> or runs scripts in the diff, so
each page's stylesheets are inlined as <style> and its scripts dropped:
both engines then see the styling a browser would, and nothing else.

usage: fetch.py OUT_DIR [URL...]   (defaults to the list below)
"""
import re
import sys
import urllib.parse
import urllib.request

PAGES = [
    "https://info.cern.ch/hypertext/WWW/TheProject.html",
    "https://example.com/",
    "https://en.wikipedia.org/wiki/Web_browser",
    "https://news.ycombinator.com/",
    "https://www.rust-lang.org/",
    "https://danluu.com/",
    "https://www.gnu.org/",
    "https://lite.cnn.com/",
]

HEADERS = {"User-Agent": "noonoo-bliss-diff/0 (layout comparison tool)"}


def get(url):
    request = urllib.request.Request(url, headers=HEADERS)
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read().decode(response.headers.get_content_charset() or "utf-8", "replace")


LINK = re.compile(r"<link\b[^>]*>", re.I)
SCRIPT = re.compile(r"<script\b.*?</script\s*>", re.I | re.S)


def inline(url, html):
    def replace(match):
        tag = match.group(0)
        if not re.search(r"rel\s*=\s*[\"']?[^\"'>]*stylesheet", tag, re.I):
            return tag
        href = re.search(r"href\s*=\s*[\"']([^\"']+)[\"']", tag, re.I)
        if not href:
            return ""
        try:
            css = get(urllib.parse.urljoin(url, href.group(1)))
        except Exception as error:  # a missing sheet shouldn't sink the page
            print(f"  {href.group(1)}: {error}", file=sys.stderr)
            return ""
        return "<style>" + css.replace("</style", "<\\/style") + "</style>"

    return LINK.sub(replace, SCRIPT.sub("", html))


def name(url):
    parts = urllib.parse.urlsplit(url)
    slug = re.sub(r"[^a-z0-9]+", "-", (parts.netloc + parts.path).lower()).strip("-")
    return slug[:80] + ".html"


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    for url in sys.argv[2:] or PAGES:
        try:
            html = inline(url, get(url))
        except Exception as error:
            print(f"{url}: {error}", file=sys.stderr)
            continue
        path = f"{out}/{name(url)}"
        with open(path, "w", encoding="utf-8") as file:
            file.write(html)
        print(f"{url} -> {path} ({len(html)} bytes)")


if __name__ == "__main__":
    main()
