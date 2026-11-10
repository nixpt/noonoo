# Headless Chromium's block geometry of a page, in DOM order from <body>, measured once the
# page's `load` event has fired, with the network off: remote images fail to load, as they
# do when noonoo lays the page out with no images.
#   E idx tag id x y w h n_client_rects in_svg   one per element (in_svg: 1 for a descendant
#                                                 of an <svg>, 0 otherwise, the <svg> itself too)
#   T x y w h text                          one per text client rect
# Both engines use DejaVu Sans: it is forced here, and fragment-xdump registers it.
import html as H
import json
import os
import re
import subprocess
import sys
import tempfile

CHROMIUM = os.environ.get("CHROMIUM", "/usr/bin/chromium")
INJ = r"""<style>*{font-family:'DejaVu Sans'!important}</style><script>
(function(){function go(){var out=[];var b=document.body;var idx=0;
function walk(n){if(n.nodeType!==1)return;var t=n.tagName.toLowerCase();
if(t==='script'||t==='style'||t==='head')return;
var r=n.getBoundingClientRect();var cr=n.getClientRects().length;
var sv=(n.parentElement&&n.parentElement.closest('svg'))?1:0;
out.push(['E',idx++,t,n.id||'',r.x,r.y,r.width,r.height,cr,sv]);
for(var c=n.firstChild;c;c=c.nextSibling){if(c.nodeType===1)walk(c);else if(c.nodeType===3){var rg=document.createRange();rg.selectNodeContents(c);var rs=rg.getClientRects();for(var i=0;i<rs.length;i++){var q=rs[i];if(q.width>0||q.height>0)out.push(['T',q.x,q.y,q.width,q.height,c.data.slice(0,40)]);}}}}
walk(b);document.documentElement.setAttribute('data-x',JSON.stringify(out));
}window.addEventListener('load',go);})();</script>"""


def run(path, width=1024):
    src = open(path, encoding="utf-8").read()
    with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False, encoding="utf-8") as f:
        f.write(src + INJ)
        tmp = f.name
    try:
        o = subprocess.run(
            [CHROMIUM, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-sandbox",
             "--host-resolver-rules=MAP * ~NOTFOUND",
             f"--window-size={width},600", "--dump-dom", "file://" + tmp],
            capture_output=True, text=True, timeout=60,
        ).stdout
    finally:
        os.unlink(tmp)
    m = re.search(r'data-x="([^"]*)"', o)
    if not m:
        raise SystemExit(f"{path}: chromium produced no geometry")
    return json.loads(H.unescape(m.group(1)))


if __name__ == "__main__":
    w = int(sys.argv[2]) if len(sys.argv) > 2 else 1024
    for row in run(sys.argv[1], w):
        if row[0] == "E":
            print("E\t%d\t%s\t%s\t%.2f\t%.2f\t%.2f\t%.2f\t%d\t%d" % tuple(row[1:]))
        else:
            print("T\t%.2f\t%.2f\t%.2f\t%.2f\t%r" % tuple(row[1:]))
