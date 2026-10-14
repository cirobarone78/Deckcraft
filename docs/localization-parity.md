# Localization parity with PowerPoint

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-14 · **Change:** major (expanded Simplified Chinese interface, localized editor prompts and native file dialogs; user review pending) · **Target:** Microsoft PowerPoint for Mac 16.113 (Microsoft 365)

DeckCraft defaults to English and offers a **Simplified Chinese interface preview** under
File → Options → Interface language (or Ctrl/Cmd+,). Changes apply without restarting and the
desktop saves the choice with its existing UI preferences. The browser still does not persist
preferences. Unsupported interface languages are disabled. System-language detection (#15)
and other translations (#30, PR #34) remain open. The previous **~8% ready**, **55–90 h**
estimate has not been re-measured; catalog size is not a reviewed coverage percentage.

The catalog lives in `crates/ui-egui/src/zh_hans.rs`, with context-local language handling in
`i18n.rs`. It covers ribbon tools, galleries (shapes, transitions, animations, charts and table
styles), File pages and print/export settings, format/background/animation/comment panes,
dialogs, command palette labels, slideshow/presenter controls, contributor tables, common
engine notices and native file-picker titles/filters. Formatted labels keep Rust's compile-time
format checks. Editor-only placeholder prompts are supplied through a renderer callback and
cached by interface language; document contents, custom prompts, stored IDs and API responses
remain unchanged. Unknown strings and third-party diagnostic details retain English fallback.

On Windows the interface prefers the user's installed DengXian/等线 face. No proprietary font
is bundled, downloaded or installed by this build. Other platforms keep existing font fallback;
browser CJK coverage depends on embedded fonts. This expanded preview was compiled for a local
Windows installer. Its visual and functional validation and native-language review are pending
with the user; the tests from the earlier partial preview are not evidence for this expanded pass.

Review → Language is a separate **text-language** dialog. Its unimplemented choices are now
disabled rather than looking actionable, with a link to interface preferences. This preview
does not implement proofing languages or alter the language metadata of a presentation.

## How the numbers are measured

- **UI strings:** ~1,050 user-visible string literals (measured 2026-10-10: distinct
  capitalized literals in `crates/ui-egui/src`, `crates/engine/src/cmd` and `crates/model/src`,
  including the 226 command labels). A catalog will refine the count; until then percentages are
  against this figure.
- **Script support** is about slide text (what users type and import), separately from the UI:
  bidi and shaping live in `crates/text` (UAX #9, HarfRust, UAX #14 line breaking).
- **PowerPoint's languages:** `Contents/Resources/*.lproj` in PowerPoint 16.113.4: ar, cs, da, de,
  el, en, en_GB, es, es_MX, fi, fr, fr_CA, he, hu, id, it, ja, ko, nl, no, pl, pt (Brazil), pt_PT,
  ru, sk, sv, th, tr, zh_CN, zh_TW (26 languages). PowerPoint for **Mac** has no Hindi or Vietnamese
  UI; PowerPoint for Windows has both.

## The twelve key languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support (slide text) | Native review | Status | PowerPoint for Mac UI | Hours to full |
|---|---|---|---|---|---|---|---|---|
| English | en | ~1,050 / ~1,050 (100%) | yes / yes / no help | full | yes | **full** | yes | 0 |
| Simplified Chinese (Mandarin) | zh-Hans | expanded catalog; reviewed coverage not measured | main interface, advanced panes/dialogs, galleries and tooltips; no help translation | renders with system CJK fonts (asset policy excludes Noto CJK from builds); UAX #14 breaking; IME composition (#101); no true vertical layout | pending | preview | yes | not re-measured |
| Spanish | es | 0 (0%) | no | full (Latin) | no | none | yes (es, es-MX) | 3–5 |
| Hindi | hi | 0 (0%) | no | Devanagari shaping through HarfRust, untested; no bundled Devanagari font | no | none | **no** (Windows only) | 4–7 |
| Arabic | ar | 0 (0%) | no | bidi, contextual shaping, kashida justification, RTL bullets and caret (#4); **UI not mirrored**; browser export shows boxes (#81) | no | none | yes | 10–16 (incl. UI mirroring) |
| French | fr | 0 (0%) | no | full (Latin) | no | none | yes (fr, fr-CA) | 3–5 |
| Portuguese | pt | 0 (0%) | no | full (Latin) | no | none | yes (pt-BR, pt-PT) | 3–5 |
| Indonesian | id | 0 (0%) | no | full (Latin) | no | none | yes | 3–5 |
| Japanese | ja | 0 (0%) | no | renders with system or craft-fonts Japanese faces; IME composition (#101); vertical text is rotated, not laid out vertically | no | none | yes | 4–7 |
| German | de | 0 (0%) | no | full (Latin) | no | none | yes | 3–5 |
| Korean | ko | 0 (0%) | no | system Hangul fonts, in the interface too; IME composition (#101) | no | none | yes | 4–7 |
| Vietnamese | vi | 0 (0%) | no | stacked diacritics through HarfRust | no | none | **no** (Windows only) | 3–5 |

Other translated interface languages shipped: **0** (Ukrainian is in review as PR #34). PowerPoint for Mac ships 16 more
than the twelve above: Czech, Danish, Greek, Finnish, Hebrew, Hungarian, Italian, Dutch, Norwegian,
Polish, Russian, Slovak, Swedish, Thai, Turkish and Traditional Chinese.

The Chinese catalog and a manual language setting exist; system detection (#15), a reusable
plural/number framework, native review and an `xtask` completeness check remain open.
The previous infrastructure estimate was **8–14 h** and has not been re-measured.
IME composition landed with #101 (slide text, notes and text fields; one undo step
per composition); the hours that included it are kept until the next re-measure.

## Proofing languages

PowerPoint checks spelling in dozens of languages and lets each run carry a proofing language.
DeckCraft keeps `a:rPr/@lang` on round trip but its spelling check is a small English list of
common misspellings (`crates/engine/src/cmd/review.rs`); see [gaps.md](gaps.md#proofing-spelling-thesaurus-autocorrect).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-14 | major | Expanded Simplified Chinese across the editor, dialogs, galleries, File pages, slideshow, notices and native file pickers; localized editor placeholders; installed DengXian preference; user review pending |
| 2026-10-12 | minor | IME composition for Chinese, Japanese and Korean, and interface text in any installed script (#101) |
| 2026-10-10 | major | Created: twelve-language table, ~1,050 UI strings measured, PowerPoint for Mac's 26 languages from its bundle |
