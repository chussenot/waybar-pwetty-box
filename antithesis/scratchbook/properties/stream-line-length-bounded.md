# stream-line-length-bounded

## What this is

The streaming content reader consumes producer stdout with
`BufReader::new(out).lines()` (src/content.rs:270), which reads bytes into a
freshly-allocated `String` until a `\n` — with **no length cap**. A producer
that emits a very long line, or never emits a newline at all, grows the host
(waybar) process memory without bound.

## The trust-boundary defect

Producer stdout is the data ingress and is untrusted-ish: it is the stdout of a
long-lived child (`sh -c <exec>`, src/content.rs:261-266) whose content
ultimately derives from window titles and backend state. The reader assumes
"stream stdout is one bounded, newline-delimited JSON doc per line" (SUT §10) —
each clause is independently violable:

- **Newline-less producer:** `.lines()` keeps reading into one `String` until
  EOF. A producer stuck mid-write (or deliberately withholding `\n`) makes that
  allocation grow to the size of everything written — unbounded host RAM
  (SUT §7: "No line-length cap → newline-less producer grows host memory
  unboundedly").
- **Huge single line:** a title/JSON blob far larger than expected is fully
  buffered before it can be parsed or rejected. Even the poll path
  (`Command::output()`, src/content.rs:288-296) buffers *all* stdout with no
  cap — pointing a never-exiting streaming producer at poll mode buffers
  forever (SUT §7).

Because the plugin runs **in-process inside waybar**, unbounded growth here is
unbounded growth of the whole bar: the classic unbounded-input DoS of the host.

## Code paths

- unbounded line read: src/content.rs:270-276
- stream spawn / respawn loop: src/content.rs:256-285
- poll path also uncapped (`Command::output`): src/content.rs:288-296
- SUT analysis flag: §7 (degraded states), §10 (unproven assumption), §12
  surface 8 (resource boundedness)

## Suggested assertion (net-new)

`Always`, in the stream reader loop after obtaining each `line`:
`line.len() <= LINE_CAP` for a fixed cap (e.g. 64 KiB — comfortably above a
realistic niri title, well below memory pressure). This documents the intended
bound; the real fix is a capped reader (`take(LINE_CAP).read_until(b'\n', …)`)
that truncates instead of growing. The assertion catches the unbounded case
under a fuzzing producer.

Cap calibration (measured 2026-07-22, see Investigation Log): the realistic
maximal tile-watch line is **~550 bytes** (2 sessions, long titles/folders);
the producer applies **no truncation** (a 6099-byte line passed through
whole), and JSON HTML-escaping inflates `<`/`>`/`&` 6×. A 64 KiB cap is
~120× headroom over realistic traffic while still catching a runaway producer
quickly.

## Open questions

- Does block-buffered producer stdio (4 KiB libc clumps, SUT §8) interact with
  the cap — i.e. can a partial line stall under the cap without ever
  completing? `(partial: not possible for the real tile-watch — Go's
  `os.Stdout.Write` is unbuffered, one direct write syscall per line
  (tile.go:529), no libc stdio; the question remains only for arbitrary
  non-Go `exec` producers using buffered stdio.)` Why it matters:
  distinguishes "bounded memory, delayed content" from "bounded memory,
  stalled tile"; both are acceptable vs the unbounded status quo, but the
  second overlaps the staleness surface.

### Investigation Log

#### What is the realistic maximum tile-watch line size vs PIPE_BUF (4096)?

2026-07-22:

- Examined: `/home/chussenot/agentic-db/internal/tile/tile.go` (Payload /
  SessionTile marshaling, RunWatch.emit single-write path); live measurements
  against the installed `claude-status tile-watch` with a scratch
  `--db`/`tiles.json` (crafted cache entries drive the exact production
  marshal path); `getconf PIPE_BUF /` = 4096.
- Found: empty placeholder 59B; realistic maximal (2 sessions, ~150-char
  browser-tab titles, ~35-char folders, all optional fields) **548B**. No
  length limit exists anywhere in the producer: 2×3000-char titles → a
  **6099B** line emitted whole; 800 `<` chars in one title → **4858B**
  (`json.Marshal` escapes `<`,`>`,`&` to 6-byte forms).
- Not found: niri-side title caps (out of scope) — but irrelevant to the cap
  choice: the plugin's contract is with any `exec` producer.
- Conclusion: resolved. Normal operation sits ~550B — nowhere near 4096 — so
  `LINE_CAP = 64 KiB` is comfortably safe (~120× realistic max) and the
  property's value is entirely about hostile/buggy producers, which remain
  able to emit unbounded lines today.
