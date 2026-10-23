#!/usr/bin/env python3
"""Generate the bundled J help data for jive.

This script converts the MIT-licensed jsoftware/docs_help HTML files into a
single JSON file consumed by crates/help. It deliberately uses only the Python
standard library.

Usage:
    python3 tools/gen_help.py                 # download the pinned tarball
    python3 tools/gen_help.py --source DIR    # use an already-extracted copy
"""

from __future__ import annotations

import argparse
import datetime as _dt
import html
import json
import os
import re
import sys
import tarfile
import tempfile
import urllib.request
from html.parser import HTMLParser
from pathlib import Path

PINNED_COMMIT = "ae1314972b37f16ba0a67621aedef26db2c344ae"
UPSTREAM = "https://github.com/jsoftware/docs_help"
TARBALL = f"https://github.com/jsoftware/docs_help/archive/{PINNED_COMMIT}.tar.gz"
HELP_BASE = "https://www.jsoftware.com/help"

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "crates" / "help" / "data"
OUT_FILE = DATA_DIR / "help.json"

VOID_TAGS = {
    "area",
    "base",
    "br",
    "col",
    "embed",
    "hr",
    "img",
    "input",
    "link",
    "meta",
    "param",
    "source",
    "track",
    "wbr",
}

INLINE_TAGS = {
    "a",
    "abbr",
    "acronym",
    "b",
    "big",
    "cite",
    "code",
    "em",
    "font",
    "i",
    "kbd",
    "small",
    "span",
    "strong",
    "sub",
    "sup",
    "tt",
    "u",
    "var",
}

HEADING_TAGS = {"h1": 1, "h2": 2, "h3": 3, "h4": 4, "h5": 5, "h6": 6}


def strip_tags(value: str) -> str:
    return re.sub(r"<[^>]+>", "", value)


def normalize_inline(value: str) -> str:
    """Collapse source whitespace while preserving explicit <br> breaks."""
    value = value.replace("\xa0", " ").replace("\r", " ")
    value = re.sub(r"[ \t\f\v]+", " ", value)
    value = re.sub(r" *\n *", "\n", value)
    return value


def normalize_space(value: str) -> str:
    value = normalize_inline(value)
    value = re.sub(r"\n{3,}", "\n\n", value)
    return value.strip()


def span(text: str, *, code: bool = False, bold: bool = False, italic: bool = False) -> dict:
    item = {"text": text}
    if code:
        item["code"] = True
    if bold:
        item["bold"] = True
    if italic:
        item["italic"] = True
    return item


def clean_spans(items: list[dict]) -> list[dict]:
    """Normalize whitespace and merge adjacent spans with identical styling."""
    cleaned: list[dict] = []
    for item in items:
        text = normalize_inline(item.get("text", ""))
        if not text:
            continue
        key = (item.get("code", False), item.get("bold", False), item.get("italic", False))
        if cleaned and (
            cleaned[-1].get("code", False),
            cleaned[-1].get("bold", False),
            cleaned[-1].get("italic", False),
        ) == key:
            cleaned[-1]["text"] += text
        else:
            cleaned.append(
                span(
                    text,
                    code=item.get("code", False),
                    bold=item.get("bold", False),
                    italic=item.get("italic", False),
                )
            )

    # Only trim at the outside edges of the paragraph. Whitespace between
    # spans is significant: the source often uses it to separate a word from
    # an inline <tt> token.
    if cleaned:
        cleaned[0]["text"] = cleaned[0]["text"].lstrip(" \n")
        cleaned[-1]["text"] = cleaned[-1]["text"].rstrip(" \n")
    for item in cleaned:
        item["text"] = re.sub(r"[ \t]+", " ", item["text"])
        item["text"] = re.sub(r" *\n *", "\n", item["text"])

    cleaned = [item for item in cleaned if item["text"]]
    return cleaned


def spans_text(items: list[dict]) -> str:
    return "".join(item.get("text", "") for item in items)


class ContentParser(HTMLParser):
    def __init__(self, *, is_primitive: bool = False) -> None:
        super().__init__(convert_charrefs=True)
        self.is_primitive = is_primitive
        self.blocks: list[dict] = []
        self.para: list[dict] = []
        self.heading: tuple[int, list[dict]] | None = None
        self.pre: list[str] | None = None
        self.table: list[list[dict]] | None = None
        self.row: list[dict] | None = None
        self.cell: list[dict] | None = None
        self.cell_blocks: list[dict] | None = None
        self.list_items: list[list[dict]] | None = None
        self.list_item: list[dict] | None = None
        self.inline = {"code": 0, "bold": 0, "italic": 0}
        self.skip = 0

    # -- helpers ---------------------------------------------------------

    def make_span(self, text: str) -> dict:
        return span(
            text,
            code=self.inline["code"] > 0,
            bold=self.inline["bold"] > 0,
            italic=self.inline["italic"] > 0,
        )

    def add_text(self, text: str) -> None:
        if not text:
            return
        if self.pre is not None:
            self.pre.append(text)
            return
        # Newlines in the HTML source are ordinary whitespace. Only an
        # explicit <br> becomes a hard line break.
        text = text.replace("\r", " ").replace("\n", " ")
        target = self.cell
        if target is None:
            target = self.list_item
        if target is None and self.heading is not None:
            target = self.heading[1]
        if target is None:
            target = self.para
        target.append(self.make_span(text))

    def add_break(self) -> None:
        if self.pre is not None:
            self.pre.append("\n")
            return
        target = self.cell
        if target is None:
            target = self.list_item
        if target is None and self.heading is not None:
            target = self.heading[1]
        if target is None:
            target = self.para
        target.append(span("\n"))

    def flush_para(self) -> None:
        items = clean_spans(self.para)
        self.para = []
        if items:
            self.blocks.append({"kind": "para", "spans": items})

    def flush_heading(self) -> None:
        if self.heading is None:
            return
        level, raw = self.heading
        self.heading = None
        items = clean_spans(raw)
        if items:
            self.blocks.append({"kind": "heading", "level": level, "spans": items})

    def flush_item(self) -> None:
        if self.list_item is None:
            return
        items = clean_spans(self.list_item)
        self.list_item = None
        if items and self.list_items is not None:
            self.list_items.append(items)

    def finish_cell(self) -> None:
        if self.cell is None and self.cell_blocks is None:
            return
        self.flush_cell_para()
        blocks = self.cell_blocks or []
        text = normalize_space(
            "\n".join(
                spans_text(block.get("spans", []))
                if block.get("kind") == "para"
                else block.get("text", "")
                for block in blocks
            )
        )
        self.cell = None
        self.cell_blocks = None
        if self.row is not None:
            self.row.append({"text": text, "blocks": blocks})

    def flush_cell_para(self) -> None:
        if self.cell is None or self.cell_blocks is None:
            return
        items = clean_spans(self.cell)
        self.cell = []
        if items:
            self.cell_blocks.append({"kind": "para", "spans": items})

    def finish_row(self) -> None:
        self.finish_cell()
        if self.row is None:
            return
        row = [cell for cell in self.row]
        self.row = None
        if any(cell["text"].strip() for cell in row) and self.table is not None:
            self.table.append(row)

    def finish_table(self) -> None:
        self.finish_row()
        if self.table is None:
            return
        rows = self.table
        self.table = None
        if not rows:
            return

        # The title header on primitive pages is a single three-cell row:
        # name/symbol/name. It duplicates metadata we already store.
        if self.is_primitive and not self.blocks and len(rows) == 1 and len(rows[0]) == 3:
            first, middle, last = (cell["text"].strip() for cell in rows[0])
            if len(first) < 30 and len(last) < 30 and middle:
                return

        # Primitive pages normally use a single-row table with a narrow empty
        # spacer in the middle for the monad and dyad descriptions.
        if self.is_primitive and len(rows) == 1:
            row = [cell for cell in rows[0]]
            if len(row) == 3 and not row[1]["text"].strip():
                row = [row[0], row[2]]
            if (
                len(row) == 2
                and len(row[0]["text"]) > 30
                and len(row[1]["text"]) > 30
            ):
                for label, cell in zip(("Monad", "Dyad"), row):
                    self.blocks.append(
                        {"kind": "heading", "level": 3, "spans": [span(label, bold=True)]}
                    )
                    if cell["blocks"]:
                        self.blocks.extend(cell["blocks"])
                    elif cell["text"]:
                        self.blocks.append(
                            {"kind": "para", "spans": [span(cell["text"])]}
                        )
                return
            if len(row) == 1 and len(row[0]["text"]) > 30:
                if row[0]["blocks"]:
                    self.blocks.extend(row[0]["blocks"])
                elif row[0]["text"]:
                    self.blocks.append(
                        {"kind": "para", "spans": [span(row[0]["text"])]}
                    )
                return

        self.blocks.append(
            {"kind": "table", "rows": [[cell["text"] for cell in row] for row in rows]}
        )

    def finish_code(self) -> None:
        if self.pre is None:
            return
        text = "".join(self.pre).replace("\r", "")
        self.pre = None
        lines = text.split("\n")
        while lines and not lines[0].strip():
            lines.pop(0)
        while lines and not lines[-1].strip():
            lines.pop()
        text = "\n".join(lines)
        if not text:
            return
        block = {"kind": "code", "text": text}
        if self.cell_blocks is not None:
            self.cell_blocks.append(block)
        else:
            self.blocks.append(block)

    # -- HTMLParser callbacks -------------------------------------------

    def handle_starttag(self, tag: str, attrs) -> None:
        tag = tag.lower()
        if tag in ("script", "style"):
            self.skip += 1
            return
        if self.skip:
            return

        if tag == "pre":
            if self.cell_blocks is not None:
                self.flush_cell_para()
            else:
                self.flush_para()
            self.pre = []
            return
        if self.pre is not None:
            if tag == "br":
                self.pre.append("\n")
            return

        if tag in HEADING_TAGS:
            self.flush_para()
            self.heading = (HEADING_TAGS[tag], [])
            return
        if tag == "table":
            self.flush_para()
            self.table = []
            self.row = None
            self.cell = None
            self.cell_blocks = None
            return
        if tag == "tr":
            if self.table is not None:
                self.finish_row()
                self.row = []
            return
        if tag in ("td", "th"):
            if self.table is not None:
                self.finish_cell()
                self.cell = []
                self.cell_blocks = []
            return
        if tag in ("ul", "ol"):
            self.flush_para()
            self.flush_item()
            self.list_items = []
            return
        if tag == "li":
            self.flush_item()
            self.list_item = []
            return
        if tag == "br":
            self.add_break()
            return
        if tag == "hr":
            self.flush_para()
            return
        if tag in ("p", "div", "center", "blockquote"):
            self.flush_para()
            return
        if tag in ("tt", "code"):
            self.inline["code"] += 1
            return
        if tag in ("b", "strong"):
            self.inline["bold"] += 1
            return
        if tag in ("i", "em"):
            self.inline["italic"] += 1
            return
        if tag in INLINE_TAGS:
            return

    def handle_endtag(self, tag: str) -> None:
        tag = tag.lower()
        if tag in ("script", "style"):
            if self.skip:
                self.skip -= 1
            return
        if self.skip:
            return

        if self.pre is not None:
            if tag == "pre":
                self.finish_code()
            return

        if tag in HEADING_TAGS:
            self.flush_heading()
            return
        if tag == "table":
            self.finish_table()
            return
        if tag == "tr":
            self.finish_row()
            return
        if tag in ("td", "th"):
            self.finish_cell()
            return
        if tag in ("ul", "ol"):
            self.flush_item()
            if self.list_items:
                self.blocks.append({"kind": "list", "items": self.list_items})
            self.list_items = None
            return
        if tag == "li":
            self.flush_item()
            return
        if tag in ("p", "div", "center", "blockquote"):
            self.flush_para()
            return
        if tag in ("tt", "code"):
            self.inline["code"] = max(0, self.inline["code"] - 1)
            return
        if tag in ("b", "strong"):
            self.inline["bold"] = max(0, self.inline["bold"] - 1)
            return
        if tag in ("i", "em"):
            self.inline["italic"] = max(0, self.inline["italic"] - 1)
            return

    def handle_data(self, data: str) -> None:
        if self.skip:
            return
        self.add_text(data)

    def close(self) -> None:
        super().close()
        self.flush_para()
        self.flush_heading()
        self.flush_item()
        self.finish_table()


def page_fragment(raw: str) -> str:
    top = raw.find("<!--top jump end-->")
    bottom = raw.find("<!--bottom jump start-->")
    if top != -1:
        start = top + len("<!--top jump end-->")
        end = bottom if bottom != -1 and bottom > start else len(raw)
        return raw[start:end]
    return raw


def page_title(raw: str) -> str:
    match = re.search(r"<title[^>]*>(.*?)</title>", raw, re.S | re.I)
    if not match:
        return ""
    title = html.unescape(strip_tags(match.group(1)))
    return normalize_space(title)


def extract_examples(blocks: list[dict]) -> list[str]:
    examples: list[str] = []

    def add(value: str) -> None:
        value = value.rstrip()
        if value and value not in examples:
            examples.append(value)

    for block in blocks:
        if block.get("kind") != "code":
            continue
        for line in block.get("text", "").splitlines():
            # J console transcripts indent input by three spaces.
            if line.startswith("   ") and line[3:].strip():
                add(line[3:])
    return examples[:80]


def clean_names(value: str) -> str:
    value = normalize_space(value)
    value = re.sub(r"\s*/\s*$", "", value)
    value = re.sub(r"\s*\u2022\s*$", "", value)
    value = re.sub(r"\(\s*\)", "", value)
    return normalize_space(value)


def parse_vocabulary(path: Path) -> tuple[list[dict], set[str]]:
    raw = path.read_text(encoding="utf-8", errors="replace")
    fragment = page_fragment(raw)
    entries: list[dict] = []
    seen: set[tuple[str, str]] = set()
    vocab_pages: set[str] = set()

    # Vocabulary entries live in table cells. Scan cells rather than the whole
    # page so navigation links are not mistaken for entries.
    for cell in re.findall(r"<td[^>]*>(.*?)</td>", fragment, re.S | re.I):
        anchors = list(
            re.finditer(r'<a href="(d[0-9a-z]+)\.htm"[^>]*>(.*?)</a>', cell, re.S | re.I)
        )
        if not anchors:
            continue

        for match in anchors:
            href = match.group(1)
            link_text = normalize_space(html.unescape(strip_tags(match.group(2))))
            if link_text and link_text not in ("<<", ">>"):
                vocab_pages.add(href)

        # Build a fallback name from the whole cell. The symbol texts are
        # removed so that e.g. "&.  &.:  Under (Dual)" becomes "Under (Dual)".
        fallback = normalize_space(html.unescape(strip_tags(cell)))
        for match in anchors:
            symbol_match = re.search(r"<tt>(.*?)</tt>", match.group(2), re.S | re.I)
            if symbol_match:
                symbol_text = normalize_space(
                    html.unescape(strip_tags(symbol_match.group(1)))
                )
                if symbol_text:
                    fallback = fallback.replace(symbol_text, " ", 1)
        fallback = clean_names(fallback)

        for match in anchors:
            href, body = match.group(1), match.group(2)
            symbol_match = re.search(r"<tt>(.*?)</tt>", body, re.S | re.I)
            if not symbol_match:
                continue
            symbol = normalize_space(html.unescape(strip_tags(symbol_match.group(1))))
            if not symbol:
                continue

            names = normalize_space(
                html.unescape(strip_tags(body[symbol_match.end() :]))
            )
            names = clean_names(names) or fallback
            if not names:
                continue

            if "\u2022" in names:
                monad, dyad = names.split("\u2022", 1)
            else:
                monad, dyad = names, ""
            monad = clean_names(monad)
            dyad = clean_names(dyad)

            key = (href, symbol)
            if key in seen:
                continue
            seen.add(key)
            entries.append(
                {
                    "symbol": symbol,
                    "page": href,
                    "monad": monad,
                    "dyad": dyad,
                    "title": "",
                }
            )

    return entries, vocab_pages


def add_idioms(pages: list[dict]) -> None:
    idioms = [
        ("+/ y", "Sum the items of y.", "+/ 1 2 3 4"),
        ("*/ y", "Multiply the items of y (product).", "*/ 1 2 3 4"),
        ("+/\\ y", "Prefix sums of y.", "+/\\ 1 2 3 4"),
        ("+/\\. y", "Suffix sums of y.", "+/\\. 1 2 3 4"),
        ("# y", "Tally: number of items in y.", "# 3 1 2"),
        ("x # y", "Copy: repeat each item of y according to x.", "1 0 1 # 10 20 30"),
        ("#~", "Filter: keep items where the boolean mask is 1.", "(] #~ 2&|) 1 2 3 4"),
        ("/:~ y", "Sort y ascending (grade up, then from).", "/:~ 3 1 2"),
        ("\\:~ y", "Sort y descending.", "\\:~ 3 1 2"),
        ("~. y", "Nub: unique items of y, preserving order.", "~. 1 2 1 3"),
        ("~: y", "Nub sieve: mask marking the first occurrence of each item.", "~: 1 2 1 3"),
        ("|. y", "Reverse y.", "|. 1 2 3"),
        ("x |. y", "Rotate y left by x items.", "2 |. 1 2 3 4"),
        ("|: y", "Transpose y.", "|: 2 3 $ i. 6"),
        (",. y", "Ravel items: make each item a row.", ",. 1 2 3"),
        (",: y", "Itemize / laminate y.", "1 2 ,: 3 4"),
        ("< y", "Box y.", "< 1 2 3"),
        ("> y", "Open a boxed array.", "> < 1 2 3"),
        ("x { y", "From: select items of y by indices x.", "1 3 { 10 20 30 40"),
        ("i. y", "Integers from 0 to y-1.", "i. 5"),
        ("x i. y", "Index of y in x; returns the first matching position.", "10 20 30 i. 20"),
        ("i: y", "Steps from -y to y.", "i: 3"),
        ("I. y", "Indices where the boolean mask y is 1.", "I. 1 0 1 1"),
        ("x e. y", "Member: is each item of x in y?", "1 3 e. 1 2 3"),
        ("/: y", "Grade up: indices that sort y ascending.", "/: 3 1 2"),
        ("\\: y", "Grade down: indices that sort y descending.", "\\: 3 1 2"),
        ("+~ y", "Reflex: apply + with y on both sides (double y).", "+~ 3"),
        ("x -~ y", "Passive: reverse the arguments, computing y - x.", "2 -~ 5"),
        ("\"0", "Apply a verb to rank-0 cells (each scalar or box).", "+/\"0 1 2 3"),
        ("\"1", "Apply a verb to rank-1 cells (each row).", "+/\"1 2 3 $ i. 6"),
        ("[: f g", "Cap: compose f after g without a fork's extra argument.", "[: +/ *: 1 2 3"),
        ("0: y", "Constant function: always return 0.", "0: 99"),
        ("\". y", "Do / numbers: execute a string or convert characters to numbers.", "\". '1+2'"),
        ("\": y", "Format: convert a noun to a character representation.", "\": 1 2 3"),
        ("2 ^ 3", "Power.", "2 ^ 3"),
        ("2 % 4", "Divide.", "2 % 4"),
        ("! 5", "Factorial.", "! 5"),
    ]
    blocks: list[dict] = [
        {"kind": "heading", "level": 1, "spans": [span("Common J Idioms", bold=True)]},
        {
            "kind": "para",
            "spans": [
                span(
                    "A short, runnable cheat sheet. Press /help idioms or search for an idiom by name."
                )
            ],
        },
    ]
    examples: list[str] = []
    for expression, description, example in idioms:
        blocks.append({"kind": "heading", "level": 3, "spans": [span(expression, code=True, bold=True)]})
        blocks.append({"kind": "para", "spans": [span(description)]})
        blocks.append({"kind": "code", "text": "   " + example})
        if example not in examples:
            examples.append(example)
    pages.append(
        {
            "id": "idioms",
            "kind": "idiom",
            "title": "Common J Idioms",
            "url": f"{UPSTREAM}#readme",
            "symbols": [],
            "monad": "",
            "dyad": "",
            "blocks": blocks,
            "examples": examples,
        }
    )


def collect_pages(
    docs: Path, vocab_entries: list[dict], vocab_pages: set[str]
) -> list[dict]:
    by_page: dict[str, list[dict]] = {}
    for entry in vocab_entries:
        by_page.setdefault(entry["page"], []).append(entry)

    pages: list[dict] = []
    for directory, kind in (("dictionary", "concept"), ("phrases", "phrase")):
        folder = docs / directory
        if not folder.is_dir():
            continue
        for path in sorted(folder.glob("*.htm")):
            stem = path.stem
            if stem in ("contents", "title", "index", "vocabul"):
                continue
            raw = path.read_text(encoding="utf-8", errors="replace")
            title = page_title(raw) or stem
            is_primitive = directory == "dictionary" and stem in vocab_pages
            parser = ContentParser(is_primitive=is_primitive)
            parser.feed(page_fragment(raw))
            parser.close()
            blocks = parser.blocks
            if not blocks:
                continue
            page_kind = "primitive" if is_primitive else kind
            entries = by_page.get(stem, [])
            symbols = sorted({entry["symbol"] for entry in entries})
            monads = [entry["monad"] for entry in entries if entry["monad"]]
            dyads = [entry["dyad"] for entry in entries if entry["dyad"]]
            page_id = stem if directory == "dictionary" else f"phrase:{stem}"
            url = f"{HELP_BASE}/{directory}/{path.name}"
            pages.append(
                {
                    "id": page_id,
                    "kind": page_kind,
                    "title": title,
                    "url": url,
                    "symbols": symbols,
                    "monad": monads[0] if monads else "",
                    "dyad": dyads[0] if dyads else "",
                    "blocks": blocks,
                    "examples": extract_examples(blocks),
                }
            )

    add_idioms(pages)
    return pages


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, help="path to an extracted docs_help checkout")
    parser.add_argument("--commit", default=PINNED_COMMIT)
    args = parser.parse_args()

    if args.source:
        docs = args.source
    else:
        cache = Path.home() / ".cache" / "jive-docs"
        cache.mkdir(parents=True, exist_ok=True)
        archive = cache / f"docs_help-{args.commit}.tar.gz"
        if not archive.exists():
            print(f"downloading {TARBALL}", file=sys.stderr)
            request = urllib.request.Request(TARBALL, headers={"User-Agent": "jive-doc-generator"})
            with urllib.request.urlopen(request) as response, archive.open("wb") as handle:
                handle.write(response.read())
        temp = tempfile.TemporaryDirectory(prefix="jive-docs-")
        with tarfile.open(archive, "r:gz") as tar:
            tar.extractall(temp.name)
        roots = list(Path(temp.name).iterdir())
        docs = roots[0] if roots else Path(temp.name)

    vocab_path = docs / "dictionary" / "vocabul.htm"
    if not vocab_path.is_file():
        print(f"missing vocabulary page: {vocab_path}", file=sys.stderr)
        return 1

    vocab, vocab_pages = parse_vocabulary(vocab_path)
    pages = collect_pages(docs, vocab, vocab_pages)
    page_titles = {page["id"]: page["title"] for page in pages}
    for entry in vocab:
        entry["title"] = page_titles.get(entry["page"], entry["page"])

    payload = {
        "source": {
            "name": "jsoftware/docs_help",
            "commit": args.commit,
            "url": UPSTREAM,
            "license": "MIT",
            "generated": _dt.datetime.now(_dt.timezone.utc).isoformat(timespec="seconds"),
        },
        "pages": pages,
        "symbols": vocab,
    }

    DATA_DIR.mkdir(parents=True, exist_ok=True)
    with OUT_FILE.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, ensure_ascii=False, separators=(",", ":"))

    license_path = docs / "LICENSE"
    if license_path.is_file():
        (DATA_DIR / "LICENSE.docs_help").write_text(
            license_path.read_text(encoding="utf-8"), encoding="utf-8"
        )

    (DATA_DIR / "PROVENANCE.md").write_text(
        "# Bundled J Documentation\n\n"
        f"Source: {UPSTREAM}\n\n"
        f"Pinned commit: `{args.commit}`\n\n"
        "License: MIT (see `LICENSE.docs_help`).\n\n"
        "The content is converted from the official J help files by\n"
        "`tools/gen_help.py`. Regenerate with:\n\n"
        "```sh\n"
        "python3 tools/gen_help.py\n"
        "```\n",
        encoding="utf-8",
    )

    size = OUT_FILE.stat().st_size
    print(f"wrote {OUT_FILE} ({size / 1024:.1f} KiB, {len(pages)} pages, {len(vocab)} symbols)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
