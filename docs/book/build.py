"""Build the book and keep its theme menu to light and dark."""

from pathlib import Path
import re
import subprocess

book = Path(__file__).resolve().parent
output = book.parents[1] / "target/book"
subprocess.run(["mdbook", "build", str(book)], check=True)

# mdBook has no theme-list setting. Remove unused items so its keyboard navigation
# visits only visible choices, without maintaining a copy of the HTML template.
for page in output.glob("*.html"):
    html = page.read_text()
    if 'id="mdbook-theme-list"' not in html:
        continue
    html, removed = re.subn(
        r'<li role="none"><button[^>]*id="mdbook-theme-(?:default_theme|rust|navy|ayu)"[^>]*>[^<]*</button></li>',
        "",
        html,
    )
    html, renamed = re.subn(r'(id="mdbook-theme-coal">)Coal(</button>)', r'\1Dark\2', html)
    if (removed, renamed) != (4, 1):
        raise RuntimeError(f"Unexpected mdBook theme menu in {page}")
    page.write_text(html)
