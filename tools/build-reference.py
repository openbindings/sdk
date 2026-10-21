"""Build and identify a local static reference bundle; never deploy it."""
from pathlib import Path
import argparse
import hashlib
import html
from html.parser import HTMLParser
import json
import os
import shutil
import subprocess
import sys
import tomllib
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]


def run(arguments):
    return subprocess.check_output(list(map(str, arguments)), cwd=ROOT, text=True)


def repair_inherited_links(output):
    """Resolve known upstream links emitted literally in blanket impls.

    Only these exact hrefs on HTTP-discovery pages are rewritten. Keep the
    dependency version explicit in the destinations and record each repair;
    unrelated missing links remain failures rather than being silently removed.
    """
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    def dependency_docs(package, crate):
        versions = {p["version"] for p in lock["package"] if p["name"] == package}
        if len(versions) != 1:
            raise SystemExit(f"Resolve {package} reference links for the current dependency versions.")
        return f"https://docs.rs/{package}/{versions.pop()}/{crate}/"

    base = dependency_docs("tracing", "tracing")
    destinations = {
        "dispatcher#setting-the-default-subscriber": base + "dispatcher/index.html#setting-the-default-subscriber",
        "super::Span::current()": base + "struct.Span.html#method.current",
        "crate::Span": base + "struct.Span.html",
        "super::Subscriber": base + "trait.Subscriber.html",
        "futures_core::future::TryFuture": dependency_docs("futures-core", "futures_core") + "future/trait.TryFuture.html",
    }
    repairs = []
    for page in sorted((output / "rust/openbindings_http_discovery").rglob("*.html")):
        original = current = page.read_text()
        for source, target in destinations.items():
            needle = f'href="{source}"'
            count = current.count(needle)
            if count:
                current = current.replace(needle, f'href="{target}"')
                repairs.append({"page": page.relative_to(output).as_posix(),
                                "from": source, "to": target, "count": count})
        if current != original:
            page.write_text(current)
    return repairs


def clear_rust_docs():
    """Empty the target's documentation directory before rustdoc writes the bundle.

    Rustdoc adds to that directory and never removes a page, so pages left by an
    earlier `cargo doc`, of another crate or of a removed item, would otherwise
    ship in the bundle. Cargo removes only the documentation, keeping the build
    cache, and regenerates every page on the next rustdoc run.
    """
    subprocess.run(["cargo", "clean", "--doc"], cwd=ROOT, check=True)


def verify_local_links(output):
    """Check HTML anchor destinations, excluding external URLs and fragments."""
    class Links(HTMLParser):
        def __init__(self):
            super().__init__()
            self.targets = []

        def handle_starttag(self, tag, attrs):
            if tag == "a":
                self.targets.extend(value for name, value in attrs if name == "href" and value)

    checked = 0
    failures = []
    for page in sorted(output.rglob("*.html")):
        links = Links()
        links.feed(page.read_text())
        for href in links.targets:
            # Rustdoc uses this exact no-op for navigation controls, not a URL.
            if href == "javascript:void(0)":
                continue
            target = urlsplit(href)
            if target.scheme or target.netloc:
                if target.scheme and target.scheme not in {"http", "https", "mailto"}:
                    failures.append(f"{page.relative_to(output)}: unexpected link {href}")
                continue
            if not target.path:
                continue
            path = (page.parent / unquote(target.path)).resolve()
            checked += 1
            if not path.is_relative_to(output.resolve()) or not path.exists():
                failures.append(f"{page.relative_to(output)}: missing local target {href}")
    if failures:
        raise SystemExit("Reference links failed:\n" + "\n".join(failures))
    return {"relativeTargetsChecked": checked,
            "scope": "Local HTML anchor target files and URL schemes; no external availability or fragment check"}


def source_identity():
    """Identify actual tracked and untracked source without writing Git state."""
    names = run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"])
    return {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
            if (ROOT / name).is_file() else None
            for name in sorted(set(names.split("\0")) - {""})}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--preview", action="store_true",
                        help="Verify uncommitted source as an identified local preview, not a release candidate")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.is_relative_to(ROOT) and not output.is_relative_to(ROOT / "target"):
        raise SystemExit("Inside the checkout, use an ignored target/ output directory.")
    tree_clean = not run(["git", "status", "--porcelain"]).strip()
    if not tree_clean and not args.preview:
        raise SystemExit("Reference candidate must use a clean committed source.")
    commit = run(["git", "rev-parse", "HEAD"]).strip()
    sources = source_identity()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    build = ROOT / "packages/typescript/dist/wasm/build-info.json"
    identity = json.loads(build.read_text())
    if identity["commit"] != commit or (not args.preview and not identity["treeClean"]):
        raise SystemExit("Rebuild the TypeScript/Wasm package from this clean candidate.")
    current_rust = json.loads(run([
        "node", "--input-type=module", "-e",
        "import { rustSourceIdentity } from './packages/typescript/scripts/source-identity.mjs'; "
        "console.log(JSON.stringify(await rustSourceIdentity()));",
    ]))
    if identity["rustSource"] != current_rust:
        raise SystemExit("Rebuild the TypeScript/Wasm package from the current Rust source.")
    for name, expected in identity["artifacts"].items():
        actual = hashlib.sha256((build.parent / name).read_bytes()).hexdigest()
        if actual != expected:
            raise SystemExit(f"Wasm artifact hash mismatch: {name}")
    output.mkdir(parents=True, exist_ok=False)
    npm = "npm.cmd" if os.name == "nt" else "npm"
    subprocess.run([npm, "run", "build"], cwd=ROOT / "packages/typescript", check=True)
    clear_rust_docs()
    subprocess.run([sys.executable, ROOT / "tools/verify-api-reference.py", output / "typescript"], cwd=ROOT, check=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    shutil.copytree(target / "doc", output / "rust")
    links = [
        ("Rust SDK", "rust/openbindings/index.html"),
        ("Default schema evaluator", "rust/openbindings_json_schema_evaluator/index.html"),
        ("HTTP discovery", "rust/openbindings_http_discovery/index.html"),
        ("Custom evaluator qualification", "rust/openbindings_schema_evaluator_test_support/index.html"),
        ("TypeScript", "typescript/typescript-reference.html"),
    ]
    for _, relative in links:
        if not (output / relative).is_file():
            raise SystemExit(f"Missing generated reference: {relative}")
    entries = "\n".join(f'<li><a href="{url}">{html.escape(label)}</a></li>' for label, url in links)
    (output / "index.html").write_text(
        '<!doctype html><html lang="en"><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width,initial-scale=1">'
        '<title>OpenBindings SDK reference</title>'
        '<style>body{font:17px/1.6 system-ui;max-width:52rem;margin:4rem auto;padding:0 1.5rem}'
        'li{margin:.5rem 0}code{overflow-wrap:anywhere}</style>'
        f'<h1>OpenBindings SDK {html.escape(version)}</h1>'
        '<p>Rust libraries and the TypeScript facade share one exact document engine.</p>'
        f'<ul>{entries}</ul><p>Source base: <code>{commit}</code>'
        f'{" (local preview; see REFERENCE.json for source hashes)" if args.preview else ""}.</p>'
        f'<p><a href="https://github.com/openbindings/sdk/tree/{commit}">'
        'Source, installation status, guides, and capability limits</a>.</p>'
    )
    rust_entries = "\n".join(
        f'<li><a href="{url.removeprefix("rust/")}">{html.escape(label)}</a></li>'
        for label, url in links if url.startswith("rust/")
    )
    # Rustdoc help/settings link to a crate index even with separate cargo rustdoc calls.
    (output / "rust/index.html").write_text(
        '<!doctype html><html lang="en"><meta charset="utf-8">'
        '<title>OpenBindings Rust reference</title><h1>Rust API reference</h1>'
        f'<ul>{rust_entries}</ul><p><a href="../index.html">SDK reference home</a>.</p>'
    )
    repairs = repair_inherited_links(output)
    link_check = verify_local_links(output)
    files = {p.relative_to(output).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
             for p in sorted(output.rglob("*")) if p.is_file()}
    if (run(["git", "rev-parse", "HEAD"]).strip() != commit
            or source_identity() != sources
            or (not args.preview and run(["git", "status", "--porcelain"]).strip())):
        raise SystemExit("Source changed during reference generation.")
    (output / "REFERENCE.json").write_text(json.dumps({
        "commit": commit, "version": version, "files": files,
        "treeClean": tree_clean, "sourceFiles": sources,
        "inheritedLinkRepairs": repairs, "localLinkCheck": link_check,
        "status": "local preview; not a release candidate" if args.preview else "local reference bundle; not deployed",
    }, indent=2) + "\n")
    print(f"Local reference: {output / 'index.html'}")


if __name__ == "__main__":
    main()
