#!/usr/bin/env python3
"""Gate the repository's Markdown: front matter, ADR shape, and dead links.

Run it through the task runner (`mise run docs`), which is how `mise run check`
reaches it. Exits 0 when every checked file passes, 1 otherwise, printing one
line per problem.

Three rules, in the order they are applied to a file:

1. **Front matter.** Every Markdown file carries a YAML header with `title`,
   `status` (`draft` | `active` | `superseded`) and `date` (`YYYY-MM-DD`) —
   except the tool-managed files listed in EXEMPT below.
2. **ADR shape.** `docs/adr/NNNN-<slug>.md` additionally carries
   `decision-makers`, and a `supersedes` entry (when present) must name an ADR
   that exists.
3. **Links.** Relative links and images must resolve on disk, and a `#anchor`
   must match a heading in the target file.

Stdlib only, deliberately: this gate runs in `check`, and a docs gate that
needs `pip install` is one that goes stale the first time somebody's venv is
cold.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Directories that hold no Markdown of ours.
SKIP_DIRS = {".git", "target", "node_modules"}

# Front-matter exemptions. Every entry here is a file some *tool* owns and
# rewrites; a rule a tool un-applies is a fight, not a rule.
#
#   tiles/*/README.md   include_str!-embedded into the .so and printed verbatim
#                       by `pwetty list`/`schema`. Front matter there is runtime
#                       noise in a CLI's output.
#   AGENTS.md CLAUDE.md carry `bd`-managed generated blocks.
#   .beads/ .agents/    written by `bd setup`.
#   antithesis/         the Antithesis property-discovery scratchbook, which
#                       carries its own front-matter schema (sut_path, commit,
#                       external_references) and regenerates wholesale.
#
# Everything else — README, CHANGELOG, docs/ — is ours and is gated.
EXEMPT_GLOBS = (
    "tiles/*/README.md",
    "AGENTS.md",
    "CLAUDE.md",
)
EXEMPT_DIRS = (".beads", ".agents", "antithesis")

# Files whose links we do not resolve: the scratchbooks reference absolute paths
# on the machine that generated them and sibling checkouts we do not vendor.
NO_LINK_CHECK_DIRS = (".beads", ".agents", "antithesis")

VALID_STATUS = {"draft", "active", "superseded"}
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
ADR_NAME_RE = re.compile(r"^\d{4}-[a-z0-9]+(?:-[a-z0-9]+)*\.md$")

# [text](target) and ![alt](target), skipping the ``code`` case by stripping
# fenced blocks and inline code first.
LINK_RE = re.compile(r"!?\[[^\]]*\]\(\s*([^)\s]+)")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
INLINE_CODE_RE = re.compile(r"`[^`]*`")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
HTML_ANCHOR_RE = re.compile(r"<a\s+(?:name|id)=[\"']([^\"']+)[\"']")


def markdown_files() -> list[Path]:
    out = []
    for p in REPO.rglob("*.md"):
        if any(part in SKIP_DIRS for part in p.relative_to(REPO).parts):
            continue
        out.append(p)
    return sorted(out)


def rel(p: Path) -> str:
    return str(p.relative_to(REPO))


def is_exempt(p: Path) -> bool:
    r = p.relative_to(REPO)
    if r.parts[0] in EXEMPT_DIRS:
        return True
    return any(r.match(g) for g in EXEMPT_GLOBS)


def links_checked(p: Path) -> bool:
    return p.relative_to(REPO).parts[0] not in NO_LINK_CHECK_DIRS


def split_front_matter(text: str) -> tuple[dict[str, str] | None, int]:
    """Return (fields, body_line_offset). `fields` is None when absent.

    A deliberately small YAML reader: `key: value` and `key:` followed by
    `- item` lines. Enough for the four keys the convention defines, and it
    fails loudly on anything it cannot read rather than guessing.
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != "---":
        return None, 0
    try:
        end = next(i for i in range(1, len(lines)) if lines[i].strip() == "---")
    except StopIteration:
        return None, 0

    fields: dict[str, str] = {}
    key = None
    for raw in lines[1:end]:
        if not raw.strip():
            continue
        if raw.lstrip().startswith("- ") and key:
            fields[key] = (fields[key] + " " + raw.strip()[2:]).strip()
            continue
        if ":" not in raw:
            continue
        key, _, value = raw.partition(":")
        key = key.strip()
        fields[key] = value.strip().strip("\"'")
    return fields, end + 1


def check_front_matter(path: Path, fields: dict[str, str] | None) -> list[str]:
    r = rel(path)
    if fields is None:
        return [
            f"{r}:1: missing YAML front matter "
            f"(needs title, status, date; see docs/documentation.md)"
        ]

    problems = []
    if not fields.get("title"):
        problems.append(f"{r}:1: front matter has no `title`")
    status = fields.get("status", "")
    if status not in VALID_STATUS:
        problems.append(
            f"{r}:1: front matter `status: {status or '(missing)'}` "
            f"is not one of {'|'.join(sorted(VALID_STATUS))}"
        )
    date = fields.get("date", "")
    if not DATE_RE.match(date):
        problems.append(
            f"{r}:1: front matter `date: {date or '(missing)'}` is not YYYY-MM-DD"
        )
    return problems


def check_adr(path: Path, fields: dict[str, str] | None) -> list[str]:
    r = rel(path)
    problems = []
    if not ADR_NAME_RE.match(path.name):
        problems.append(f"{r}: ADR filename must be NNNN-<slug>.md")
    if fields is None:
        return problems
    if not fields.get("decision-makers"):
        problems.append(f"{r}:1: ADR front matter has no `decision-makers`")
    superseded_by = fields.get("supersedes", "")
    if superseded_by and not (path.parent / superseded_by).exists():
        problems.append(
            f"{r}:1: `supersedes: {superseded_by}` names no ADR in {rel(path.parent)}/"
        )
    return problems


def slug(heading: str) -> str:
    """GitHub's heading-anchor slug, close enough for our own headings."""
    text = re.sub(r"`([^`]*)`", r"\1", heading)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)  # links -> their text
    text = re.sub(r"[*_~]", "", text)
    text = text.strip().lower()
    text = re.sub(r"[^\w\s-]", "", text)
    return re.sub(r"\s+", "-", text)


def anchors_of(path: Path) -> set[str]:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return set()
    found: set[str] = set()
    counts: dict[str, int] = {}
    in_fence = False
    for line in text.splitlines():
        if FENCE_RE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        found.update(HTML_ANCHOR_RE.findall(line))
        m = HEADING_RE.match(line)
        if not m:
            continue
        base = slug(m.group(2))
        n = counts.get(base, 0)
        counts[base] = n + 1
        found.add(base if n == 0 else f"{base}-{n}")
    return found


def body_lines(text: str, offset: int) -> list[tuple[int, str]]:
    """Body lines with 1-based numbers, fenced and inline code stripped."""
    out = []
    in_fence = False
    for i, line in enumerate(text.splitlines()[offset:], start=offset + 1):
        if FENCE_RE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        out.append((i, INLINE_CODE_RE.sub("", line)))
    return out


def check_links(path: Path, text: str, offset: int, anchor_cache) -> list[str]:
    problems = []
    r = rel(path)
    for lineno, line in body_lines(text, offset):
        for target in LINK_RE.findall(line):
            if re.match(r"^(https?:|mailto:|tel:|#!)", target):
                continue
            if target.startswith("#"):
                anchor = target[1:]
                if anchor and anchor.lower() not in anchor_cache(path):
                    problems.append(f"{r}:{lineno}: no heading for anchor `{target}`")
                continue
            file_part, _, anchor = target.partition("#")
            if not file_part:
                continue
            dest = (path.parent / file_part).resolve()
            if not dest.exists():
                problems.append(f"{r}:{lineno}: link target `{file_part}` does not exist")
                continue
            if anchor and dest.suffix == ".md":
                if anchor.lower() not in anchor_cache(dest):
                    problems.append(
                        f"{r}:{lineno}: `{file_part}` has no heading for anchor `#{anchor}`"
                    )
    return problems


def main() -> int:
    cache: dict[Path, set[str]] = {}

    def anchor_cache(p: Path) -> set[str]:
        if p not in cache:
            cache[p] = {a.lower() for a in anchors_of(p)}
        return cache[p]

    problems: list[str] = []
    gated = 0
    for path in markdown_files():
        text = path.read_text(encoding="utf-8")
        fields, offset = split_front_matter(text)
        exempt = is_exempt(path)

        if not exempt:
            gated += 1
            problems += check_front_matter(path, fields)
            if path.parent == REPO / "docs" / "adr":
                problems += check_adr(path, fields)
        else:
            # An exempt file has no front matter to skip past.
            offset = 0

        if links_checked(path):
            problems += check_links(path, text, offset, anchor_cache)

    if problems:
        for p in problems:
            print(p, file=sys.stderr)
        print(f"\ncheck-docs: {len(problems)} problem(s)", file=sys.stderr)
        return 1

    print(f"check-docs: ok ({gated} gated, {len(markdown_files()) - gated} tool-managed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
