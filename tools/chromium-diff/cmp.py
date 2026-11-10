# Diff noonoo's block geometry (the noonoo-chromium-diff binary) against headless Chromium's
# (chrome.py), element by element in DOM order.
#   [XDUMP=path/to/noonoo-chromium-diff] TOL=1 [SVG_SKIPS=N] python3 -I cmp.py page.html [...] [w=1024]
# XDUMP defaults to this crate's release build (target/release, or $CARGO_TARGET_DIR/release).
# Prints ALIGN (the DOM orders differ), NO BOX, SKIP (see SVG_SKIPS), and every element whose box differs by
# more than TOL px. Exits 1 if any page has a mismatch, or if SVG_SKIPS is set and a different
# number of SVG shapes was skipped; exits 2 if SVG_SKIPS is not a whole number.
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # chrome.py beside this file
import chrome  # noqa: E402

TOL = float(os.environ.get("TOL", "1"))
# Descendants of an inline <svg>: noonoo lays the <svg> out as one replaced box and models no
# SVG geometry, while Chromium reports each shape's bbox. The <svg>'s own box is always
# compared. Opt-in: with SVG_SKIPS=N set, an element that
# Chromium places inside an <svg> and noonoo gives no box is printed as SKIP instead of
# counted as a mismatch, and the run fails unless exactly N were skipped, so a new skip
# shows up as a change. Unset or empty, they count as NO BOX mismatches.
SVG_SKIPS = os.environ.get("SVG_SKIPS") or None
# Not str.isdigit(), which takes "²" too.
if SVG_SKIPS is not None and not re.fullmatch(r"[0-9]+", SVG_SKIPS):
    print(f"SVG_SKIPS must be a whole number, not {SVG_SKIPS!r}", file=sys.stderr)
    sys.exit(2)
SKIPPED = []
HERE = os.path.dirname(os.path.abspath(__file__))
XDUMP = os.environ.get("XDUMP") or os.path.join(
    os.environ.get("CARGO_TARGET_DIR") or os.path.join(HERE, "target"), "release", "noonoo-chromium-diff")


def nn(path, w):
    o = subprocess.run([XDUMP, path, str(w)], capture_output=True, text=True, check=True).stdout
    E, T = [], []
    for line in o.splitlines():
        p = line.split("\t")
        (E if p[0] == "E" else T).append(p)
    return E, T


def main(path, w):
    E, T = nn(path, w)
    C = chrome.run(path, w)
    CE = [r for r in C if r[0] == "E"]
    print(f"== {os.path.basename(path)} w={w}: noonoo {len(E)} el, chrome {len(CE)} el")
    bad = 0
    for i in range(max(len(E), len(CE))):
        a = E[i] if i < len(E) else None
        b = CE[i] if i < len(CE) else None
        if a is None or b is None or a[2] != b[2]:
            print("  ALIGN", a and a[2:4], b and b[2:4])
            bad += 1
            continue
        if a[4] == "-" and (a[2] in ("br", "tr", "tbody", "thead", "tfoot") or (b[6] == 0 and b[7] == 0)):
            continue
        if a[4] == "-" and SVG_SKIPS is not None and b[9] == 1:
            print(f"  {i} {a[2]}#{a[3]} SKIP svg shape  chrome {b[4]:.1f},{b[5]:.1f} {b[6]:.1f}x{b[7]:.1f}")
            SKIPPED.append((os.path.basename(path), i, a[2]))
            continue
        if a[4] == "-":
            print(f"  {i} {a[2]}#{a[3]} noonoo NO BOX  chrome {b[4]:.1f},{b[5]:.1f} {b[6]:.1f}x{b[7]:.1f}")
            bad += 1
            continue
        na = [float(x) for x in a[4:8]]
        nb = b[4:8]
        if max(abs(x - y) for x, y in zip(na, nb)) > TOL:
            bad += 1
            print(f"  {i} {a[2]}#{a[3]} noonoo {na[0]:.1f},{na[1]:.1f} {na[2]:.1f}x{na[3]:.1f}"
                  f" | chrome {nb[0]:.1f},{nb[1]:.1f} {nb[2]:.1f}x{nb[3]:.1f}")
    print(f"  {bad} mismatches")
    return bad


if __name__ == "__main__":
    if not os.path.exists(XDUMP):
        raise SystemExit(f"{XDUMP}: build it with `cargo build --release` here, or set XDUMP")
    w = 1024
    pages = []
    for a in sys.argv[1:]:
        if a.startswith("w="):
            w = int(a[2:])
        else:
            pages.append(a)
    total = sum(main(p, w) for p in pages)
    print(f"TOTAL {total} mismatches over {len(pages)} pages (TOL={TOL}),"
          f" {len(SKIPPED)} svg shapes skipped")
    if SVG_SKIPS is not None and len(SKIPPED) != int(SVG_SKIPS):
        print(f"SVG_SKIPS={SVG_SKIPS} expected, {len(SKIPPED)} skipped")
        sys.exit(1)
    sys.exit(1 if total else 0)
