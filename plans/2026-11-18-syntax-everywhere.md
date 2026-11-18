# Syntax colours everywhere in the F3 viewer and its editor

Status: phases A, B and D done and on main; phase C built, in review; phase L next (revision 4;
owner answers to Q1, Q2 and Q4 pending, defaults used) · Repo: navo · Branch: `plan/syntax-everywhere-20261118` · Base:
`origin/main@df7f558e` · Started: 2026-11-18

Not in scope:
- F4. It launches the external editor (`files.edit`, `crates/navo-ui/src/commands/keymap.toml:547`)
  and stays that way. This plan colours the F3 viewer and its built-in editor (`viewer.edit`,
  Ctrl+E, `keymap.toml:1133`). Orchestrator's decision, 2026-11-18.
- A Sidewise consumer of the line lexers. Compare's file diff still calls tree-sitter only
  (`crates/sidewise-ui/src/compare_diff/body/syntax.rs:28`). Deferred explicitly in D118
  (orchestrator's decision, 2026-11-18); a later plan adds it.
- New colours or roles, fold ranges for line lexers, a settings page for user languages, a
  language memory per extension, a change of which viewer F3 opens first (Q4).

Records D118. Supersedes P15 atom A4b (`plans/2026-10-15-p15-viewer.md:68`). Brings the colour
part of beta item B15 (`plans/2026-10-27-beta.md:1630`, moved to v2.0 by D85) forward. P15's D6
("paged text over 16 MB gets the rule lexer on the visible rows only") is built here.

Revision 3 answers Sol's second review of revision 2 (`017fd9b2`); its points 1–7 are closed by
name in the Log, as are the seven findings of the first review. Revision 4 answers the third
review (phase A of revision 3, `d915e744`); its points 1–7 are closed by name in the Log.

## Goal

Every text file gets calm colours in F3's Text view and in its built-in editor, on both sides of
the 16 MiB edit limit. A 300 MB ARXML colours its visible page while scrolling stays as fast as
Plain text. Logs and plain text show levels, numbers, dates, paths, URLs and GUIDs. Files with a
wrong or no extension are recognised by content. A2L, DBC, LDF, OIL, DCM, m4, TASKING `.lsl` and
`.map` get rules, and Notepad++ user languages load as they are.

## Decisions

Final for builders. D118 in `docs/decisions.md` records the owner's call; these are its parts.

- D1 — **Phases run one after another, in the owner's order.** Phase A (paged ARXML and JSON)
  ships alone: review RA, then VM proof VA. Phase B (log/text and the editor) starts after VA;
  phase D (content sniff) after RB; phase C (automotive rules and Notepad++) after D2; phase L
  (picker) after RC and the owner's Q1 answer. Only L0, a pure move, may run early.
- D2 — **Two engines, one choice.** Tree-sitter grammars (via `sidewise_kit::languages::detect`)
  stay in the viewer's editor, for files up to `EDIT_LIMIT` (16 MiB; `viewer/kinds.rs:28`).
  Everything else uses a *line lexer*: a scanner that colours text chunk by chunk from a small
  carried state. The paged view never runs tree-sitter and never reads bytes beyond the rows its
  fill already reads.
- D3 — **Choice order** (`choose`, atoms A10 and C10), first hit wins:
  0. `NAVO_SYNTAX=off` in the process environment → Plain (D17).
  1. The picker choice for this open file (`Plain` → no colours; a named language).
  2. A rule language by extension (user files beat built-ins). Added by C10.
  3. `detect(name, head)` is not `"text"`: the editor uses that grammar; the paged view maps it
     (`xml` → Xml; `json`, `jsonc`, `json5` → Json; any other grammar → Log).
  4. Otherwise Log. Until B1a lands, Log emits no spans.
- D4 — **Line lexers live in `sidewise-core`** (`crates/sidewise-core/src/lines/`), GPUI-free.
  Navo's own `navo_core` is a different crate (`crates/navo-core`) and has no `lines`; Navo code
  reaches the lexers only through the kit re-export `sidewise_kit::languages::lines` (A1k).
  Inside `sidewise-kit` and `sidewise-ui`, `navo_core` is the alias of `sidewise-core`
  (`crates/sidewise-kit/Cargo.toml:54`). Reason: the Linux Callgrind job
  (`.github/workflows/bench.yml:27`, valgrind only) cannot build GPUI. Rule languages
  (`navo-syntax`) are Navo-only and plug in through the same `LineLexer` trait.
- D5 — **Roles are D64's, no new colours.** Lexers emit only `lines::ROLES`, all of which are in
  `sidewise-kit/src/languages/captures.rs` `HIGHLIGHT_NAMES`; the contrast tests
  (`crates/sidewise-kit/src/tokens/syntax/tests.rs:77` and `:191`) hold them at 4.5:1 in both
  modes; B4 adds a guard test. Log mapping: ERROR/ERR/FATAL/CRITICAL/CRIT/FAIL/FAILED/FAILURE/
  PANIC/EXCEPTION → `title`; WARN/WARNING → `keyword`; INFO/NOTICE → `function`; DEBUG/TRACE/
  VERBOSE/DBG → `comment.doc`; dates and times → `attribute`; numbers and hex → `number`;
  double-quoted strings → `string`; single-quoted strings → `string.special`; URLs and paths →
  `link_uri`; GUIDs → `constructor`. `comment.doc` and `string.special` are in `lines::ROLES`
  and `HIGHLIGHT_NAMES`. Why the two extra roles: D7's exact-coverage rule binds only `comment`
  and `string`, and level words and single-quoted strings are short chunk-local tokens that may
  lose their colour at a cut.
- D6 — **Caps, all checked before allocation.**
  - Chunk: at most `MAX_SPANS` = 4096 spans; after that the lexer still advances its state but
    emits nothing. Span vectors start at `min(chunk.len() / 2, MAX_SPANS)` capacity.
  - Page: lexing stops after `MAX_PAGE_BYTES` = 1 MiB of row text or `MAX_PAGE_SPANS` = 65 536
    spans; later rows get no spans and no checkpoints.
  - Editor line: a line longer than `MAX_LINE` = 64 KiB (length from rope offsets, before any
    slice is made) gets no spans and keeps its incoming state.
    GPUI 0.9.1 itself skips visible lines over 10,000 bytes before it calls the adapter, so in
    the editor lines over 10,000 bytes stay plain; the 64 KiB cap is the adapter's own guard.
  - Editor call: one `styles` call returns at most 65 536 runs (the rest default).
  - Rule lexer: `Run::push` stops at 4096 tokens per line (`navo-syntax`, C6b).
- D7 — **Lexers stream across chunks.** A paged row holds at most 4 KiB
  (`crates/sidewise-core/src/paging/rows.rs:3`, cut at `:42`) and a row whose `next == end`
  continues the same logical line. `lex(chunk, state)` lexes any part of a line. `LineState`
  bits: 0–7 mode, 8–11 the byte length (0–8) of a carried *opening prefix*, 12–15 a carried
  *closing partial*, 16–23 flags (an open escape: `\` in a JSON or log string).
  - Opening prefixes, which may still grow into a longer delimiter: Xml `<`, `<!`, `<!-`, `<![`,
    `<![C` … `<![CDATA`; Json `/`. The lexer emits no span for these bytes; the next chunk
    colours the rest of the delimiter and what follows.
  - Closing partials: `-`, `--` in an Xml comment; `]`, `]]` in CDATA; `?` in a declaration;
    `*` in a Json block comment. Comment and declaration bytes are coloured at once (they are
    inside the construct either way); `]`, `]]` stay plain until `>` confirms the CDATA end.
  - `line_end(state)` is applied after a row that ends a line (`next > end`). It clears bits
    8–15 and the escape flag, then: Xml InTagName → InTag, InEntity → Text; Json InLineComment
    → Normal; every other mode carries over. So a `<` at a line end never joins `!--` on the
    next line, `--` at a line end never closes a comment with the next line's `>`, and a Json
    `//` comment ends with its line. An open Json string carries over (lenient display).
  - *Split safety*, checked by `lines::check_split` in every lexer's proptest: for any cut of a
    line into chunks at char boundaries, every chunked span lies inside a whole-line span of the
    same role, and every byte that the whole line colours `comment` or `string` is coloured the
    same when chunked. A short token cut by a chunk boundary (a name, a number, a level word)
    may lose its colour. Three named exceptions, built into `check_split`:
    - E1 — carried opening prefix: the `prefix_len()` bytes before a cut are exempt (plain
      chunked; `comment` whole when the prefix became `<!--`).
    - E2 — Json key role: a string whose `property` role depends on a `:` past the chunk end
      (the string is still open at the cut, or only spaces follow it in the chunk) is emitted
      `string`. `check_split` takes an `accept` list of (chunked, whole) role pairs; Json passes
      `[("string", "property")]`, every other lexer `[]`. This is the only place a chunked colour
      differs from the whole-line one, and both are string roles.
    - E3 — cap exhaustion: when any call (the whole line or a chunk) returns `MAX_SPANS` spans,
      every byte from the *horizon* on is exempt; the horizon is the smallest end offset (in line
      coordinates) of the last span of any capped call. Past the cap the state still advances,
      so the next chunk or row starts in the right mode.
  Rule lexers (`navo-syntax`) cannot carry partial delimiters: in the paged view a rule-language
  block delimiter cut by a 4 KiB row cut is missed (R6).
- D8 — **Paged state between pages.** A state cache maps a row's start offset to the lexer state
  at that offset. The fill stores a checkpoint at every lexed row start and one at `last.next`
  (the next page's top after a full-screen PageDown). The cache holds at most 16 384 entries
  (the oldest half is dropped when full). Its key is `(source_rev, lexer_rev)`. The first
  statement of `request_rows` resets the cache to its `want()` key, before the early return for
  an in-flight fill. So `set_lexer` during a fill, and a source change (`source_rev += 1` in
  `follow_changes`, `paged/mod.rs:280`, which `render` runs right before `request_rows`,
  `paged/view.rs:276-277`), reset it before any later fill starts; a `put` under another key is
  dropped, so the in-flight fill stores nothing. The lookup uses the fill's resolved top
  (`FillResult.top`, after queued moves), never `Want.top`. Top 0 starts from
  `LineState::default()`. A top in the cache starts from its state. A cold top (jump, Ctrl+End,
  go to line, several queued PageDowns) calls `LineLexer::resync(head)` with the first ≤ 64 KiB
  of the page text:
  - Xml: `-->` before any `<!--` → InComment; `]]>` before any `<![CDATA[` → InCData; a `<`
    followed by a name char, `/`, `!` or `?` before any terminator → Text; neither → `None`.
  - Json: `*/` before any `/*` → InBlockComment; else Normal. Log: always its default state.
  - `None` draws the page Plain and stores no checkpoints.
- D9 — **Stale colours are never shown.** `Want` carries `lexer_rev`. While a fill for a new
  lexer is pending, `RowCache::screen` still shows the old rows but marks them `colours: false`,
  and draw passes no syntax theme for them.
- D10 — **No syntax colours on the hit row and in contrast themes.** The hit row keeps
  `Theme::selected_content`; `is_contrast` keeps system text colours.
- D11 — **Editor work is bounded.** The adapter keeps a checkpoint (line state) every 64 lines, at
  most 65 536 checkpoints. A `styles` call lexes from the nearest checkpoint at or before the
  range; when that is more than 1 MiB before the range, it lexes from the line start nearest to
  64 KiB before the range with the default state and stores nothing.
- D12 — **One speed number, measured on Windows.** Same release build with `--features tester`
  (it reads `NAVO_LOG`), a fresh process per run, the 300 MB ARXML in the Text view (Alt+1):
  runs with colours against runs with `NAVO_SYNTAX=off` (D17).
  - Held PageDown for 10 s reaches at least 95 % of the Plain line number (median of 3 runs).
  - Peak working set is at most 16 MiB above Plain.
  - Fill-to-paint: p95 of `paint_us` (A9's debug line: from the fill request to the first paint
    of its rows) is at most 4 ms above Plain's p95. `fill_us + lex_us` on the same line is
    worker time only; VA reports it beside `paint_us` and does not gate on it.
  - The Callgrind budgets of A4b, B3 and C11b are hard limits in the bench code.
- D13 — **Content sniff** (only when the extension is unknown or ambiguous, as today): `<?xml`
  (today); a root `<` + ASCII letter or `_`, `<!--` or `<!DOCTYPE` → `xml`; JSON when the first
  non-space byte is `{` followed by optional space and `"` or `}`, or `[` followed by optional
  space and one of `{ [ " ] -`, a digit, or the whole words `true`, `false`, `null`. `[section]`
  stays text. A UTF-16 head (BOM `FF FE` or `FE FF`) is decoded with `String::from_utf16_lossy`
  from at most 4096 bytes before sniffing. `cdd`, `odx`, `odx-d`, `odx-c`, `odx-v`, `odx-f` and
  `odx-e` join the XML extensions; `pdx` is a ZIP container and stays binary.
- D14 — **Rule languages load at start, in the background (Q2).** Built-ins, then
  `%APPDATA%\Notepad++\userDefineLangs\*.xml` (XML only, read-only), then
  `<StoreDirs.config>\languages\*.toml` and `*.xml` (the resolved config folder, portable or
  not; later wins). A missing folder is silent; a folder error, a folder-entry error and a file
  error each become one `tracing::warn` with the path and the error. Files opened before loading
  ends use the built-ins; new files need a restart. Supported UDL subset (the existing importer,
  `crates/navo-syntax/src/udl/`): `UserLang` `name` and `ext`; `Global` `caseIgnored`;
  `Keywords1`–`8` with their `Prefix` flags (`udl/mod.rs:96`); `Operators1`–`2`; comments;
  delimiters. Styles and folding are ignored; colours always come from D64 roles.
- D15 — **Language picker (Q1).** Like the encoding picker: header button, Ctrl+Shift+L, View
  menu entry "Language…" routed through `op_of` and `run_view` like Encoding. Rows, top down:
  "Auto (detected: X)" where X is what `choose` gives with `Auto` (computed afresh, not read
  from the current body), "Plain text", "Log", a separator, then every kit grammar and every
  rule language, sorted, deduped. Menu IDs are `ItemId::Action(&'static str)`: `language.auto`,
  `language.plain`, `log`, kit names (`&'static`), and rule names borrowed from the
  `&'static Registry` that `rules()` returns (C7). The choice lives on `Viewer` and lasts for the
  open file (each open makes a new `Viewer`, `viewer/mod.rs:245`), across Text/Hex switches. A
  change reloads through the UNSAVED guard of `choose_encoding`.
- D16 — **Built-in automotive rules** (`navo-syntax`): a2l, dbc, ldf, lsl, map join m4, oil and
  dcm. D112 moves all of them into the automotive module once the module registry exists.
- D17 — **Support switch.** `NAVO_SYNTAX=off`, read once per process, turns every line lexer and
  grammar colour off (D3 step 0). It gives D12 its Plain baseline in the same build and the
  owner a way out before the picker lands.
- D18 — **F3 keeps opening Rich where it does today (Q4 default).** ARXML (Tree for XML at any
  size, `viewer/kinds/tree.rs:31,63`), JSON up to 64 MiB, MAP (`kinds/map.rs:29`), DBC and A2L
  (`kinds/description.rs:28,74`) open in their Rich view; colours show in the Text view (Alt+1,
  or Text in the View menu, Alt+V) and keep across switches. A13 and B7 test that path.

## Risks and rollback

- R1 — Slower paged scrolling. Lexing runs in the background fill (`paged/rows.rs`
  `request_rows`), never on the UI thread; A4b benches it with hard limits; VA measures D12.
  Rollback: `Highlight::paged_lexer` returns `None` (one line in A10), or `NAVO_SYNTAX=off`.
- R2 — Editor hitch on a jump in a 16 MiB file. Bounded by D6 and D11; B3 benches the 1 MiB
  catch-up. Rollback: `choose` returns `Plain` instead of `Line` for the editor.
- R3 — A broken user UDL. Errors are per file; the rest loads. Nothing is written to user folders.
- R4 — Wrong sniff. An extension and the picker always beat the sniff.
- R5 — Memory. The state caches are capped (D8, D11); VA measures peak working set.
- R6 — Rule languages over 16 MiB (a big A2L in Text view) can miss a block comment cut by a
  row cut, until the next delimiter. Accepted: display only, rare (one cut per 4 KiB).
- No data-safety risk: display only. The one new dependency use is `gungraun =0.20.0` (already in
  the workspace) as a dev-dependency of `sidewise-core` and `navo-syntax`.

## Components

| Component | Layer | Contract (in → out) | New / change | Test |
| --- | --- | --- | --- | --- |
| `line-contract` | sidewise-core | `LineLexer`, `Span`, `LineState`, `LineLanguage`, `ROLES`, caps, `check_split` | new | `lines::tests_contract` |
| `xml-lexer`, `json-lexer`, `log-lexer` | sidewise-core | `lex(chunk, state)`, `line_end`, `resync(head)` | new | `lines::tests_*` |
| `line-bench` | sidewise-core, navo-syntax | Callgrind benches with hard limits; IDs in `perf/speed.toml` | new | `cargo bench … --bench line_lexers` / `rule_lexer` |
| `kit-names` | kit | `lines` re-export, `known_languages`, `canonical` | new | `languages::names` tests |
| `row-colours` | sidewise-ui | `TextLine.spans` → shown runs → ordered `overlay` → `text_row`; test-support `run_probe` | change | `paged::syntax_runs::tests`, `run_probe` |
| `paged-lexing` | navo-ui | `fill_page`, `lex_page`, `StateCache`, `set_lexer`, `relex`, stale flag, paint log | new | `paged::syntax` tests, `viewer::syntax::tests`, `viewer::tests::paged_syntax` |
| `highlight-choice` | navo-ui | `choose(&Pick) → Highlight`, `Highlight::paged_lexer` | new | `viewer::syntax::tests` |
| `line-highlighter` | kit | `LineHighlighter: InputHighlighter`, `factory` | new | `languages::line_highlighter::tests` |
| `editor-colours` | navo-ui | `EditorFile.highlight` drives `read_only_state` | change | `viewer::tests::language` |
| `content-sniff` | kit | `detect` with root/JSON/UTF-16 sniff | change | `languages::tests::sniff` |
| `rule-sets` | navo-syntax | built-in TOML rules, token cap | change | `builtin::tests`, `lexer::tests` |
| `rules-state` | navo-ui, navo-syntax, navo-app | `load_udl_dir`, `RuleLexer`, `rules()`, `load_rules`, start-up thread | new | `registry::tests`, `syntax_rules` tests, `languages` tests |
| `language-picker` | navo-ui | `ChooseLanguage`, picker menu, View entry, header button | new | `viewer::tests::language_picker` |

## Atoms

DS = `api-builder` on DeepSeek via `dshq` (brief per `deepseek-briefs.md`). Sonnet = Sonnet 5.5.
Every check is `cargo xtask check -p <crate>` plus the named nextest filter; each new test is
shown red first. File lengths are at `df7f558e`. Every atom touches at most three files
(`Cargo.lock` and insta snapshots count) and about 150 changed lines, and leaves the workspace
building and green on its own.

### Phase A — big files (ARXML and JSON over 16 MiB)

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | streaming lexer contract | api-builder · DS | high | — | `nextest -p sidewise-core lines::` | done |
| A1k | kit: `lines` re-export, names API | api-builder · DS | medium | A1 | `nextest -p sidewise-kit languages::names` | done |
| A2a | Xml: text, tags, attributes, values | api-builder · DS | high | A1 | `nextest -p sidewise-core lines::` | done |
| A2b | Xml: comment, CDATA, declarations, resync | api-builder · DS | high | A2a | `nextest -p sidewise-core lines::` | done |
| A3a | Json: tokens, strings, escapes | api-builder · DS | high | A2b | `nextest -p sidewise-core lines::` | done |
| A3b | Json: comments, keys, resync | api-builder · DS | high | A3a | `nextest -p sidewise-core lines::` | done |
| A4a | `gungraun` dev-dependency + `Cargo.lock` | patcher · Sonnet | low | — | `cargo xtask check -p sidewise-core` | done |
| A4b | bench target, benches, speed IDs | api-builder · DS | medium | A4a, A3b | `cargo bench -p sidewise-core --bench line_lexers` + ID diff | done |
| A5 | `TextLine.spans` + shown mapping | api-builder · DS | high | A1 | `nextest -p sidewise-ui syntax_runs` | done |
| A6 | row colours, ordered overlay (+ Navo literal) | ui-builder · Sonnet | medium | A5 | `nextest -p sidewise-ui syntax_runs` + `xtask check -p navo-ui` | done |
| A6p | highlight-run probe (test support) | ui-builder · Sonnet | low | A6 | `nextest -p sidewise-ui --features test-support run_probe` | done |
| A7 | streaming `lex_page` in the fill | ui-builder · Sonnet | high | A1k, A2b, A6 | `nextest -p navo-ui paged::syntax viewer::tests::moves` | done |
| A8 | state cache, checkpoints, resync | api-builder · Sonnet | high | A7, A3b | `nextest -p navo-ui paged::syntax` | done |
| A9 | stale flag, theme threading, paint timing | ui-builder · Sonnet | medium | A8 | `nextest -p navo-ui paged::syntax viewer::tests` | done |
| A10 | `choose` (no rules yet) | api-builder · DS | high | A1k | `nextest -p navo-ui viewer::syntax` | done |
| A11 | paged load path | ui-builder · Sonnet | medium | A9, A10 | `nextest -p navo-ui paged::syntax viewer::tests` | done |
| A12 | relex after reuse and Follow | ui-builder · Sonnet | medium | A11 | `nextest -p navo-ui viewer::syntax viewer::tests::follow` | done |
| A13 | paged colour tests | ui-builder · Sonnet | medium | A12, A6p | `nextest -p navo-ui viewer::tests::paged_syntax` | done |
| RA | review of phase A | reviewer · Sol 6.1 | high | A13, A4b | written verdict, no must-fix open | done |
| VA | VM proof of phase A | tester · Hydrogen | high | RA | D12 numbers + screenshots | done |

Phase A is ready: every atom has closed inputs, A1 and A4a can start now, and the rest start
when the After column clears.

**Lint staging.** The gate runs clippy with `-D warnings` on all targets
(`xtask/src/gate/check.rs:37`), so an item whose first non-test caller lands in a later atom
carries a temporary expectation whose reason names the atom that removes it. rustc treats an
`expect(dead_code)` item as live, so only these entry points need one; any other dead item is a
stop. The `cfg_attr(not(test), …)` form is for items the atom's own tests call.

| Item | Added by | Attribute | Removed by |
| --- | --- | --- | --- |
| `syntax_runs` (sidewise-ui `paged/syntax_runs.rs`) | A5 | `#[cfg_attr(not(test), expect(dead_code, reason = "A6's row_highlights calls it"))]` | A6 |
| `PagedView::set_lexer` (`paged/syntax.rs`) | A7 | `#[cfg_attr(not(test), expect(dead_code, reason = "A11's PagedBody::set_lexer calls it"))]` | A11 |
| `choose` (`viewer/syntax.rs`; tests call `choose_with`) | A10 | `#[expect(dead_code, reason = "A11's Text::load calls it")]` | A11 |
| `Highlight::paged_lexer` (`viewer/syntax.rs`) | A10 | `#[cfg_attr(not(test), expect(dead_code, reason = "A11's Text::load calls it"))]` | A11 |
| `LanguageChoice` (`Plain`, `Named` built only by tests) | A10 | `#[cfg_attr(not(test), expect(dead_code, reason = "L3b's choice_of builds Plain and Named"))]` | L3b |

### Phase B — log/text and the editor (after VA)

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| B1a | Log: scanner, strings, words, levels | api-builder · DS | high | VA | `nextest -p sidewise-core lines::` | done |
| B1b | Log: hex and numbers | api-builder · DS | high | B1a | `nextest -p sidewise-core lines::` | done |
| B2a | Log shapes: URL, path, GUID | api-builder · DS | high | B1b | `nextest -p sidewise-core lines::` | done |
| B2b | Log shapes: date, time | api-builder · DS | high | B2a | `nextest -p sidewise-core lines::` | done |
| B3 | Log and editor benches + IDs | api-builder · DS | medium | B2b | `cargo bench -p sidewise-core --bench line_lexers` | done |
| B4 | editor adapter core | api-builder · Sonnet | high | VA | `nextest -p sidewise-kit line_highlighter` | done |
| B5 | `InputHighlighter` impl + factory | api-builder · Sonnet | high | B4 | `nextest -p sidewise-kit line_highlighter` | done |
| B6 | editor uses the choice | ui-builder · Sonnet | medium | B5 | `nextest -p navo-ui viewer::tests::language` | done |
| B7 | editor colour tests (F3 and Ctrl+E) | ui-builder · Sonnet | medium | B6, B1a | `nextest -p navo-ui viewer::tests::language` | done |
| RB | review of phase B | reviewer · Sol 6.1 | high | B7, B3 | written verdict | done |

### Phase D — content sniff (after RB)

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| D1a | UTF-16 head and markup root sniff | api-builder · DS | high | RB | `nextest -p sidewise-kit languages::tests` | done |
| D1b | JSON sniff | api-builder · DS | high | D1a | `nextest -p sidewise-kit languages::tests` | done |
| D2 | CDD/ODX as XML | patcher · Sonnet | low | D1b | `nextest -p sidewise-kit languages::tests` | done |

### Phase C — automotive rules and Notepad++ (after D2)

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| C1 | a2l rules | api-builder · DS | medium | D2 | `nextest -p navo-syntax builtin` | built (in review) |
| C2 | dbc rules | api-builder · DS | medium | C1 | same | built (in review) |
| C3 | ldf rules | api-builder · DS | medium | C2 | same | built (in review) |
| C4 | lsl rules | api-builder · DS | medium | C3 | same | built (in review) |
| C5 | map rules | api-builder · DS | medium | C4 | same | built (in review) |
| C6 | `load_udl_dir`, entry errors kept | api-builder · DS | medium | D2 | `nextest -p navo-syntax registry` | built (in review) |
| C6b | token cap in `Run::push` | api-builder · DS | medium | D2 | `nextest -p navo-syntax lexer` | built (in review) |
| C7 | `RuleLexer` + `rules()` state | api-builder · DS | high | C6b | `nextest -p navo-ui syntax_rules` | built (in review) |
| C8 | `load_rules` with folder policy | api-builder · DS | high | C6, C7 | `nextest -p navo-ui syntax_rules` | built (in review) |
| C9 | start-up thread in navo-app | api-builder · DS | medium | C8 | `nextest -p navo-app --lib languages` | building |
| C10 | `choose` step 2 (rules) | api-builder · DS | high | C7, C1 | `nextest -p navo-ui viewer::syntax` | building |
| C11a | `gungraun` dev-dependency of `navo-syntax` + `Cargo.lock` | patcher · Sonnet | low | D2 | `cargo xtask check -p navo-syntax` | built (in review) |
| C11b | rule lexer bench target, bench, speed IDs | api-builder · DS | medium | C11a, C5, C6b | `cargo bench -p navo-syntax --bench rule_lexer` + ID diff | built (in review) |
| C12 | rendered Oil test | ui-builder · Sonnet | medium | C10 | `nextest -p navo-ui viewer::tests::language` | blocked |
| RC | review of phases D and C | reviewer · Sol 6.1 | high | C9, C11b, C12 | written verdict | blocked |

### Phase L — language picker (after RC; Q1 default yes, pending)

| Atom | What | Role | Effort | After | Check | Status |
| --- | --- | --- | --- | --- | --- | --- |
| L0 | move `on_view`, `view_actions` to a sibling | api-builder · DS | medium | — | `nextest -p navo-ui viewer::tests::menu` + body diff | ready |
| L1 | every `Load` literal → `Load::new` | api-builder · DS | low | RC | `nextest -p navo-ui viewer::tests::open` | blocked (Q1) |
| L2a | `Load.language` + `in_language` | ui-builder · Sonnet | medium | L1 | `nextest -p navo-ui viewer::tests::open` | blocked (Q1) |
| L2b | `Viewer.language`, `Request.language` | ui-builder · Sonnet | medium | L2a | `nextest -p navo-ui viewer::tests` | blocked (Q1) |
| L3a | `ViewerBody::language` + overrides | ui-builder · Sonnet | medium | L2b | `nextest -p navo-ui viewer::tests` | blocked (Q1) |
| L3b | picker model (rows, IDs) | ui-builder · Sonnet | medium | L3a | `nextest -p navo-ui viewer::language` | blocked (Q1) |
| L3c | picker open, event, choose | ui-builder · Sonnet | medium | L3b | `nextest -p navo-ui viewer::tests` | blocked (Q1) |
| L4a | `ChooseLanguage` action + l10n | patcher · Sonnet | low | L3c | `nextest -p navo-ui commands` | blocked (Q1) |
| L4b | command entry + 2 snapshots | ui-builder · Sonnet | medium | L4a | `nextest -p navo-ui commands` | blocked (Q1) |
| L4c | key bind + 2 snapshots | ui-builder · Sonnet | medium | L4b | `nextest -p navo-ui commands` | blocked (Q1) |
| L5 | View entry, `op_of`/`run_view`, listener | ui-builder · Sonnet | medium | L4c, L0 | `nextest -p navo-ui viewer::tests::menu` | blocked (Q1) |
| L6 | header buttons | ui-builder · Sonnet | medium | L5 | `nextest -p navo-ui viewer::tests` | blocked (Q1) |
| L7 | picker tests | ui-builder · Sonnet | medium | L6 | `nextest -p navo-ui viewer::tests::language_picker` | blocked (Q1) |
| RL | review of phase L | reviewer · Sol 6.1 | high | L7 | written verdict | blocked |
| VB | final VM proof | tester · Hydrogen | high | RL (or RC if Q1 is no) | screenshots + import checks | blocked |

One cargo gate at a time on the build host. Shared files are edited in series (the After column
already orders them): `lines/mod.rs` (A1, A2a, A2b, A3a, A3b, B1a), `languages/mod.rs` (A1k,
B4), `builtin/mod.rs` and `builtin/tests.rs` (C1–C5), `viewer/syntax.rs` (A10, A11, A12, C10,
L2b, L3b), `paged/syntax.rs` (A7, A8, A9, A11), `paged/rows.rs` (A7, A8, A9), sidewise-ui
`paged/row_view.rs` and `paged/mod.rs` (A5, A6, A6p), `perf/speed.toml` (A4b, B3, C11b),
`viewer/kinds/text.rs` (A11, B6, L2a), `viewer/tests/language.rs` (B7, C12),
`languages/tests/sniff.rs` (D1a, D1b, D2).

### A1 — Streaming lexer contract

- **Files:** `crates/sidewise-core/src/lib.rs` (37; add `pub mod lines;`), new
  `src/lines/mod.rs`, new `src/lines/tests_contract.rs`.
- **Change:**
  ```rust
  pub type Span = (Range<usize>, &'static str);
  /// Opaque per-lexer state (D7): bits 0–7 mode, 8–11 carried opening-prefix length in bytes,
  /// 12–15 closing partial, 16–23 flags.
  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
  pub struct LineState(pub u32);
  impl LineState {
      /// Bytes of an opening delimiter carried past the chunk end (0–8; D7 E1).
      pub fn prefix_len(self) -> usize { ((self.0 >> 8) & 0xF) as usize }
  }
  pub const MAX_LINE: usize = 64 * 1024;
  pub const MAX_SPANS: usize = 4096;
  pub const MAX_PAGE_BYTES: usize = 1024 * 1024;
  pub const MAX_PAGE_SPANS: usize = 65_536;
  pub const ROLES: [&str; 15] = ["title", "keyword", "function", "comment", "attribute",
      "number", "string", "link_uri", "constructor", "tag", "punctuation.delimiter",
      "punctuation.bracket", "property", "boolean", "constant"];
  pub trait LineLexer: Send + Sync {
      /// Short name shown in the picker ("log", "xml", "a2l").
      fn name(&self) -> &str;
      /// Spans of one chunk of a line (no '\n'; a trailing '\r' is plain) and the carried state.
      fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState);
      /// The state after a line break; the default clears bits 8–15 (D7).
      fn line_end(&self, state: LineState) -> LineState { LineState(state.0 & !0xFF00) }
      /// A start state guessed from a cold page head; `None` draws the page Plain (D8).
      fn resync(&self, _head: &str) -> Option<LineState> { Some(LineState::default()) }
  }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum LineLanguage { Log, Xml, Json }
  impl LineLanguage {
      /// `xml` → Xml; `json`, `jsonc`, `json5` → Json; `log`, `text` → Log; else None.
      pub fn for_language(name: &str) -> Option<Self>;
  }
  impl LineLexer for LineLanguage { /* every variant: no spans, same state, until A2a/A3a/B1a */ }
  /// Checks the span contract of one chunk; used by every lexer's tests.
  pub fn check_spans(chunk: &str, spans: &[Span]) -> Result<(), String>;
  /// Checks D7 split safety: lexes `line` whole and cut at `cuts`, compares with E1–E3;
  /// `accept` lists (chunked, whole) role pairs allowed to differ (E2).
  pub fn check_split(lexer: &dyn LineLexer, line: &str, cuts: &[usize], state: LineState,
      accept: &[(&str, &str)]) -> Result<(), String>;
  ```
  Span contract: ordered, non-overlapping, non-empty, inside the chunk, on char boundaries, at
  most `MAX_SPANS`, roles from `ROLES`; gaps are plain. `check_split` lexes the line whole from
  `state`, then the chunks in order with the carried state (no `line_end` between chunks),
  shifts chunk spans to line offsets, marks exempt bytes (E1: the `prefix_len()` bytes before
  each cut, read from the state after the chunk before it; E3: every byte from the horizon on),
  and fails when a chunked span is not inside one whole-line span of the same role or an
  `accept` pair, or when a non-exempt byte the whole line colours `comment` or `string` has
  another role chunked. The error names the byte offset and both roles.
- **Check:** `for_language` table; `check_spans` rejects overlap, a span inside `é`, and an
  unknown role; with stub lexers, `check_split` fails a cut comment coloured wrongly, passes a
  3-byte carried prefix (E1), fails a chunked `string` under a whole-line `property` with
  `accept = []` and passes it with `[("string", "property")]` (E2), and passes a lexer capped at
  `MAX_SPANS` past its horizon (E3); the default `line_end` clears bits 8–15 only.
- **Stop if:** `lines/mod.rs` passes 230 lines (the check functions then move to a sibling in a
  follow-up atom).

### A1k — Kit: `lines` re-export and names API

- **Files:** `crates/sidewise-kit/src/languages/mod.rs` (33), `languages/names.rs` (111; `XML` at
  53, `resolve` at 99, `all()` at 109 under `#[cfg(test)]`).
- **Change:** `pub use navo_core::lines;` (in the kit, `navo_core` is `sidewise-core`).
  `all()` loses `#[cfg(test)]`. New `pub fn known_languages() -> Vec<&'static str>` (sorted,
  deduped) and `pub fn canonical(name: &str) -> Option<&'static str>` (ASCII lowercase, exact
  grammar name, then `resolve`), both re-exported from `mod.rs`. Tests inline in `names.rs`
  (`#[cfg(test)] mod tests`).
- **Check:** `canonical("JSON")` → `json`; `canonical("arxml")` → `xml`; `canonical("nope")` →
  None; the list contains `xml` and is sorted.
- **Stop if:** the kit's `navo-core` entry is not `sidewise-core` (`Cargo.toml:54`).

### A2a — Xml: text, tags, attributes, values

- **Files:** new `lines/markup.rs`, new `lines/tests_markup.rs`, `lines/mod.rs` (dispatch `lex`
  and `line_end` for `Xml`).
- **Change:** `pub(super) fn lex(chunk, state) -> (Vec<Span>, LineState)` and
  `pub(super) fn line_end(state) -> LineState`. Modes: 0 Text, 1 InTagName, 2 InTag,
  3 InValueDouble, 4 InValueSingle, 5 InEntity (unknown values act as 0; A2b adds 6 InComment,
  7 InCData, 8 InDecl). A chunk ending in `<` carries prefix length 1 and emits nothing for it
  (D7 E1); `</` is an end tag at once.
  - Text: `<` or `</` → `punctuation.delimiter`, then the name `[A-Za-z_:][A-Za-z0-9_:.-]*` →
    `tag` (mode 1 while the name runs past the chunk end); `&name;` / `&#N;` → `constant`.
  - InTag: attribute name → `attribute`; `=` → `punctuation.delimiter`; `"…"`/`'…'` → `string`
    (open at chunk end → 3/4); `>` or `/>` → `punctuation.delimiter`, mode 0.
  - `line_end`: clears bits 8–15; InTagName → InTag; InEntity → Text; other modes carry.
- **Check:** `<SHORT-NAME>Can</SHORT-NAME>`; `<A b="1">`; a value over two lines; a value cut
  mid-string by a chunk; `<` | `A>` cut (`<` plain, `A` `tag`); `<A` at a line end then
  `b="1">` (`b` is `attribute`); `<` at a line end then `A>` (plain: the prefix is cleared);
  proptest: `check_spans` with state carried over 5 lines, and `check_split` (`accept = []`) on
  random cuts.
- **Stop if:** `markup.rs` passes 200 lines.

### A2b — Xml: comments, CDATA, declarations, resync

- **Files:** `lines/markup.rs`, `lines/tests_markup.rs`, `lines/mod.rs` (dispatch `resync` for
  `Xml`).
- **Change:** `<!--` → `comment` through `-->` (else mode 6); `<![CDATA[` and `]]>` →
  `punctuation.delimiter`, content plain (mode 7); `<?` → `keyword` through `?>` (else 8); `<!`
  + letter → `keyword` through `>` (else 8). Opening prefixes `<!`, `<!-`, `<![`, `<![C` …
  `<![CDATA` carry their byte length and emit nothing (E1). Closing partials (D7): `-`, `--` in
  a comment are coloured `comment` at once; `]`, `]]` in CDATA stay plain until `>`; `?` in a
  declaration is `keyword` at once. `line_end` clears both partials; modes 6–8 carry.
  `pub(super) fn resync(head: &str) -> Option<LineState>` per D8.
- **Check:** a comment over two lines; `--` | `>` and `]]` | `>` cut by a chunk; `<!-` | `-x-->`
  (`<!-` plain, `-x-->` `comment`); `--` at a line end then `>` on the next line stays
  InComment; 5 000 `<a>` tags then `<!--` in one chunk return `MAX_SPANS` spans and end in
  InComment (E3); `resync("x --> <a>")` → InComment; `resync("<!-- a -->")` → Text;
  `resync("plain words")` → None; the A2a proptests over inputs with comments and CDATA.
- **Stop if:** `markup.rs` passes 320 lines (report; the block modes then move to a sibling in a
  follow-up atom).

### A3a — Json: tokens, strings, escapes

- **Files:** new `lines/json.rs`, new `lines/tests_json.rs`, `lines/mod.rs` (dispatch `lex` and
  `line_end` for `Json`).
- **Change:** modes 0 Normal, 1 InString; flag bit 16 Escape (a chunk ending after `\` in a
  string). Strings → `string`; numbers `-?digits(.digits)?([eE][+-]?digits)?` → `number`;
  `true`/`false` → `boolean`; `null` → `constant`; `{}[]` → `punctuation.bracket`; `:` `,` →
  `punctuation.delimiter`. `line_end` clears bits 8–15 and Escape; InString carries (an invalid
  multi-line string shows as one string).
- **Check:** `{"a": [1, -2.5e3, true, null]}`; a string over two lines; `"a\` | `"b"` cut after
  the backslash stays one string; `"a\` at a line end then `"b"`: the escape is cleared, so the
  first `"` closes the string; `check_spans`/`check_split` proptests (`accept = []`).
- **Stop if:** `json.rs` passes 200 lines.

### A3b — Json: comments, keys, resync

- **Files:** `lines/json.rs`, `lines/tests_json.rs`, `lines/mod.rs` (dispatch `resync` for
  `Json`).
- **Change:** `//` → `comment` to the line end (mode 3 InLineComment, carried across chunks;
  `line_end` → Normal); `/* */` → `comment` (mode 2 InBlockComment; a closing `*` partial is
  coloured at once); a `/` at a chunk end carries prefix length 1 (E1). Keys (E2): a closed
  string whose next non-space byte in the same chunk is `:` → `property`; a string still open at
  the chunk end, or followed only by spaces up to it, → `string`; otherwise `string`. The
  proptests pass `accept = [("string", "property")]`. `resync` per D8.
- **Check:** `"k" : "v"`; `"k"` | ` : 1` → `string` chunked, `property` whole, accepted; `/` |
  `/ c` (E1); `// c` cut into two chunks stays `comment` and the next line starts Normal;
  `/* c` then `*/ 1`; `*` | `/` cut; `resync("a */ b")` → InBlockComment; proptests as A3a.
- **Stop if:** `json.rs` passes 300 lines.

### A4a — `gungraun` dev-dependency of `sidewise-core`

- **Files:** `crates/sidewise-core/Cargo.toml` (`gungraun = "=0.20.0"` in `[dev-dependencies]`,
  as in `crates/navo-core/Cargo.toml:21`), `Cargo.lock`.
- **Change:** the dependency only; no `[[bench]]` yet, so bench discovery and the speed guard
  see nothing new.
- **Check:** `cargo xtask check -p sidewise-core`; `git diff Cargo.lock` adds only `gungraun` to
  `sidewise-core`'s dependency list (no new package: the workspace already locks 0.20.0). No
  test: no behaviour.
- **Stop if:** `Cargo.lock` gains a package.

### A4b — Xml/Json benches with hard limits, and their IDs

- **Files:** `crates/sidewise-core/Cargo.toml` (`[[bench]] name = "line_lexers" harness =
  false`), new `crates/sidewise-core/benches/line_lexers.rs`, `perf/speed.toml`.
- **Read:** `crates/navo-core/benches/rename.rs` (shape), `.github/workflows/bench.yml:47-123`.
- **Change:** the bench target, the bench and its manifest IDs land together, because discovery
  lists a `[[bench]]` target at once and the speed guard rejects measured IDs the manifest does
  not list. Inputs are built in code and lexed the way `lex_page` does: row chunks of ≤ 4096
  bytes with carried state. Each setup asserts the total span count is above a floor (so a cap
  or an empty lexer cannot pass). Group `line_lexers`, function `bench_page`, one
  `#[bench::<id>]` case each, each with
  `Callgrind::default().hard_limits([(EventKind::Ir, N)])` (gungraun 0.20.0 `src/common.rs:992`):
  - `arxml_typical`: 120 lines of about 90 bytes, floor 600 spans, N = 2 000 000;
  - `xml_dense`: one minified line cut into 120 rows of 4096 bytes, floor 30 000 spans,
    N = 25 000 000;
  - `json_dense`: the same for JSON, floor 30 000 spans, N = 30 000 000 (raised from 25 000 000 on 2026-11-18: RA fixed the fixture so all 120 rows are lexed, 491 KB and 55 308 spans; measured 26 501 604);
  - `xml_capped`: 300 rows of 4096 bytes of dense tags, asserts lexing stopped at
    `MAX_PAGE_BYTES`, N = 30 000 000.
  `perf/speed.toml` gets one `[[bench]]` per ID, `guarded = false`, after a comment naming this
  plan: `sidewise-core/line_lexers::line_lexers::bench_page.<id>` (the format of
  `perf/speed.toml:62`).
- **Check:** the bench runs under valgrind on Linux; quote the four counts. Red, then green: the
  sorted IDs the bench prints, compared (`diff`) with the sorted `sidewise-core/line_lexers` IDs
  in `perf/speed.toml`, list four missing IDs before the manifest edit and nothing after; quote
  both. `cargo xtask speed-targets --config perf/speed.toml --discover` lists `line_lexers`.
- **Stop if:** the bench needs a GPUI crate, a count passes its limit (report the hot function),
  or the printed ID format differs from `perf/speed.toml:62`.

### A5 — Spans on paged rows and the shown-text mapping (sidewise-ui)

- **Files:** `crates/sidewise-ui/src/window_view/viewer/paged/lines.rs` (150; `TextLine` at 17,
  its only literal at 99), new `paged/syntax_runs.rs` (tests inside), `paged/mod.rs` (9).
- **Change:** `TextLine` gets `pub spans: Vec<Span>` (`navo_core::lines::Span`), empty in
  `text_lines`. `pub(super) fn syntax_runs(text: &str, shift: usize, limit: usize, spans:
  &[Span]) -> Vec<(Range<usize>, &'static str)>` walks `text.chars()` exactly like
  `spans.rs:68` `shown_text` (skip `shift` chars, stop at `shift + limit`, a control char becomes
  one space) and returns each span's range in the shown string, clipped; empty results dropped.
  `syntax_runs` takes its row of the lint staging table.
- **Check:** ASCII; a shift that cuts a span; U+0085 before a span; a limit inside a span; `é`
  before a span.

### A6 — Row colours (sidewise-ui, plus Navo's literal)

- **Files:** `paged/row_view.rs` (268; `TextLook` at 25, `text_row` at 212, `with_highlights` at
  257), `paged/syntax_runs.rs`, `crates/navo-ui/src/window_view/viewer/paged/draw.rs` (188; the
  only `TextLook` literal at 62 gets `syntax: None`).
- **Change:** `TextLook` gets `pub syntax: Option<Arc<HighlightTheme>>`. In `syntax_runs.rs`:
  - `fn overlay(syntax: &[(Range<usize>, HighlightStyle)], paints: &[(Range<usize>,
    HighlightStyle)]) -> Vec<(Range<usize>, HighlightStyle)>` resolves overlaps in order: it
    cuts the shown string at every start and end of both lists (a sorted, deduped `Vec` of
    boundaries); a piece's style is the syntax style covering it (spans never overlap, so at
    most one), then each covering paint, in input order, replaces every field it sets
    (`color`, `background_color`, `font_weight`, `font_style`, `underline`, `strikethrough`,
    `fade_out`: `paint.x.or(base.x)`); pieces without a style are dropped. It does not use
    `gpui_kit::combine_highlights` (it folds a `HashSet`, so the order is not fixed) or
    `HighlightStyle::highlight` (it blends colours).
  - `row_highlights(look, text, line, is_hit) -> (String, Vec<(Range<usize>, HighlightStyle)>)`
    builds today's paint runs and, when `text.syntax` is set and D10 allows, the syntax runs
    (`syntax_runs`, styled by `HighlightTheme::style(name)`), merged with `overlay(syntax,
    paints)`. `text_row` calls it and stays under 50 lines. A6 removes A5's lint row.
- **Check:** the Hit paint over a `string` span gives exactly the hit colour (not a blend); a
  Match paint (no colour) keeps the syntax colour under the match background; of two
  overlapping paints the later one wins; the output is sorted and non-overlapping; no syntax
  run on the hit row or with `is_contrast`; `xtask check -p navo-ui` builds.
- **Stop if:** `row_view.rs` would pass 300 lines.

### A6p — Highlight-run probe at `with_highlights` (test support)

- **Files:** new `crates/sidewise-ui/src/window_view/viewer/paged/run_probe.rs` (tests inside),
  `paged/mod.rs` (`#[cfg(feature = "test-support")] pub mod run_probe;`), `paged/row_view.rs`.
- **Why:** GPUI test snapshots expose no highlight runs, so A13 needs its own observer of what
  `text_row` hands to `StyledText::with_highlights` (`row_view.rs:257`).
- **Change:**
  ```rust
  #[derive(Clone, Debug, PartialEq)]
  pub struct DrawnRow {
      pub start: u64, pub shown: String, pub runs: Vec<(Range<usize>, HighlightStyle)>,
  }
  /// Records one drawn row; keeps the newest 4096 (a thread-local `RefCell<VecDeque<DrawnRow>>`).
  pub fn record(row: DrawnRow);
  /// Drains and returns the recorded rows, oldest first.
  pub fn take() -> Vec<DrawnRow>;
  ```
  `text_row` calls `run_probe::record` under `#[cfg(feature = "test-support")]` with
  `line.start`, the shown string and a clone of the runs, right before `with_highlights`. Product
  builds do not have the feature (only dev-dependencies enable it: `navo-ui/Cargo.toml:64`,
  `sidewise-ui/Cargo.toml:11`); GPUI tests render on the test thread, so the thread-local sees
  every row.
- **Check:** `nextest -p sidewise-ui --features test-support run_probe`: two `record`s come back
  from `take` in order and a second `take` is empty; 5 000 records keep the newest 4096. Red
  first with `take` returning `Vec::new()`.
- **Stop if:** `row_view.rs` would pass 300 lines.

### A7 — Streaming `lex_page` in the fill (navo-ui)

- **Files:** new `crates/navo-ui/src/window_view/viewer/paged/syntax.rs` (tests inside),
  `paged/rows.rs` (212; `Want` at 9, `Drawn` at 17, `FillResult` at 27, `fill` at 134,
  `request_rows` at 165, `rows_filled` at 190), `paged/mod.rs` (321; `PagedView` at 43, `new` at
  80).
- **Change:**
  - `pub(super) struct PagedSyntax { lexer: Option<Arc<dyn LineLexer>>, rev: u64 }` (`Default`),
    one field `syntax` in `PagedView` (set in `new`), `mod syntax;`.
  - In `syntax.rs`: `pub(in crate::window_view::viewer) fn set_lexer(&mut self, lexer:
    Option<Arc<dyn LineLexer>>, cx: &mut Context<Self>)` on `PagedView`. It returns at once when
    both lexers are `None` or have the same `name()`; otherwise it stores the lexer, bumps `rev`
    and calls `request_rows`. It takes its row of the lint staging table.
  - `Want` gets `lexer_rev: u64` (from `syntax.rev`); its derived `PartialEq` makes a new lexer
    refill a filled page.
  - `fn lex_page(lexer: &dyn LineLexer, lines: &mut [TextLine], start: LineState) -> PageLex`
    with `PageLex { starts: Vec<(u64, LineState)>, end: (u64, LineState) }`: each row is one
    chunk, lexed in order with the carried state; `line_end` after a row with `next > end`;
    `starts` records the state at each `row.start`; `end` is the state at the last row's
    `next`; D6 page caps apply.
  - `pub(super) fn fill_page(source, want, moves, index, lexer: Option<Arc<dyn LineLexer>>) ->
    FillResult` in `syntax.rs` calls `fill` (now `pub(super)`, four parameters unchanged), then,
    on `Drawn::Text`, `lex_page` from `resync` of the page head (≤ 64 KiB of row text), keyed on
    the resolved `FillResult.top` (A8 adds the cache). `Drawn` and `FillResult` become
    `pub(super)`. The background closure of `request_rows` is exactly one `fill_page` call, so
    tests of `fill_page` test the fill path.
  - `#[cfg(test)] pub(super) fn text_spans(&self, index: usize) -> Vec<Span>` on `RowCache`
    (`rows.rs`; `filled` is private there) and `#[cfg(test)] pub(in crate::window_view::viewer)
    fn row_spans(&self, index: usize) -> Vec<Span>` on `PagedView` (`syntax.rs`).
- **Check:** in `syntax.rs`: rows `<!-` + `-x-` + `->` (one logical line, three chunks) colour
  `-x-->` as one comment (E1 leaves `<!-` plain); a two-line page carries Xml state; `end` equals
  the state at the next row; through the fill path, `text_window(cx, Setup::huge(..))` on
  `<A b="1">` rows, then `set_lexer(Some(Xml))` and a redraw give `row_spans(0)` a `tag` (red
  before the closure lexes); a second `set_lexer` with the same lexer adds no fill (`fills()`);
  `viewer::tests::moves` stays green.
- **Stop if:** `paged/mod.rs` would pass 330 lines.

### A8 — State cache, checkpoints, resync

- **Files:** new `paged/syntax/state.rs` (tests inside; declared by `mod state;` in
  `paged/syntax.rs`, so `paged/mod.rs` is not edited), `paged/syntax.rs`, `paged/rows.rs`.
- **Read, not edited:** `paged/mod.rs:280` (`source_rev += 1` in `follow_changes`) and
  `paged/view.rs:276-277` (`render` runs `follow_changes`, then `request_rows`). The reset below
  covers that source change without editing it.
- **Change:**
  - `pub(super) struct StateCache { key: (u64, u64), states: HashMap<u64, LineState>, order:
    VecDeque<u64> }` with `reset(key)` (clears only when `key` differs), `get(key, offset) ->
    Option<LineState>` (`None` under another key) and `put(key, offset, state)` (dropped under
    another key); caps per D8. `PagedSyntax` holds `Arc<Mutex<StateCache>>`; a poisoned lock
    reads through `into_inner`.
  - The first statement of `request_rows` is `self.syntax.reset_cache((want.source_rev,
    want.lexer_rev))` (a `PagedSyntax` method in `syntax.rs`), before its in-flight early
    return (D8).
  - `fill_page` takes `Option<Lexing>` instead of the bare lexer, with `Lexing { lexer, cache,
    key }` and `key` from the sent `Want`. Start state per D8: top 0 → `LineState::default()`;
    a cached top → its state; else `resync` (`None` → no spans, no puts). After lexing it puts
    every `starts` entry and `end` under `key`.
  - `#[cfg(test)]` `cache_key()` and `cache_len()` on `PagedView`.
- **Check:** tests call `fill_page`, the closure's body:
  - a page whose last row opens a comment, then `moves = [PageDown]`: the next page starts in
    InComment from the `end` checkpoint;
  - queued moves: `moves = [PageDown; 3]` with only page 1 cached land on an uncached top inside
    a comment, `resync` gives InComment, and the lookup used `FillResult.top`;
  - stale completion: the cache reset to `(0, 2)`, then `fill_page` with key `(0, 1)` draws its
    rows but `cache_len()` stays 0;
  - top 0 with the head `plain words` then `<a>`: `<a>` gets `tag` (default state, not resync);
  - 20 000 puts keep ≤ 16 384 entries; a cold page of plain words inside a huge comment draws
    Plain and puts nothing;
  - GPUI: `set_lexer(Xml)` then, before the executor runs, `set_lexer(Json)`: `cache_key()` is
    `(source_rev, 2)` as soon as the call returns; after `run_until_parked` it still is and
    `fills()` grew by 2. Bumping `source_rev` and calling `request_rows` with a fill pending
    moves `cache_key()` at once.
- **Stop if:** the fill must read bytes outside the page.

### A9 — Stale flag, theme threading and paint timing

- **Files:** `paged/rows.rs` (`Filled` at 22, `Screenful` at 46, `screen` at 101),
  `paged/draw.rs` (`rows` at 42, `content` at 127, `body` at 146 with its measuring `canvas` at
  154), `paged/syntax.rs`.
- **Change:**
  - `Screenful` gets `colours: bool` = `filled.want.lexer_rev == want.lexer_rev` (D9).
  - `body` (has `cx`) reads `cx.try_global::<KitTheme>()` and passes
    `Option<Arc<HighlightTheme>>` (None without a lexer or when `!screen.colours`) to
    `content(t, screen, syntax)` and `rows(t, screen, syntax)`, which sets `TextLook.syntax`. As
    in `crates/sidewise-ui/src/window_view/compare_screen/file/syntax.rs:31-33`.
  - Timing (D12): `request_rows` takes `Instant::now()` before it spawns; `fill_page` measures
    `fill_us` (around `fill`) and `lex_us` (around lexing) and returns them in `FillResult`;
    `rows_filled` stores `FillTiming { requested, fill_us, lex_us, rows, painted:
    Rc<Cell<bool>> }` in `Filled`. `body` hands the timing to the canvas paint callback, which,
    the first time it runs for that `Filled`, logs `tracing::debug!(target: "navo::paged",
    fill_us, lex_us, paint_us, rows)` with `paint_us = requested.elapsed()` in µs. One line per
    fill, at its first paint; Plain fills log it too (`lex_us` 0).
- **Check:** in `syntax.rs`: `colours` is false for a filled page of rev 1 under want rev 2;
  `FillTiming::paint_once()` returns the µs on its first call and `None` after;
  `viewer::tests` green.
- **Stop if:** `draw.rs` would pass 215 lines.

### A10 — Highlight choice (no rules yet)

- **Files:** new `crates/navo-ui/src/window_view/viewer/syntax.rs`, new `viewer/syntax/tests.rs`,
  `viewer/mod.rs` (331; `mod syntax;`).
- **Change:**
  ```rust
  pub(in crate::window_view::viewer) enum Highlight {
      Grammar(&'static str), Line(Arc<dyn LineLexer>), Plain,
  }
  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub(in crate::window_view::viewer) enum LanguageChoice { #[default] Auto, Plain, Named(String) }
  pub(in crate::window_view::viewer) struct Pick<'a> {
      pub name: &'a str, pub head: &'a [u8], pub choice: &'a LanguageChoice,
  }
  /// D3 with the D17 switch (`NAVO_SYNTAX`) read once per process (a `OnceLock`).
  pub(in crate::window_view::viewer) fn choose(pick: &Pick<'_>) -> Highlight;
  /// D3 without the switch; the tested core.
  fn choose_with(pick: &Pick<'_>, enabled: bool) -> Highlight;
  impl Highlight {
      /// The paged engine (D3 step 3): Line → its lexer; Grammar `xml` → Xml, `json`, `jsonc`,
      /// `json5` → Json, any other grammar → Log (`LineLanguage::for_language`); Plain → None.
      pub fn paged_lexer(self) -> Option<Arc<dyn LineLexer>>;
  }
  ```
  `choose` does not know the view: it returns the language; `paged_lexer` maps it for the paged
  view, and the editor (B6) uses `Grammar` as is. `Named(n)`: `n == "log"` → `Line(Log)`;
  `canonical(n)` is some → `Grammar`; else as Auto. Log is `LineLanguage::Log`.
  `Highlight::name()` comes with its first caller, L3b. `choose`, `paged_lexer` and
  `LanguageChoice` take their rows of the lint staging table.
- **Check:** table through `choose_with` and `paged_lexer`: `ecu.arxml` → Grammar `xml`, paged
  Xml; `big.json` → paged Json; `main.rs` → Grammar `rust`, paged Log; `x.dat` + `<?xml` →
  paged Xml; `Plain` → Plain, no paged lexer; `Named("JSON")` on `x.txt` → paged Json;
  `Named("log")` → Line `log`; `enabled: false` → Plain.
- **Stop if:** `choose` needs window or app state.

### A11 — Paged load path

- **Files:** `viewer/kinds/text.rs` (111; `TextLoad` at 15, `paged_text` at 22, `load` at 47,
  `PagedBody::new` at 92), `viewer/paged/syntax.rs`, `viewer/syntax.rs`.
- **Change:** `TextLoad::Paged(Paged, Option<Arc<dyn LineLexer>>)`. `paged_text(paged)` keeps
  its signature and builds `TextLoad::Paged(paged, None)`; its one caller (`open.rs:126`, the
  Follow load) is coloured by A12's `relex`, so `open.rs` is not edited. The paged branch of
  `load()` takes `choose(&Pick { name: sniff.name, head: sniff.head, choice:
  &LanguageChoice::Auto }).paged_lexer()`. `attach` calls the new `PagedBody::set_lexer(&self,
  lexer, cx: &mut App)`, written in `paged/syntax.rs` as an `impl PagedBody` block that forwards
  to `PagedView::set_lexer`. `PagedBody` stays a one-field tuple, so `kinds/hex.rs:45` and
  `archive/nested.rs:69` build it unchanged (no lexer). A11 removes the lint rows of
  `set_lexer`, `choose` and `paged_lexer`.
- **Check:** red, then green, in `paged/syntax.rs`: `big.arxml` through
  `tests::structured::fake_with("big.arxml", len)` and `hooks(Setup::huge(..))`: F3 opens the
  Tree (D18), Alt+1 opens paged Text, and `row_spans(0)` has `tag` and `attribute`. Red on A11's
  parent with only this test added (spans empty); quote both. `viewer::tests` green.
- **Stop if:** another `PagedBody` construction site exists.

### A12 — Relex after reuse and after Follow

- **Files:** `viewer/syntax.rs`, `viewer/syntax/tests.rs`, `viewer/modes.rs` (312; reuse at
  186-195, `follow_shown_from` at 301).
- **Change:** new `Viewer::relex(&self, cx: &mut App)` in `syntax.rs`: when
  `PagedBody::of(body)` is some, it calls `set_lexer` with `choose(&Pick { name, head } from
  self.sniff(), choice: Auto).paged_lexer()` (Auto until L2b). `modes.rs` calls it right after a
  successful `reuse` (a Hex-born body shown as Text gets colours) and in `follow_shown_from`
  before `follow_from_line` (editor → Follow). `Request::paged_text` keeps its signature: the
  Follow load stays lexer-free and `relex` colours it from the viewer's own name and head. The
  same-name rule of `set_lexer` (A7) makes a relex with an unchanged lexer free.
- **Check:** red on A12's parent with these tests added, green after; quote both:
  - Hex-first reuse: `big.arxml`, F3 (Tree), Alt+3 (Hex: `kinds/hex.rs:45` builds a lexer-free
    `PagedBody`), Alt+1 (reuse) → `row_spans(0)` has `tag`;
  - editor → Follow: a small `app.xml` (`Setup::new`), F3 (Tree), Alt+1 (editor), Alt+F → the
    paged Follow view, `row_spans(0)` has `tag`;
  - `viewer::tests::follow` stays green.
- **Stop if:** `modes.rs` would pass 320 lines, or Alt+F in the editor does not reach
  `follow_from_editor` (`modes.rs:268`).

### A13 — Paged colour tests

- **Files:** new `viewer/tests/paged_syntax.rs`, `viewer/tests.rs` (485; `mod paged_syntax;`
  beside the other test modules).
- **Change:** tests with `fake_with`, `Setup::huge` (`tests.rs:99`), `PagedView::row_spans`
  (A7) and the run probe (A6p):
  - F3 on `big.arxml` opens Tree (D18); Alt+1 shows Text with `tag`, `attribute`, `string`,
    `comment` on `<A b="1"><!-- c --></A>`;
  - rendering: after a redraw, `run_probe::take()` holds the row starting `<A b="1">` with a run
    styled `KitTheme::global(cx).highlight_theme.style("tag")` (the test app sets the global, as
    `window_view/tests/settings_state.rs:235` reads it); after `set_lexer(None)` and a redraw
    the same row has no syntax run;
  - Alt+3 then Alt+1 keeps spans; follow mode (`tests/follow.rs` helpers) keeps spans;
  - a comment cut by a 4 KiB row stays a comment; PageDown, PageUp, PageDown give the same
    spans on the same rows; Ctrl+End inside a comment colours it as a comment;
  - `big.bin` (binary) has none.
- **Check:** red: this atom's whole diff (the test file and its `mod paged_syntax;` line) applied
  in a scratch worktree at A11's parent, where lexers, cache, stale flag and probe exist but no
  load path sets a lexer: the tests compile and fail on empty spans. Green on HEAD. Quote both.
- **Stop if:** the probe records no row for the paged view (report; do not weaken to a model
  check).

### RA — Review of phase A

Sol 6.1 high reviews A1–A13 with A4a, A4b and A6p: split safety and its exceptions, UI-thread
work, caps, stale colours, cache keys and resets, the lint staging table. Must-fix items go to a
fresh narrow Sonnet fix atom.

### VA — VM proof of phase A

- **Tester:** Hydrogen, one release build of main after RA with `--features tester` (only that
  feature reads `NAVO_LOG`, `crates/navo-app/src/logging/mod.rs:12`), synthetic files made on the
  VM by a PowerShell script in a folder named `Prüfung`: a 300 MB ARXML (repeated
  `<ECUC-CONTAINER-VALUE>` blocks with comments, CDATA, attributes, CRLF) and a 200 MB minified
  JSON.
- **Steps:** F3 opens the Tree first (D18: Rich stays the default until the owner answers Q4);
  Alt+1; screenshots in light and dark. For D12: 3 fresh processes with colours and 3 with
  `NAVO_SYNTAX=off`, each with `NAVO_LOG=navo::paged=debug`: note the `paint_us` of the first
  line after Alt+1 (the first paint of the Text view), hold PageDown 10 s, note the line number
  and `(Get-Process navo).PeakWorkingSet64`, and take from the log the p95 of `paint_us` (fill
  request to first paint: the gated D12 number) and of `fill_us + lex_us` (worker time only,
  reported beside it, not gated). Then Ctrl+End and PageUp inside a comment.
- **Pass:** all three D12 numbers; colours in both themes; the comment after Ctrl+End coloured.

### B1a — Log: scanner, strings, words, levels

- **Files:** new `lines/log.rs`, new `lines/tests_log.rs`, `lines/mod.rs` (dispatch).
- **Change:** `pub(super) fn lex(chunk: &str, state: LineState) -> (Vec<Span>, LineState)`, a
  byte scanner. State: InDoubleString and Escape flags (carried across chunks), InWord (a chunk
  that starts inside a word starts no token); `line_end` clears all. A token starts only at a
  boundary (line start, or after a byte that is not ASCII alphanumeric or `_`). A word that
  matches nothing is skipped whole; a word is `[A-Za-z0-9_]+` joined by single dots
  (`v1.2.3`, `file.txt`). Any other char advances by its UTF-8 length.
  - `"` string up to the next `"` not after `\`; unclosed at line end → plain quote (the
    string state is dropped by `line_end`).
  - `'` string only at a boundary, closed by a `'` followed by a non-word byte (or the line end
    under `lex_last`), at most 200 bytes, and never across a `"`. A candidate that is still
    open at a chunk end is carried (byte count in state bits 24–31, `PENDING` when the chunk
    ends on the `'`). A candidate with no close is dropped and its bytes are lexed plain; past
    200 bytes the lexer skips to the next whitespace. Log also overrides `lex_last` (the chunk
    end is a boundary) and is `line_local`. In plain `lex`, a token with no whitespace after it
    in the chunk is unresolved: no span, and a carried skip mode runs to the next whitespace
    or to a quote that may open a string, so double-quoted strings keep exact coverage.
  - level word: D5 upper-case lists; lower-case or Capitalised forms only before `:`, `]` or `|`.
- **Check:** `"[ERROR] x"`; `"error: x"` title; `"errors found"` nothing; `"don't stop 'now'"` →
  `'now'`; `"v1.2.3"` nothing; a CRLF line; `check_spans`/`check_split` proptests.
- **Stop if:** a function passes 50 lines or `log.rs` passes 200.

### B1b — Log: hex and numbers

- **Files:** `lines/log.rs`, `lines/tests_log.rs`.
- **Change:** hex `0x`/`0X` + hex digits, and bare hex of exactly 8 or 16 digits with at least one
  digit and one `a-f`/`A-F`, ending at a boundary → `number`; number: optional `-` at a
  boundary, `digits(.digits)*`, optional `e[+-]digits` → `number`; letters right after (`12ms`)
  stay plain.
- **Check:** `"code 0x1F at 12ms"`; `"1.2.3"` one number; `"id 7FFE12AB ok"` number;
  `"id DEADBEEF ok"` nothing; `"café 42"` → `42`; proptests.
- **Stop if:** `log.rs` passes 260 lines.

### B2a — Log shapes: URL, path, GUID

- **Files:** new `lines/log_shapes.rs`, new `lines/tests_log_shapes.rs`, `lines/log.rs` (one
  call into `log_shapes::shape(bytes, at) -> Option<(usize, &'static str)>`).
- **Change:** `shape` tries `url`, `path`, `guid` (each `fn(bytes: &[u8], at: usize) ->
  Option<usize>`, the match length), called between strings and hex.
  - URL: `http://`, `https://`, `ftp://`, `file://` (ASCII case-insensitive) up to whitespace or
    `"'<>)]}`; trailing `.,;:` dropped → `link_uri`.
  - Path: `X:\` or `X:/`; `\\` + word char; `/` + word char with another `/` later in the run.
    Ends at whitespace or `"'<>|*?`; trailing `.,:;)` dropped; non-ASCII letters stay inside →
    `link_uri`.
  - GUID: 8-4-4-4-12 hex, optional `{}`, boundary at both ends → `constructor`.
- **Check:** `"C:\\Work\\Prüfung\\ecu.arxml,"`, `"\\\\srv\\share x"`, `"/usr/lib/x.so"`, `"/x"`
  (no), `"see https://a.b/c."`, `"{6F9619FF-8B86-D011-B42D-00C04FC964FF}"`.

### B2b — Log shapes: date and time

- **Files:** `lines/log_shapes.rs`, `lines/tests_log_shapes.rs`.
- **Change:** `shape` also tries `date` and `time` (before `path`). Date `YYYY-MM-DD`,
  `YYYY/MM/DD`, `YYYY.MM.DD`, `DD.MM.YYYY`, `DD/MM/YYYY`; with `T` + time, one span →
  `attribute`. Time `H:MM` or `HH:MM`, optional `:SS`, optional `[.,]` + 1–9 digits, optional
  `Z`, `[+-]HH:MM` or `[+-]HHMM`, ending at a boundary → `attribute`. `log.rs` is not touched.
- **Check:** `"2026-11-18T09:15:02.123Z INFO"`, `"18.11.2026 09:15"`, `"2026/11/18"` (a date,
  not a path), `"12:5"` (no).

### B3 — Log and editor benches, IDs

- **Files:** `crates/sidewise-core/benches/line_lexers.rs`, `perf/speed.toml`.
- **Change:** `bench_page.log_typical` (120 lines of about 120 bytes, floor 400 spans)
  N = 2 000 000; `bench_page.log_dense` (120 rows of 4096 dense bytes, floor 20 000 spans)
  N = 25 000 000; `bench_catch_up.log_1mib` (1 MiB of lines lexed in a row, floor 50 000 spans)
  N = 60 000 000; `bench_cap.line_over_max` (one 1 MiB line rejected by its length) N = 50 000;
  and their IDs (`guarded = false`) in the same commit (A4b's rule).
- **Check:** bench under valgrind; quote the counts; the ID diff of A4b, red before the manifest
  edit and empty after.
- **Stop if:** a count passes its limit.

### B4 — Editor adapter core (kit)

- **Files:** new `crates/sidewise-kit/src/languages/line_highlighter.rs`, new
  `languages/line_highlighter/tests.rs`, `languages/mod.rs` (`pub mod line_highlighter;`).
- **Read:** gpui-base 0.9.1 `src/input/base/rope_ext.rs:93-203` (`line_start_offset`,
  `slice_line`, `lines_len`, `offset_to_point`).
- **Change:** `pub struct LineHighlighter { lexer: Arc<dyn LineLexer>, text: Rope, marks:
  RefCell<Vec<LineState>> }` (`marks[k]` = state at line `64 * k`; at most 65 536). Inherent
  `fn set_text(&mut self, text: &Rope, from_row: usize)` (clone; drop marks after `from_row / 64`)
  and `fn spans(&self, range: &Range<usize>) -> Vec<Span>` (absolute, clipped, ordered) per D11
  and D6: a line's length comes from `line_start_offset(row + 1) - line_start_offset(row)`
  before `slice_line`; each line is one chunk followed by `line_end`.
- **Check:** coverage of a 3-line rope; a counting stub proves a second call lexes at most 63
  lines before the range; an edit at row 130 keeps marks 0–2; a range 2 MiB into a 3 MiB rope
  lexes ≤ 64 KiB + the range; a 1 MiB single line is never sliced (counting stub sees no call);
  70 000 one-char spans give 65 536 runs; every `ROLES` entry is in `HIGHLIGHT_NAMES` (D5).
- **Stop if:** a function passes 50 lines.

### B5 — `InputHighlighter` impl and factory

- **Files:** `languages/line_highlighter.rs`, `languages/line_highlighter/tests.rs`.
- **Read:** gpui-base 0.9.1 `src/input/editor/highlighting.rs:30-76`,
  `src/input/base/state.rs:1125-1139`, the test double at `state.rs:8948`.
- **Change:** `pub fn factory(lexer: Arc<dyn LineLexer>) -> InputHighlighterFactory` (any
  language name). `update` → `set_text(text, edit.start_position.row or 0)`; `styles` fills
  gaps with `HighlightStyle::default()`, unknown role → default, covers the range exactly;
  `fold_ranges` → empty; `language` → `lexer.name()`.
- **Check:** with a stub resolver, `styles` over `"ERROR x"` gives the `title` style on `ERROR`
  and default on the rest; runs cover the range.
- **Stop if:** `ensure_highlighter_factory` replaces an app factory (D2 would be wrong; report).

### B6 — Editor uses the choice

- **Files:** `viewer/editor/body.rs` (261; `EditorFile` at 19), `viewer/editor/mod.rs` (228;
  `read_only_state` at 97), `viewer/kinds/text.rs` (`EditorFile` built at 56).
- **Change:** `EditorFile.language` → `highlight: Highlight`, from `choose` (A10).
  `read_only_state(text, highlight: &Highlight, …)`: Grammar → `.language(g)`; Line →
  `.language(l.name())` then `set_highlighter_factory(LineHighlighter::factory(l.clone()), cx)`;
  Plain → `.language("text")`. The same state serves F3 and Ctrl+E (`start_editing`).
- **Check:** the two tests in `tests/language.rs` stay green.
- **Stop if:** `EditorFile` is built anywhere besides `text.rs:56`.

### B7 — Editor colour tests

- **Files:** `viewer/tests/language.rs` (43; `language_after_f3` at 10 asserts a grammar at 25).
- **Change:** a second helper without the grammar assertion. Tests: `app.log` →
  `language_name() == "log"` and the editor's drawn runs include the `title` colour on `ERROR`;
  Ctrl+E on `app.log` keeps `"log"` and the colour; `notes.txt` → `"log"`; F3 on a small
  `ecu.arxml` keeps its Tree default (D18) and Alt+1 shows the `xml` grammar.
- **Check:** red on the B5 commit (scratch worktree at that sha), green on HEAD; quote both.
- **Stop if:** drawn runs cannot be observed (report).

### D1a — UTF-16 head and markup root sniff

- **Files:** `crates/sidewise-kit/src/languages/sniff.rs` (90; `head_text` at 25, `content` at
  31, `is_xml` at 44), new `languages/tests/sniff.rs`, `languages/tests.rs` (196; `mod sniff;`).
- **Change:** `head_text` decodes a UTF-16 head (BOM) from at most 4096 bytes with
  `String::from_utf16_lossy`; `content` tries `is_markup_root` after the existing checks (D13).
- **Check:** `notes.dat` + `<AUTOSAR>` → xml; `<!-- c -->` → xml; `<134>Oct` → text; a UTF-16 LE
  `<?xml` head → xml; a UTF-16 BE one → xml; `app.log` + `started` → text.
- **Stop if:** an existing test in `tests.rs` turns red.

### D1b — JSON sniff

- **Files:** `languages/sniff.rs`, `languages/tests/sniff.rs`.
- **Change:** `is_json` per D13, tried after `is_markup_root`.
- **Check:** `config.dat` + `{ "a": 1 }` → json; `x.unknown` + `[1, 2]` → json; `[true]` →
  json; `[network]` → text; `[trueish]` → text.

### D2 — CDD and ODX as XML

- **Files:** `languages/names.rs` (`XML` at 53), `languages/tests/sniff.rs`.
- **Change:** add the seven D13 extensions to `XML`.
- **Check:** `ecu.cdd`, `a.odx-d` → xml; `a.pdx` → text.

### C1–C5 — Built-in rules, one language per atom

- **Files (each):** new `crates/navo-syntax/src/builtin/rules/<name>.toml`, `builtin/mod.rs`
  (19; `RULES`), `builtin/tests.rs` (85; list assertion at 23).
- **Change:** add the file to `RULES`, extend the sorted list assertion, one kind test.
  - **C1 a2l:** `//`, `/* */`; `"` strings with `\`; `/begin`, `/end` → keyword; PROJECT,
    HEADER, MODULE, MOD_PAR, MOD_COMMON, CHARACTERISTIC, MEASUREMENT, AXIS_PTS, AXIS_DESCR,
    COMPU_METHOD, COMPU_TAB, COMPU_VTAB, COMPU_VTAB_RANGE, RECORD_LAYOUT, FUNCTION,
    FUNCTION_LIST, GROUP, SUB_GROUP, SUB_FUNCTION, REF_CHARACTERISTIC, REF_MEASUREMENT,
    DEF_CHARACTERISTIC, IN_MEASUREMENT, OUT_MEASUREMENT, LOC_MEASUREMENT, IF_DATA, A2ML,
    MEMORY_SEGMENT, MEMORY_LAYOUT, ANNOTATION, ANNOTATION_TEXT, UNIT, FRAME, VARIANT_CODING →
    type; UBYTE, SBYTE, UWORD, SWORD, ULONG, SLONG, A_UINT64, A_INT64, FLOAT16_IEEE,
    FLOAT32_IEEE, FLOAT64_IEEE, VALUE, CURVE, MAP, CUBOID, ASCII, VAL_BLK, RAT_FUNC, TAB_VERB,
    TAB_INTP, TAB_NOINTP, IDENTICAL, FORM, LINEAR, NO_COMPU_METHOD, MSB_FIRST, MSB_LAST,
    FNC_VALUES, COLUMN_DIR, ROW_DIR, ECU_ADDRESS, BYTE_ORDER, FORMAT, PHYS_UNIT, EXTENDED_LIMITS,
    READ_ONLY, MATRIX_DIM, NUMBER, BIT_MASK, DISPLAY_IDENTIFIER → constant.
  - **C2 dbc:** `//`; `"` strings; VERSION, NS_, BS_, BU_, BO_, SG_, CM_, BA_DEF_, BA_DEF_DEF_,
    BA_, VAL_, VAL_TABLE_, SIG_VALTYPE_, BO_TX_BU_, EV_, ENVVAR_DATA_, SG_MUL_VAL_, SIG_GROUP_ →
    keyword; Vector__XXX → constant; operators `: | @ + - ( ) [ ] ,`.
  - **C3 ldf:** `//`, `/* */`; `"` strings; LIN_description_file, LIN_protocol_version,
    LIN_language_version, LIN_speed, Channel_name, Nodes, Master, Slaves, Signals,
    Diagnostic_signals, Frames, Sporadic_frames, Event_triggered_frames, Diagnostic_frames,
    Node_attributes, Schedule_tables, Signal_groups, Signal_encoding_types,
    Signal_representation, MasterReq, SlaveResp → keyword; logical_value, physical_value,
    bcd_value, ascii_value, delay, ms, kbps → type; operators `= ; { } , :`.
  - **C4 lsl:** `//`, `/* */`; `"` strings; `#include`, `#define`, `#if`, `#ifdef`, `#ifndef`,
    `#else`, `#endif`, architecture, derivative, processor, memory, bus, space, section_layout,
    group, select, reserved, stack, heap, copytable, section_setup, mau, map, size, type,
    priority, ordered, contiguous, fill, align, attributes, run_addr, load_addr, dest,
    src_offset, dest_offset, id, endianness, little, big, nvram, ram, rom → keyword.
  - **C5 map** (TASKING map report): `*` line comments; `|`, `+`, `-` operators; Section,
    Address, Size, Name, Symbol, Group, Chip, Space, Locate, Link, Result, Processed, Files,
    Overlay, Cross, References, Memory, Usage, Total → keyword. `.map` keeps its Rich viewer.
- **Check (each):** the list test red first, then green.
- **Stop if:** a word list needs a token kind outside `TokenKind`, or another built-in claims
  the extension.

### C6 — XML-only UDL folder load, entry errors kept

- **Files:** `crates/navo-syntax/src/registry/mod.rs` (90; `load_dir` at 26, entries dropped by
  `entry.ok()` at 31-34), `registry/tests.rs` (91).
- **Change:** one private walker shared by `load_dir` and the new `pub fn load_udl_dir(&mut self,
  dir: &Path) -> Vec<LoadError>` (only `*.xml`, case-insensitive, through `from_udl`). The walker
  turns each failed `read_dir` entry into a `LoadError` with the folder path instead of
  dropping it.
- **Check:** a folder with `a.xml`, `b.toml`, `c.txt` loads only `a`; a CRLF UDL loads; a UDL
  with `ext="foo bar"` claims both; a UDL in a folder named `Prüfung-ü` loads.
- **Stop if:** `load_dir`'s existing tests change.

### C6b — Token cap in `Run::push`

- **Files:** `crates/navo-syntax/src/lexer/spans.rs` (88; `push` at 23),
  `lexer/tests/mod.rs` (74).
- **Change:** `pub const MAX_TOKENS: usize = 4096;` `push` ignores tokens once `tokens.len() ==
  MAX_TOKENS`; the state at line end is unchanged.
- **Check:** a 20 000-word keyword line gives 4096 tokens; a block comment opened after the cap
  still carries its state to the next line.

### C7 — `RuleLexer` and the rules state (navo-ui)

- **Files:** `crates/navo-ui/Cargo.toml` (move `navo-syntax` from `[dev-dependencies]` line 62
  to `[dependencies]`), new `crates/navo-ui/src/syntax_rules.rs` (tests inside), `src/lib.rs`
  (59; `pub mod syntax_rules;`).
- **Change:** `static INSTALLED: OnceLock<Registry>`, `static BUILTIN: OnceLock<Registry>`;
  `pub fn install_rules(Registry) -> bool` (first call wins); `pub fn rules() -> &'static
  Registry` (installed, else built-in). `pub struct RuleLexer { name: &'static str, lexer:
  navo_syntax::Lexer }`, `RuleLexer::new(&'static Language)`; `impl LineLexer`: each chunk is
  one line for the rule lexer (R6); tokens → `(range, kind.highlight_name())`, clipped to
  `MAX_SPANS`; state `Normal` = 0, `Block(n)` = `0x1_0000 | n`, `Delimited(n)` = `0x2_0000 | n`,
  other values = `Normal`.
- **Check:** state round-trip; `rules()` before install has `oil`; `install_rules` with a `foo`
  language returns true, a second call false, and `rules().for_name("a.foo")` finds it.
- **Stop if:** `Cargo.lock` changes (`navo-syntax` is already in the graph), or `navo-ui` needs
  another crate.

### C8 — `load_rules` with the folder policy

- **Files:** new `crates/navo-ui/src/syntax_rules/load.rs` (tests inside), `syntax_rules.rs`
  (`mod load; pub use load::{RuleDir, load_rules};`).
- **Change:** `pub enum RuleDir { Udl(PathBuf), Own(PathBuf) }`; `pub fn load_rules(dirs:
  &[RuleDir]) -> (Registry, Vec<LoadError>)`: built-ins, then each folder in order. Per folder:
  `fs::metadata` → `NotFound` silent; any other error or not a folder → one `LoadError`; `Udl` →
  `load_udl_dir`, `Own` → `load_dir`.
- **Check:** with `tempfile`: precedence (`Own` `oil` beats `Udl` `oil` beats built-in); a
  missing folder silent; a file where a folder should be → one error; a folder named
  `Prüfung-ü` loads; a CRLF TOML and a CRLF UDL load.
- **Stop if:** `syntax_rules.rs` passes 200 lines.

### C9 — Start-up thread (navo-app)

- **Files:** new `crates/navo-app/src/gui/profile/languages.rs` (tests inside),
  `src/gui/profile.rs` (123; `mod languages;` and the call before `metrics::mark` at 32).
  `gui.rs` (399) is not touched.
- **Change:** `pub(super) fn install(dirs: Option<&StoreDirs>)`: `None` → return; a thread named
  `languages` runs `load_rules(&folders(&dirs.config, std::env::var_os("APPDATA").as_deref()))`
  then `install_rules`; each `LoadError` → `tracing::warn!(path = %e.path.display(), error =
  %e.error, "language file skipped")`; a failed spawn → one warn. Pure `fn folders(config:
  &Path, appdata: Option<&OsStr>) -> Vec<RuleDir>` gives
  `[Udl(appdata/Notepad++/userDefineLangs), Own(config/languages)]`; `Udl` only with `APPDATA`
  (Q2). `config` is the resolved `StoreDirs.config` (`crates/navo-store/src/dirs/mod.rs:8`),
  portable or not.
- **Check:** `folders` with and without `APPDATA`; with
  `StoreDirs::in_profile("Prüfung-ü")` the `Own` folder is `Prüfung-ü/config/languages`.
  The real Windows folders → VB.
- **Stop if:** `profile.rs` would pass 135 lines.

### C10 — `choose` uses rules

- **Files:** `viewer/syntax.rs`, `viewer/syntax/tests.rs`.
- **Change:** D3 step 2: `rules().for_name(name)` → `Line(RuleLexer)`; `Named(n)` tries
  `rules().get(n)` first.
- **Check:** `cfg.oil` → rule `oil` in both paths, and its lexer colours `TASK` as `keyword`;
  `x.a2l` → `a2l`; `Named("a2l")` on `x.txt` → `a2l`.

### C11a — `gungraun` dev-dependency of `navo-syntax`

- **Files:** `crates/navo-syntax/Cargo.toml` (`gungraun = "=0.20.0"` in `[dev-dependencies]`),
  `Cargo.lock`.
- **Change and check:** as A4a, for `navo-syntax`.

### C11b — Rule lexer bench and its IDs

- **Files:** `crates/navo-syntax/Cargo.toml` (`[[bench]] name = "rule_lexer" harness = false`),
  new `crates/navo-syntax/benches/rule_lexer.rs`, `perf/speed.toml`.
- **Change:** target, bench and IDs land together, as in A4b. Group `rule_lexer`:
  `bench_page.a2l` (120 lines, floor 500 tokens) N = 3 000 000; `bench_page.udl_dense` (a UDL
  with 8 keyword groups of 500 words, 120 rows of 4096 bytes, floor 20 000 tokens)
  N = 30 000 000; `bench_cap.udl_tokens` (one row of 20 000 keywords, asserts 4096 tokens)
  N = 5 000 000. One `[[bench]]` per ID in `perf/speed.toml`, `guarded = false`.
- **Check:** bench under valgrind; quote the counts; the ID diff of A4b, red before the manifest
  edit and empty after.

### C12 — Rendered Oil test

- **Files:** `viewer/tests/language.rs`.
- **Change:** F3 on `cfg.oil` → `language_name() == "oil"` and the drawn runs include the
  `keyword` colour on `TASK`.
- **Check:** red on the C9 commit, green on HEAD.

### RC — Review of phases D and C

Sol 6.1 high: sniff, importer error paths, token cap, start-up thread, rules state.

### L0 — Move `on_view` and `view_actions`

- **Files:** `viewer/view_menu.rs` (406; `on_view` 134-141, `view_actions` 143-176), new
  `viewer/view_menu/actions.rs`.
- **Change:** move both unchanged (`mod actions;`). `view_actions` keeps its reach: it is
  `pub(super)` in `view_menu.rs` (visible in `viewer`, called from `viewer/view.rs:93`), so in
  the sibling it becomes `pub(in crate::window_view::viewer)`. `on_view` stays private.
- **Check:** a structural red, then green, both quoted: `git grep -n -e 'fn on_view<' -e 'fn
  view_actions(' -- crates/navo-ui/src/window_view/viewer/` lists `view_menu.rs:134` and
  `view_menu.rs:143` before the move (red: both still live there) and exactly two lines after
  it, both in `view_menu/actions.rs`. Then the body check: `git show HEAD:<view_menu.rs> | sed -n
  134,176p` against the moved bodies; the `diff` shows only the visibility line (quote it);
  `viewer::tests::menu` green.
- **Stop if:** any other line changes.

### L1 — Every `Load` literal becomes `Load::new`

- **Files:** `viewer/open.rs` (the literal at 136 in `loader`), `viewer/tests/open.rs` (390;
  literals at 181 and 323).
- **Change:** `Load::new(hooks, location, sniff, cancel).in_encoding(encoding)` at all three.
- **Check:** `viewer::tests::open` green; `git grep -n "Load {"` finds only the struct.

### L2a — `Load.language`

- **Files:** `viewer/kinds.rs` (216; `Load` at 82, `new` at 92, `in_encoding` at 108),
  `viewer/kinds/text.rs`.
- **Change:** `Load.language: &'a LanguageChoice` (`new` sets a static `Auto`), `in_language`;
  `text.rs` passes it to `choose`.
- **Check:** `viewer::tests::open` green; behaviour in L7 through `Kinds`.

### L2b — `Viewer.language` and `Request.language`

- **Files:** `viewer/mod.rs` (`Viewer` at 99; `detected`, `reencoding` at 120-121),
  `viewer/open.rs` (`Request` at 109, `loader` at 129), `viewer/syntax.rs` (`relex` reads it).
- **Change:** `Viewer.language: LanguageChoice`, `Viewer.relanguage: Option<u64>`;
  `Request.language` copied from the Viewer and passed through `in_language`; the Follow view
  gets it from `relex` (A12).
- **Check:** `viewer::tests` green.
- **Stop if:** `viewer/mod.rs` would pass 345 lines.

### L3a — `ViewerBody::language`

- **Files:** `viewer/body.rs` (122; `encoding` default at 119), `viewer/editor/body.rs`,
  `viewer/paged/body.rs`.
- **Change:** `fn language(&self, _: &App) -> Option<SharedString> { None }`; `EditorBody`
  returns the editor's `language_name()`; `PagedBody` its lexer's name (`plain` without one).
- **Check:** `viewer::tests` green.

### L3b — Picker model

- **Files:** new `viewer/language.rs` (tests inside), `viewer/mod.rs` (`mod language;`),
  `viewer/syntax.rs` (`Highlight::name()`; drops the `LanguageChoice` lint row of A10).
- **Read:** `viewer/encoding.rs` (145; `row` and `encoding_picker`, the model).
- **Change:** `pub(super) fn language_picker(current: &LanguageChoice, auto: &str) -> ShellMenu`
  with the D15 rows and IDs; `auto` is `choose(Auto).name()`. `pub(super) fn choice_of(id:
  &str) -> LanguageChoice` maps IDs back.
- **Check:** rows in order (Auto, Plain text, Log, separator, sorted names), `xml` and a rule
  name present once, the checked row matches `current`, `choice_of` round-trips every ID.
- **Stop if:** `language.rs` passes 150 lines.

### L3c — Picker open, event and choose

- **Files:** new `viewer/language/menu.rs`, `viewer/language.rs` (`mod menu;`).
- **Change:** `open_language_menu`, `language_menu_event`, `choose_language` on `NavoWindow`,
  as in `encoding.rs` (`open_encoding_menu`, `encoding_menu_event`, `choose_encoding`): UNSAVED
  guard with `relanguage`, then `reload_kind`. A body without a language → note "Languages
  apply to the text view. Alt+1 switches to it.".
- **Check:** `viewer::tests` green; behaviour in L7.

### L4a — `ChooseLanguage` action and l10n

- **Files:** `viewer/keys.rs` (85; `ChooseEncoding` at 52),
  `crates/sidewise-l10n/locales/en-US/menu.ftl` (14).
- **Change:** `ChooseLanguage` in the actions list; `cmd-viewer-language = Choose the syntax
  colours`.
- **Check:** `commands` tests green (no snapshot changes yet).

### L4b — Command entry and its snapshots

- **Files:** `crates/navo-ui/src/commands/viewer.rs` (367; the encoding entry at 112-114),
  `commands/snapshots/command_registry.snap`, `commands/snapshots/palette_entries.snap`.
- **Change:** `("viewer::ChooseLanguage", "viewer.language", "cmd-viewer-language")`; the two
  snapshots accepted with `cargo insta` and reviewed: exactly one added line each.
- **Check:** `commands` tests green (`commands/tests.rs:276` and `:538`).
- **Stop if:** a test requires every command to have a key (report; L4b and L4c then merge).

### L4c — Key bind and its snapshots

- **Files:** `commands/keymap.toml` (2260; the encoding bind at 1248-1249),
  `commands/snapshots/bound_keys.snap`, `commands/snapshots/bound_order.snap`.
- **Change:** bind `ctrl-shift-l` to `viewer.language` right after the encoding bind; snapshots
  accepted and reviewed: one added line each.
- **Check:** `commands` tests green (`commands/tests.rs:284` and `:338`).
- **Stop if:** `ctrl-shift-l` is bound anywhere at the pin.

### L5 — View menu entry, routing and listener

- **Files:** `viewer/view_menu.rs` (`view_entries` at 42, the Encoding entry at 71, `op_of` at
  110, `run_view` at 194, `view_menu_event` at 345), `view_menu/labels.rs` (15; `ENCODING` at
  10), `view_menu/actions.rs` (from L0).
- **Change:** `LANGUAGE = "Language\u{2026}"`; `ViewOp::Language`; `op_of` maps
  `ChooseLanguage.name()`; `run_view` opens the picker like `ViewOp::Encoding`; the entry after
  Encoding when `has_encoding`; `on_view::<ChooseLanguage>(root, ViewOp::Language, cx)` in
  `view_actions`.
- **Check:** `viewer::tests::menu` green.

### L6 — Header buttons (mouse parity)

- **Files:** `viewer/language.rs` (`language_button`), `viewer/editor/view.rs` (225; encoding
  button at 107), `viewer/paged/view.rs` (300; `encoding_control` at 103).
- **Change:** ghost button with tooltip "Syntax colours (Ctrl+Shift+L)" dispatching
  `ChooseLanguage`, right of the encoding button in both headers.
- **Stop if:** `paged/view.rs` would pass 310 lines.

### L7 — Picker tests

- **Files:** new `viewer/tests/language_picker.rs`, `viewer/tests.rs` (`mod language_picker;`).
- **Change:** Ctrl+Shift+L, the View menu entry and a header-button click each open the picker;
  it lists Auto, Plain text, Log, xml, a2l; Plain on `app.log` → `text`; Log on `ecu.arxml` →
  `log`; the Auto label on a file switched to Log still names `xml`; on a huge file the choice
  survives Alt+3 then Alt+1; unsaved edits trigger the guard.
- **Check:** red on the L6 parent, green on HEAD.

### VB — Final VM proof

- **Tester:** Hydrogen, release build, synthetic files only: 40 MB and 2 MB `.log` (levels,
  dates, paths with `ü`, GUIDs, hex, CRLF), `notes.txt`, small `.a2l`, `.dbc`, `.ldf`, `.oil`,
  `.lsl`, `.map`, `.dcm`, `.m4`, `.cdd`, `.odx-d`, `notes.dat` holding XML, a UTF-16 `.xml` copy
  without extension; one UDL (CRLF) in `%APPDATA%\Notepad++\userDefineLangs` for `.foo`, one
  TOML in `<config>\languages` for `.bar` (config folder in a profile path with `ü`), a second
  UDL for `.bar` to prove precedence, and a broken UDL.
- **Steps:** F3 (Alt+1 where Rich opens) and Ctrl+E on each, light and dark; restart after adding
  the language files; the picker by key, View menu and button.
- **Pass:** colours on every file in both themes; `x.foo` coloured by the UDL, `x.bar` by the
  TOML; the broken file logged once as `language file skipped`; screenshots looked at.

## Done when

- Phase A: F3's Text view colours ARXML and JSON over 16 MiB; D12 holds in VA; RA has no open
  must-fix.
- F3 and Ctrl+E colour `.txt`, `.log`, unknown text, JSON, XML/ARXML/CDD/ODX, A2L, DBC, LDF,
  OIL, DCM, m4, LSL and MAP files (VB).
- Notepad++ UDLs and `<config>\languages` files colour their extensions after a restart (VB).
- The picker opens by key, View menu and header button (L7, VB).
- The full gate passes on main; Windows CI green; the bench job green with the hard limits.

## Open questions for the owner (answers pending; defaults used)

- **Q1 — Language picker (phase L).** Default: yes (D15). It is the only way to turn colours off
  or fix a wrong guess per file. If no, phase L drops (L0 stays as a tidy-up) and one small
  atom removes `LanguageChoice` and its lint row from `choose`.
- **Q2 — Read Notepad++'s own folder.** Default: yes, read-only, XML only (D14). If no, C6
  drops and C9's `folders` returns only `Own`.
- **Q4 — Rich or Text first for big files.** F3 opens Rich (tree/outline) for ARXML, A2L, DBC,
  MAP and 16–64 MiB JSON; colours apply in Text mode. Keep Rich as default, or open Text by
  default for big files? Recommended: keep Rich default; Alt+V to Text keeps colours (D18). If
  Text, one atom changes the scores in `kinds/tree.rs`, `map.rs` and `description.rs`.

Q3 (compare line colours) is closed: the orchestrator deferred the Sidewise consumer in D118.

## Log

- 2026-11-18 — Revision 1 (`a2bdf5d2`) written from the owner's call (order a, b, d, c).
- 2026-11-18 — Sol 6.2 review of revision 1: not ready. Revision 2 (`017fd9b2`) closed it:
  - Finding 1 (atoms over three files): ≤ 3 files per atom; one rule per atom; contract split
    from Log; L0 proves the move by a body diff.
  - Finding 2 (dependencies): kit re-export first; `Cargo.lock` in bench slices; serial shared
    files; A7's test in its filter; A13 asserts rendering.
  - Finding 3 (seams): kit-only access (D4); theme threaded through `draw.rs`; `Load` literals;
    `op_of`/`run_view`; real Log and Oil colour checks.
  - Finding 4 (paged state): split rows, a bounded state cache, cold resync, stale rejection.
  - Finding 5 (bounds and speed): caps, dense benches, one speed number, peak memory.
  - Finding 6 (detection and imports): Follow head, reuse relex, UTF-16 sniff, XML-only UDL,
    folder errors, non-ASCII and CRLF tests.
  - Finding 7 (goal gaps): F3 and Ctrl+E; phase A first; D118 no longer promises compare.
- 2026-11-18 — Sol 6.2 review of revision 2: not ready. Revision 3 closes it:
  - Point 1 (starts and dependencies): A7 needs A2b; the names API moves into A1k before A10;
    A2, A3, B1, B2, D1 and L3 are split to about 150 lines; phases run in order, gated on VA
    then each review (D1); only A1 and L0 are ready; L0 keeps `view_actions`' reach.
  - Point 2 (buildable commits): A6 sets `syntax: None` in Navo's only `TextLook` literal; A11
    keeps `Request::paged_text`'s signature and A12 changes it with its caller; `PagedBody` stays
    a tuple so `archive/nested.rs:69` builds; L1 turns all three `Load` literals into
    `Load::new`; A7's accessor is visible to viewer siblings.
  - Point 3 (page edges): D7 streams rows as chunks with delimiter and escape carry and a split
    safety check; D8 stores row checkpoints and one at `last.next`, looks up the resolved top,
    drops stale puts and draws Plain when `resync` returns `None`; A8 and A13 test split
    comments and CDATA, queued moves and stale fill completion.
  - Point 4 (real work): bench inputs are chunked like `lex_page` and assert a span floor;
    capped paths have their own benches; D6 checks lengths before slices and caps page, editor
    and rule-token output (C6b); D12 compares fresh processes of the same build with
    `NAVO_SYNTAX=off` (D17) and adds fill-to-paint latency.
  - Point 5 (executable checks): D15 fixes menu IDs (`&'static Registry` for rule names), a Log
    row and an Auto label computed afresh; L4 is split into three passing slices with their four
    snapshots; C12 renders Oil; A13, B7, C12 and L7 show red on a named earlier commit.
  - Point 6 (UDL and Windows): D14 names keyword prefix flags; C6 keeps folder-entry errors; C8
    and C9 test precedence, CRLF XML and non-ASCII paths through `StoreDirs.config`; VB adds
    CDD/ODX, DCM, m4 and `.txt`.
  - Point 7 (owner scope): Rich-first is Q4 with a default (D18) that A13 and B7 test; Ctrl+E
    replaces F4 by the orchestrator's decision; the Sidewise consumer is deferred in D118.
- 2026-11-18 — Third review (phase A of revision 3, `d915e744`): not ready. Revision 4 closes it,
  and marks phase A ready:
  - Point 1 (L0 proof): L0's check adds a structural red: `git grep` finds `on_view` and
    `view_actions` in `view_menu.rs` before the move and only in `view_menu/actions.rs` after.
  - Point 2 (split edges): D7 stores the carried opening-prefix length in `LineState` and builds
    three exceptions into `check_split`: E1 (a carried prefix such as `<!-` stays plain), E2 (a
    Json key whose `:` is past the chunk shows `string`), E3 (bytes past a cap horizon). It also
    defines `line_end`: partials clear, `//` comments and tag names end. A2a and A3a dispatch
    `line_end`; A2b and A3b add `lines/mod.rs` for the `resync` dispatch.
  - Point 3 (warning gate, paint order): a lint staging table names each temporary
    `expect(dead_code)` (A5, A7, A10) and the atom that removes it (A6, A11, L3b); `Highlight`
    loses the `paged` pick field and its unused `name()` (L3b adds it). A6's `overlay` resolves
    overlaps in input order with field-wise replacement, not `combine_highlights` (a `HashSet`
    fold) or `HighlightStyle::highlight` (it blends colours).
  - Point 4 (cache): `request_rows` resets the cache as its first statement, before the
    in-flight return; A8 names `paged/mod.rs:280` as read, not edited, because `render` runs
    `request_rows` right after it (`view.rs:276-277`); top 0 uses the default state; A8 tests
    queued moves and stale completion through `fill_page` (the closure's whole body) and the
    synchronous reset in a GPUI test.
  - Point 5 (bench order): A4a lands the dev-dependency and `Cargo.lock`; A4b lands `[[bench]]`,
    the bench and the four `perf/speed.toml` IDs together, and its red diffs the measured IDs
    against the manifest. A4s is gone; the same rule splits C11 into C11a and C11b and adds the
    ID diff to B3.
  - Point 6 (reds, observer): A11 (paged load), A12 (Hex-first reuse, editor → Follow) and A13
    (its whole diff with the module line, at A11's parent) each name a red. A6p adds a
    test-support probe at `with_highlights` that A13 reads. A12 colours the Follow view through
    `relex`, so `Request::paged_text` and `open.rs` keep their shape.
  - Point 7 (VM numbers): VA builds with `--features tester`, sets `NAVO_LOG`, notes the first
    paint and gates the p95 of `paint_us` (A9: fill request to first paint); `fill_us + lex_us`
    is reported as worker time only. F3 opens Rich first (D18) until the owner answers Q4.
- 2026-11-18 — A9–A13 built (Sonnet ui-builders) and integrated. RA (Sol 6.2 high): not ready, 6 must-fix + 1 should-fix — XML adjacent comments at a cut, bare CR stripped at chunk end, JSON number split, cold-resync line join, page byte cap off by one row, quadratic run overlay, five functions over 50 lines. All reproduced red and fixed (two Sonnet fix atoms + one split atom). RA round 2: 1–5 and 7 closed; new must-fix (json_dense fixture over the span cap, so the bench lexed only part of the page) and should-fix (JSON `/`|`1` and `1`|`-2` cuts) fixed red→green. Decisions (Manwë): `json_dense` limit 25 M → 30 M Ir (the full 491 KB page now measures 26.5 M); a JSON number tail cut at `.`/`e`/`e-` stays uncoloured (under-colouring at 4 KiB cuts only); a bare CR is XML content; the cold-jump open comment at EOF stays plain per D8. GHA bench (noise mode) proved the lexer benches deterministic; its `rename::bench_check` noise (0.74 % > 0.5 %) is outside phase A and not fixed. Next: VA in the VM.
- 2026-11-18 — VA (Hydrogen): step 1 PASS (Tree first, Text colours in both themes; screenshots checked by Manwë); steps 2–4 blocked by another job's wedged VM lock. Decision: run VA steps 2–4 with phase B in one VM build (VAB); phase A stays off main until D12 passes.
- 2026-11-18 — Phase B built: B1a–B3 by DeepSeek (dshq, one chained session), B4–B5 api-builder Sonnet, B6–B7 ui-builder Sonnet. B7 stopped correctly (editor runs not observable); decision: a `cfg(test)` recording factory (`editor/run_probe.rs`); grammar colours left to the VM proof.
- 2026-11-18 — RB (Sol 6.2 high): not ready, 4 must + 3 should — a word at the line end never coloured, split-safety holes hidden by narrowed proptest generators, `'` strings and DEBUG→comment breaking D7 coverage, run cap counting spans not runs; replay cost, GPUI's 10,000-byte gate, a weak Ctrl+E test. GHA Callgrind: log_dense 44.3 M > 25 M, log_1mib 94.6 M > 60 M. Decisions (Manwë): `LineLexer::lex_last` (a chunk that ends its line; default = `lex`) and `line_local()`; Log drops any token cut by a chunk end and skips to the next whitespace; **D5 change:** DEBUG/TRACE/VERBOSE/DBG → `comment.doc`, `'` strings → `string.special`; the editor keeps GPUI's 10,000-byte line gate (D6); a head with NUL and no UTF-16 BOM gets Plain. Fixes by two Sonnet high workers; GHA bench green (log_dense 14.1 M, log_1mib 30.0 M, log_typical 0.46 M). RB round 2: ship with fixes — the binary guard before Auto detection, a `lex_last` split mode in the proptests, an exact run count at the cap, a cold continuation row stays plain; all fixed red→green. Deferred: a bench of the editor adapter itself (RB2 point 5; GPUI cannot build in the Callgrind job).
- 2026-11-18 — god-mode idea: the flaky `navo-index` dense-pages heap test now reads a per-thread counter (`Probe::thread_retained`); it rides with this batch.
- 2026-11-18 — VAB (Hydrogen, one VM build of phases A and B): all 6 steps PASS, including the D12 speed numbers; log colours in dark and light and a plain `blob.bin` checked by Manwë on the screenshots. VA is done.
- 2026-11-18 — Phase D built: D1a, D1b and D2 by DeepSeek, reviewed by hand. D1a's content sniff broke three paged tests that set XML lexers; they now start from Log. Phases A, B and D land on main as one squash commit.
- 2026-11-18 — Phase C rules lane built: C11a (Manwë), C6b (DeepSeek, finished by Manwë after two older speed tests met the token cap), C6, C1–C5 and C11b (DeepSeek), each red then green. C11b instruction counts wait for the GHA bench. UI lane: C7 and C8 (DeepSeek) built; C8 waives the 200-line cap on `syntax_rules.rs` because its tests live inside.
