---
name: efficient-ai
description: Same quality, fewer tokens — Shinkansen Daiya timetable design system for empirical AI agent token economics.
colors:
  cobalt: "#0B4EA2"
  cobalt-dark: "#083978"
  cobalt-tint: "#E7F0FA"
  jade: "#0F7B5F"
  jade-hover: "#0C654E"
  jade-text: "#0A634C"
  jade-soft: "#E8F5F0"
  jade-border: "#A8DBC9"
  mint-on-dark: "#6EE7B7"
  vermilion: "#D9381E"
  vermilion-text: "#B4250F"
  vermilion-soft: "#FDF0ED"
  vermilion-border: "#F0B6AA"
  stock: "#F7F8FA"
  sheet: "#FFFFFF"
  tint: "#EDF1F6"
  obsidian: "#11161D"
  obsidian-plate: "#1B232E"
  ink-muted: "#4A5565"
  ink-on-dark: "#DCE5F0"
  rule: "#C6D0DC"
  rule-soft: "#DCE3EC"
typography:
  display:
    fontFamily: "'Archivo', 'Segoe UI', system-ui, sans-serif"
    fontSize: "clamp(40px, 4.3vw, 56px)"
    fontWeight: 900
    lineHeight: 0.98
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "'Archivo', 'Segoe UI', system-ui, sans-serif"
    fontSize: "clamp(25px, 2.5vw, 34px)"
    fontWeight: 900
    lineHeight: 1.1
    letterSpacing: "-0.025em"
  title:
    fontFamily: "'Archivo', 'Segoe UI', system-ui, sans-serif"
    fontSize: "18.5px"
    fontWeight: 900
    lineHeight: 1.2
    letterSpacing: "-0.02em"
  body:
    fontFamily: "'Public Sans', 'Segoe UI', system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "0em"
  label:
    fontFamily: "'JetBrains Mono', ui-monospace, 'Cascadia Mono', Consolas, monospace"
    fontSize: "12px"
    fontWeight: 700
    lineHeight: 1.35
    letterSpacing: "0.04em"
    fontFeature: "\"tnum\" 1"
  code:
    fontFamily: "'JetBrains Mono', ui-monospace, 'Cascadia Mono', Consolas, monospace"
    fontSize: "14.5px"
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: "0em"
rounded:
  none: "0px"
  node: "50%"
spacing:
  xs: "6px"
  sm: "12px"
  md: "16px"
  lg: "24px"
  xl: "32px"
  section: "60px"
components:
  button-solid:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.sheet}"
    typography: "{typography.title}"
    rounded: "{rounded.none}"
    padding: "11px 18px"
  button-solid-hover:
    backgroundColor: "{colors.cobalt-dark}"
    textColor: "{colors.sheet}"
  button-line:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.obsidian}"
    typography: "{typography.title}"
    rounded: "{rounded.none}"
    padding: "10px 16px"
  button-line-hover:
    backgroundColor: "{colors.obsidian}"
    textColor: "{colors.sheet}"
  button-copy-latch:
    backgroundColor: "{colors.jade}"
    textColor: "{colors.sheet}"
    typography: "{typography.title}"
    rounded: "{rounded.none}"
    padding: "15px 30px"
  button-copy-latch-hover:
    backgroundColor: "{colors.jade-hover}"
    textColor: "{colors.sheet}"
  button-copy-latch-copied:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.jade-text}"
  tab-platform:
    backgroundColor: "{colors.obsidian-plate}"
    textColor: "{colors.ink-on-dark}"
    rounded: "{rounded.none}"
    padding: "7px 13px"
  tab-platform-active:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.sheet}"
  tab-filter:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.obsidian}"
    rounded: "{rounded.none}"
    padding: "9px 16px"
  tab-filter-active:
    backgroundColor: "{colors.obsidian}"
    textColor: "{colors.sheet}"
  toggle-layer:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink-muted}"
    rounded: "{rounded.none}"
    padding: "11px 11px"
  toggle-layer-active:
    backgroundColor: "{colors.jade-soft}"
    textColor: "{colors.obsidian}"
  select-rail:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.obsidian}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "11px 36px 11px 12px"
  chip-layer:
    backgroundColor: "{colors.cobalt-tint}"
    textColor: "{colors.cobalt-dark}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "2px 8px"
  badge-impact-good:
    backgroundColor: "{colors.jade-soft}"
    textColor: "{colors.jade-text}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "6px 10px"
  badge-impact-bad:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.vermilion-text}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "6px 10px"
  fascia-nav:
    backgroundColor: "{colors.cobalt}"
    textColor: "{colors.sheet}"
    rounded: "{rounded.none}"
    height: "76px"
    padding: "0 32px"
  compartment-sheet:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.obsidian}"
    rounded: "{rounded.none}"
    padding: "20px 22px"
---

# Design System: efficient-ai

## Overview

**Creative North Star: "The Shinkansen Daiya Timetable"**

The `efficient-ai` visual system charts multi-turn AI coding agent token economics with the operational clarity of a Japanese Shinkansen train-graph (`daiya`) and station wayfinding fascia. Ceramic coated timetable stock (`#F7F8FA`) and pure chart sheets (`#FFFFFF`) carry dense, verifiable telemetry anchored by a royal-cobalt enamel header (`#0B4EA2`) and obsidian command tracks (`#11161D`). Every surface privileges falsifiable measurements, tabular alignment, and instant mechanical legibility over decorative staging.

Material contrast comes from crisp ruled borders (`1px` to `6px`), alternating light paper grounds and high-contrast obsidian bands, and strict signal-color discipline: Express Jade (`#0F7B5F`) marks verified cost savings and cache-safe compaction stops, while Signal Vermilion (`#D9381E`) marks cache-busting spikes and busted myths. Motion behaves like an instrument needle rather than a showcase effect, snapping across exponential ease-out curves (`cubic-bezier(0.16, 1, 0.3, 1)`) and collapsing cleanly under reduced-motion preferences.

The system explicitly rejects category-default purple-glow SaaS heroes, dark neon terminal clichés, soft drop shadows, rounded floating cards, colored vertical left-border stripes, eyebrow kickers above headings, and decorative glyph icons.

**Key Characteristics:**
- Ceramic timetable stock (`#F7F8FA`) and pure chart sheets (`#FFFFFF`) framed by royal-cobalt enamel (`#0B4EA2`) and obsidian command tracks (`#11161D`)
- Three self-hosted variable typography voices (`Archivo`, `Public Sans`, `JetBrains Mono`) with universal tabular numerals on live telemetry
- Flat architectural depth using `1px–6px` structural rules and tonal paper stepping with zero `box-shadow` declarations
- Machined `0px` corners across all compartments and controls, reserving `50%` circles exclusively for route-track station nodes and SVG graph markers
- Strict signal-color semantics separating Express Jade verified savings from Signal Vermilion cache-bust spikes
- Decisive exponential ease-out (`cubic-bezier(0.16, 1, 0.3, 1)`) transitions paired with square-miter inline SVG icons and full reduced-motion support

## Colors

A daylight-first civic timetable palette built on cool ceramic paper, deep obsidian ink, royal-cobalt structural authority, and two strictly rationed operational signals.

### Primary
- **Shinkansen Royal Cobalt** (`#0B4EA2`, `--cobalt`): Sticky station fascia ground, hero emphasis highlight, solid primary CTA fill, active install platform tab, range slider thumb, link underlines, `:focus-visible` outlines on light surfaces, structural bottom/top command rails, and primary route-track segments.
- **Deep Cobalt** (`#083978`, `--cobalt-dark`): Solid CTA hover state, fare-summary headline text, and classification chip label ink on cobalt wash.
- **Cobalt Wash** (`#E7F0FA`, `--cobalt-tint`): Subtle interactive hover fill on layer switches and filter tabs, fare-summary footer band fill, and leaderboard layer chip background.

### Secondary
- **Express Jade** (`#0F7B5F`, `--jade`): Fascia `5px` bottom express stripe, primary 1-click copy-install latch fill, active 4-layer switch registration bar and `ON` state chip, `eai` curve and station dots on the live timetable SVG, cache-safe fare row bottom border and proportional bar fill, and terminal route station `04`.
- **Deep Express Jade Hover** (`#0C654E`): Hover darken state for the Express Jade copy-install latch.
- **Express Jade Ink** (`#0A634C`, `--jade-text`): High-contrast text companion on light stock, pure sheets, and jade wash for verified savings KPIs (`−37.6%`), turn-inspector savings, active layer impacts, copied latch confirmation text, fareboard savings, positive leaderboard badges, and SVG curve callouts.
- **Jade Wash** (`#E8F5F0`, `--jade-soft`): Active additive layer switch fill, cache-safe fare row ground, verified impact badge background, and station share pill fill.
- **Jade Rule** (`#A8DBC9`, `--jade-border`): Crisp `1px` containment border around positive impact badges and station share tags.
- **Mint on Dark** (`#6EE7B7`): High-contrast luminous jade accent used exclusively on `#11161D` obsidian grounds for CLI ledger commands and terminus footer link hover states.

### Tertiary
- **Signal Vermilion** (`#D9381E`, `--vermilion`): Strictly reserved for cache-busting spikes and misleading single-stream warnings—naive pruning curve on the timetable SVG, cache-bust fare row top/bottom rules, status tag, and bar fill, build/test/git `6.8%` meter segment, and mobile myth-row frame.
- **Deep Signal Vermilion Ink** (`#B4250F`, `--vermilion-text`): High-contrast text companion on light grounds and vermilion wash for `+111.8%` spike KPIs, recovery-loop tax figures, naive pruning fare readouts, myth-busted leaderboard badges, and SVG spike callouts.
- **Vermilion Wash** (`#FDF0ED`, `--vermilion-soft`): Background band for the naive per-turn pruning fare row and myth-busted leaderboard rows.
- **Vermilion Rule** (`#F0B6AA`, `--vermilion-border`): Crisp `1px` border framing myth-busted impact badges.

### Neutral
- **Ceramic Coated Stock** (`#F7F8FA`, `--stock`): Default page canvas, hero timetable stage, alternating section bands, simulator control column, and route station body compartments.
- **Pure Chart Sheet** (`#FFFFFF`, `--sheet`): Crisp elevated paper surface for the Daiya graph frame, alternating white section bands, plumbing meter block, laws ledger, simulator fareboard, leaderboard schedule, select dropdowns, line buttons, and inverted text on cobalt/obsidian.
- **Cool Rail Tint** (`#EDF1F6`, `--tint`): Secondary structural wash for the Daiya turn inspector strip, inline `<code>` tags, and custom scrollbar track.
- **Obsidian Command Track / Primary Ink** (`#11161D`, `--obsidian` / `--ink`): Primary display/body ink, `html` overscroll ground, 1-click install rail, Daiya and fareboard header bars, leaderboard `thead`, everyday CLI ledger, anchored terminus footer, `2px–3px` outer compartment frames, and vanilla baseline dashed stroke.
- **Obsidian Plate** (`#1B232E`, `--obsidian-2`): Recessed dark plate surface for unselected platform tabs on the obsidian install rail.
- **Slate Muted Ink** (`#4A5565`, `--ink-muted`): Secondary narrative copy, section metadata, KPI descriptions, simulator notes, inactive layer text, and SVG axis labels (high contrast on `#F7F8FA` and `#FFFFFF`).
- **Light Ink on Dark** (`#DCE5F0`): Readable secondary copy on obsidian tracks (terminus footer body and Daiya chart legend keys).
- **Timetable Hairline** (`#C6D0DC`, `--rule`): Standard `1px` horizontal and vertical dividers separating section bands, ledger cells, layer buttons, filter tabs, fare rows, and range slider track fill.
- **Soft Cell Rule** (`#DCE3EC`, `--rule-soft`): Subtle `1px` table row separators, inline `<code>` outline, and inactive layer `OFF` state badge fill.

### Named Rules
**The Signal Semantics Rule.** Express Jade (`#0F7B5F` / `#0A634C`) is used solely for verified cost savings, cache-safe states, and the primary copy-install latch; Signal Vermilion (`#D9381E` / `#B4250F`) is used solely for cache-bust spikes and myth-busted warnings. Never use either signal for neutral decoration or generic UI chrome.

**The Contrast-Paired Text Rule.** Never set small text on light paper or tinted washes in raw `#0F7B5F` or `#D9381E`; always step to the darkened ink tokens (`#0A634C` and `#B4250F`) on light surfaces, and step positive code/link highlights up to `#6EE7B7` on `#11161D` obsidian tracks.

## Typography

**Display Font:** `Archivo` (with `'Segoe UI', system-ui, sans-serif` fallback) — self-hosted variable `.woff2` (`docs/fonts/archivo-latin.woff2`, weights `600 900`, preloaded, `font-display: swap`).
**Body Font:** `Public Sans` (with `'Segoe UI', system-ui, sans-serif` fallback) — self-hosted variable `.woff2` (`docs/fonts/public-sans-latin.woff2`, weights `400 700`, preloaded, `font-display: swap`).
**Label/Mono Font:** `JetBrains Mono` (with `ui-monospace, 'Cascadia Mono', Consolas, monospace` fallback) — self-hosted variable `.woff2` (`docs/fonts/jetbrains-mono-latin.woff2`, weights `500 700`, preloaded, `font-display: swap`).

**Character:** Compact geometric wayfinding grotesque (`Archivo`) handles headlines, figures, and tactile controls with railway-signboard authority; civic neutral `Public Sans` carries calm explanatory prose; and `JetBrains Mono` anchors telemetry, command strings, axis ticks, and empirical proof badges without visual drift.

### Hierarchy
- **Display** (`900`, `clamp(40px, 4.3vw, 56px)`, line-height `0.98`, tracking `-0.035em`): Primary hero `h1` statement only, with cobalt inline emphasis (`em`, non-italic).
- **Headline** (`900`, `clamp(25px, 2.5vw, 34px)`, line-height `1.1`, tracking `-0.025em`): Section `h2` titles, hero KPI readouts (`clamp(25px, 2.3vw, 32px)`), law ledger figures (`32px`), and fareboard dollar totals (`clamp(22px, 2.2vw, 28px)`).
- **Title** (`800–900`, `16px–22px`, line-height `1.15–1.25`, tracking `-0.02em` to `-0.03em`): Brand wordmark (`22px`, `800`), CLI side heading (`22px`, `900`), route station `h3` (`18.5px`, `900`), plumbing chart `h3` (`18px`, `800`), fare row titles (`16.5px`, `800`), and control labels (`13px–17px`, `700–900`).
- **Body** (`400–700`, `16px`, line-height `1.55`): Primary narrative copy (`16.5px` hero thesis, `16px` section intros capped at `max-width: 78ch`, `14px` table/station/select copy, `13px–13.5px` law and CLI cell descriptions, `12.5px` weight `600` KPI captions).
- **Label** (`700`, `11px–12.5px`, line-height `1.2–1.4`, uppercase + `0.04em` tracking on table headers): Telemetry bars (`12px`), section metadata (`12.5px`), corpus readout (`12.5px`), leaderboard `thead` (`11.5px` uppercase), status tags (`11px`), layer chips (`11.5px`), impact badges (`12px`), and SVG axis ticks (`11px–12px`).
- **Code** (`500–700`, `12px–14.5px`, line-height `1.4`): Obsidian install command (`14.5px`, `500`), CLI ledger commands (`13.5px`, `700`), station commands (`12px`, `700`), and inline `<code>` fragments (`0.86em` on `--tint` with `1px` `--rule-soft` frame).

### Named Rules
**The Tabular Telemetry Rule.** Every numeric figure, currency readout, percentage delta, turn index, and benchmark table cell must apply `font-variant-numeric: tabular-nums` so interactive calculator updates and chart hover sweeps stay horizontally locked.

**The Zero-Kicker Baseline Rule.** Section headers never place stacked uppercase eyebrow kickers above the heading. Supporting context sits inline on the right edge of the `h2` baseline above a `3px` obsidian rule.

## Layout

Content aligns to a `1360px` maximum-width container (`--max-w`) with `32px` desktop horizontal gutters (`24px` tablet hero, `18px` compact fascia/install bars at `<=860px`, `16px` mobile at `<=600px`). Page sections alternate between Ceramic Coated Stock (`#F7F8FA`) and Pure Chart Sheet (`#FFFFFF`) bands divided by `1px` `#C6D0DC` rules (`2px` obsidian beneath the hero stage), carrying `60px` desktop vertical padding (`40px` at `<=600px`).

The top sticky station fascia holds a `76px` desktop minimum height (`62px` at `<=860px`, `z-index: 60`) with `5px` jade bottom track. Below it, the first viewport pairs an asymmetric 2-column timetable stage (`minmax(320px, 430px) minmax(0, 1fr)`, `34px` gap) with a full-width obsidian 1-click install rail (`minmax(0, 1fr) auto`, `5px` cobalt bottom track). Analytical sections use tightly coupled multi-column grids: 3-column hero KPIs, 4-column additive layer toggles, 4-column empirical laws ledger, asymmetric simulator split (`minmax(310px, 440px) minmax(0, 1fr)`), 4-station horizontal route track (continuous `6px` rail segments connecting nodes `01→04` and terminating cleanly at node `04` with no trailing stub), and a `280px minmax(0, 1fr)` obsidian CLI ledger with 2-column command rows.

**Responsive Breakpoints:**
- **`<=1100px`:** Stacks hero stage to 1 column; collapses 4-column laws ledger and route stations to 2 columns (hiding the horizontal connecting rail segments); stacks CLI ledger sidebar above command rows.
- **`<=860px`:** Wraps fascia navigation onto a horizontally scrollable second line and hides the corpus telemetry string; stacks simulator controls above fareboard; stacks install rail with a `100%`-width copy latch; collapses 4-column layer switches to a 2×2 grid.
- **`<=760px`:** Converts the Empirical Technique Leaderboard table into stacked, scannable `2px`-bordered mobile timetable records with monospace `data-label` prefixes and zero horizontal scroll or off-screen columns.
- **`<=600px`:** Collapses KPIs, laws, stations, CLI rows, and fare rows to single-column flow; reduces section padding to `40px 16px`; switches the Daiya SVG dynamically to compact `420×230` coordinates (`224px` height) with condensed curve callouts; hides narrow meter labels except the dominant `63.9%` File Reads bar.

**Browser-Surface & Accessibility Polish:**
- `html` sets `background: #11161D` (matching the obsidian terminus footer to prevent white overscroll flashes) and `scrollbar-color: #0B4EA2 #EDF1F6; scrollbar-width: thin;`.
- `body` uses `overflow-x: clip` to prevent accidental horizontal scroll without disabling `position: sticky` on the station fascia.
- Text selection uses `#0B4EA2` background with `#FFFFFF` ink.
- Keyboard navigation provides a visually hidden `.skip-link` (`#11161D` ground, `#FFFFFF` monospace label at `z-index: 1000` on `:focus`), `2px solid #0B4EA2` `:focus-visible` outlines (`outline-offset: 2px`) on light surfaces, explicit `#FFFFFF` focus outlines on cobalt and obsidian surfaces, and cobalt-to-jade thumb shift on focused range sliders.
- Interactive regions declare semantic ARIA roles (`role="tablist"`/`role="tab"`/`aria-selected`, `aria-pressed`, `aria-live="polite"`, `role="img"` with descriptive labels, and `aria-hidden="true"` on decorative SVGs).

### Named Rules
**The Ruled Compartment Grid Rule.** Related metrics, toggles, and command items live inside shared `2px` obsidian outer frames separated by `1px` internal `#C6D0DC` hairlines rather than floating as isolated cards with background gutters between them.

## Elevation & Depth

This system uses **zero `box-shadow` declarations**. Depth, containment, and focal hierarchy are established strictly through flat tonal paper stepping (`#F7F8FA` ceramic stock → `#FFFFFF` pure chart sheet → `#EDF1F6` cool tint → `#11161D` obsidian command track) and a disciplined five-step border weight ladder: `1px` internal cell dividers (`#C6D0DC` / `#DCE3EC`), `2px` interactive controls and analytical frames (`#11161D`), `3px` section header anchors and KPI dividers (`#11161D`), `5px` express fascia/install rails (`#0F7B5F` / `#0B4EA2`), and `6px` terminus footer anchor (`#0B4EA2`).

State transitions rely on decisive exponential ease-out motion (`--ease-expo: cubic-bezier(0.16, 1, 0.3, 1)`): `120ms` press detent (`translateY(1px)` on the copy latch), `160ms` quiet link underlines, `180ms` surface inversions across buttons/tabs/nav, `200ms–220ms` layer switch highlights, and `280ms` fare bar width transitions, active top-bar `scaleX(0→1)` registration sweeps, and `requestAnimationFrame` SVG curve morphing (`1 - 2^(-10t)`). Under `@media (prefers-reduced-motion: reduce)`, smooth scrolling resets to `auto`, CSS transitions collapse to `0.01ms`, and SVG series snap directly without interpolation.

### Named Rules
**The Zero-Shadow Architectural Rule.** Components remain flat at rest and on hover; never add drop shadows, ambient blurs, or glow halos. Communicate interactivity through crisp border weight, surface inversion, and horizontal registration tracks.

## Shapes

Every structural container, button, tab, select dropdown, range slider thumb, table frame, classification chip, impact badge, and command well uses machined right angles (`0px` border radius). Corner rounding is intentionally absent from rectangular UI to sustain the precision-printed timetable aesthetic.

Circular geometry (`50%` radius) is native to one role only: railway station and data markers—specifically the `38px` circular route-track nodes (ringed by `5px` cobalt or filled in jade along a `6px` connecting rail) and `4px–5.5px` SVG circle points marking fixed `K` compaction boundaries and the live hover cursor.

All UI icons are custom inline SVGs on a `14×14` `viewBox` (`12px` `.ic-sm` and `14px` `.ic`), drawn with `2px–2.2px` strokes, `stroke-linecap="square"`, and `stroke-linejoin="miter"`. Emphasis bands run strictly horizontally along top or bottom borders (`5px` fascia bottom track, `5px` install bottom track, `3px` active layer top sweep, `2px` top/bottom semantic fare borders)—never vertically along left edges.

### Named Rules
**The Horizontal Track Rule.** Use horizontal top or bottom borders and full-width tinted bands to signal hierarchy or state. Never attach vertical colored left-border accent stripes to cards, fare rows, callouts, or table rows.

## Components

Machined, flat, high-contrast components with crisp borders, instant surface inversions, and zero decorative chrome.

### Buttons
- **Shape:** Sharp machined rectangle (`0px` radius) across all variants.
- **Primary Solid:** Royal Cobalt fill (`#0B4EA2`), `2px` `#0B4EA2` border, pure white `Archivo` `800` (`14px`) text, `11px 18px` padding; shifts to Deep Cobalt (`#083978`) over `180ms` `--ease-expo` on hover.
- **Ruled Line (Secondary):** Pure white sheet (`#FFFFFF`), `2px` obsidian (`#11161D`) border, obsidian `Archivo` `800` (`14px`) text, `10px 16px` padding; inverts cleanly to solid obsidian ground with white ink on hover (uses white border override when placed on a cobalt surface).
- **Copy Install Latch (Signature Action):** Express Jade fill (`#0F7B5F`), `2px` `#0F7B5F` border, pure white `Archivo` `900` (`17px`, `0.03em` tracking), `15px 30px` padding, `min-width: 192px` (`100%` at `<=860px`). Hover darkens to `#0C654E`; active depresses `1px` (`120ms`); clicked confirmation swaps to pure white ground with `#0A634C` ink and inline square-miter checkmark SVG for `1700ms`; `:focus-visible` outlines in `#FFFFFF`.

### Chips
- **Layer Classification Chip:** Cobalt Wash (`#E7F0FA`) background, Deep Cobalt (`#083978`) `JetBrains Mono` `700` (`11.5px`) text, `1px` light cobalt hairline, `2px 8px` padding, `0px` radius.
- **Empirical Impact Badge:** Block-level monospace readout (`12px`, `700`, `6px 10px` padding, `0px` radius); verified savings pair Jade Wash (`#E8F5F0`), Express Jade Ink (`#0A634C`), and Jade Rule (`#A8DBC9`); cache-bust/myth warnings pair pure white (`#FFFFFF`), Deep Signal Vermilion Ink (`#B4250F`), and Vermilion Rule (`#F0B6AA`).
- **Station Share & Fare Status Tags:** Compact `JetBrains Mono` `700` (`11px–11.5px`, `2px 7px` padding, `0px` radius); station share tags use jade wash/border/text, while fare tags (`CACHE BUST` / `CACHE SAFE`) use solid vermilion or jade fills with `#FFFFFF` ink.

### Cards / Containers
- **Corner Style:** Strictly `0px` radius across every frame and ledger.
- **Background:** Pure Chart Sheet (`#FFFFFF`) on Ceramic Stock (`#F7F8FA`) sections, or Ceramic Stock (`#F7F8FA`) station bodies on pure white sections; dark command panels use Obsidian (`#11161D`) with Royal Cobalt (`#0B4EA2`) header/side accents.
- **Shadow Strategy:** None (see Elevation & Depth).
- **Border:** `2px solid #11161D` perimeter frame with `1px solid #C6D0DC` (or `#DCE3EC`) internal grid hairlines; semantic fare rows use full-width horizontal `2px` vermilion top/bottom borders (`#FDF0ED` fill) or `2px` jade bottom borders (`#E8F5F0` fill) with zero vertical side stripes.
- **Internal Padding:** `16px–24px` desktop (`16px` mobile), tightening to `10px` outer wrapper around desktop leaderboard tables and `14px` inside stacked mobile table records.

### Inputs / Fields
- **Select Dropdown (`select.rail-select`):** Full-width `0px` square frame on `#FFFFFF` with `2px solid #11161D` border, `11px 36px 11px 12px` padding, bold `14px` `Public Sans` ink, and a custom CSS dual-gradient geometric chevron in `#11161D` (zero browser default arrow).
- **Range Slider (`input[type="range"].rail-range`):** Flat `8px` `#C6D0DC` track bordered by `1px solid #11161D`; custom `18×22px` sharp rectangular (`0px` radius) `#0B4EA2` thumb with `2px solid #11161D` stroke that switches fill to `#0F7B5F` on `:focus-visible`.

### Navigation
- **Sticky Enamel Station Fascia:** Full-width `#0B4EA2` bar (`min-height: 76px`, sticky `top: 0`, `z-index: 60`, `5px solid #0F7B5F` bottom track). Left carries the inverted white `EAI` plate (`Archivo` `900`, `18px`, `#0B4EA2` ink) and `efficient-ai` wordmark (`22px`, `800`); center carries `Archivo` `700` (`14.5px`) links with `2px` bottom underline on hover; right displays tabular `JetBrains Mono` corpus telemetry (`315 SESSIONS · 2.59B TOKENS`) and a bordered `v1.1.0` version plate that inverts to `#FFFFFF` / `#0B4EA2` on hover. At `<=860px`, telemetry hides and links wrap to a horizontally scrollable full-width sub-row.
- **Obsidian Platform Switcher:** Row of `role="tab"` plates (`#1B232E` fill, `1px` dark slate border, `7px 13px` padding, `Archivo` `700`, `13px`) switching to solid `#0B4EA2` fill/border when `aria-selected="true"`, paired directly above the monospace command well.
- **Ruled Leaderboard Filter Rail:** Fit-content `2px solid #11161D` horizontal strip of adjacent `0px` buttons separated by `1px` `#C6D0DC` rules (`9px 16px`, `Archivo` `800`, `13px`), tinting `#E7F0FA` on hover and inverting to solid `#11161D` / `#FFFFFF` when active.
- **Anchored Terminus Footer:** Full-width `#11161D` band anchored by a `6px solid #0B4EA2` top rail, repeating the inverted `EAI` plate alongside monospace links that highlight to `#6EE7B7` on hover.

### Live Daiya Timetable & Additive Layer Switches
Interactive multi-turn cost chart housed in a `2px` obsidian frame with a `#11161D` top telemetry bar, pure `#FFFFFF` SVG plot (dashed `#11161D` vanilla baseline, `#D9381E` naive pruning spike, `#0F7B5F` `eai` curve with `rgba(15,123,95,0.12)` savings polygon, dashed vertical `K`-boundary ticks, and white-filled jade-ringed compaction dots), `#EDF1F6` pointer-synced turn inspector strip, and a 4-column additive switch rail. Each switch toggles `aria-pressed`, sweeping a `3px` `#0F7B5F` top registration bar from `scaleX(0)` to `scaleX(1)` over `280ms` `--ease-expo` while switching the cell to `#E8F5F0`, the state tag to `#0F7B5F` / `#FFFFFF`, and morphing the SVG curves smoothly across `280ms`.

## Do's and Don'ts

### Do:
- **Do** preserve the daylight ceramic stock (`#F7F8FA`) and pure chart sheet (`#FFFFFF`) hierarchy anchored by `#0B4EA2` royal cobalt and `#11161D` obsidian tracks.
- **Do** self-host and preload the three variable font faces (`Archivo`, `Public Sans`, `JetBrains Mono`) and apply `font-variant-numeric: tabular-nums` to every metric, price, percentage, and table column.
- **Do** enforce strict signal semantics: `#0F7B5F` / `#0A634C` exclusively for verified savings, cache-safe states, and the copy latch; `#D9381E` / `#B4250F` exclusively for cache-bust spikes and myth-busted warnings.
- **Do** keep structural depth flat via `1px–6px` architectural borders, surface inversions, and horizontal border tracks.
- **Do** use sharp `0px` corners on all containers, buttons, tabs, inputs, badges, and tables, reserving `50%` circles exclusively for route-track station nodes and SVG graph markers.
- **Do** draw inline geometric SVGs (`stroke-linecap="square"`, `stroke-linejoin="miter"`) and honor `@media (prefers-reduced-motion: reduce)` across both CSS transitions and JavaScript animations.
- **Do** collapse wide data tables into `data-label`-prefixed stacked timetable records below `760px` so mobile views have zero horizontal overflow.

### Don't:
- **Don't** add `box-shadow`, ambient glow halos, glassmorphism, or dark neon terminal styling to any surface or control.
- **Don't** place stacked uppercase eyebrow kickers above section headings or page titles.
- **Don't** attach colored vertical left-border accent stripes to cards, fare rows, callouts, or table rows.
- **Don't** round buttons, containers, badges, inputs, or cards into soft SaaS pills or rounded rectangles.
- **Don't** use Unicode arrow/check glyphs or third-party icon fonts in place of crisp square-miter inline SVGs.
- **Don't** set small light-surface copy directly in raw `#0F7B5F` or `#D9381E` when `#0A634C` and `#B4250F` ink tokens are required for accessible contrast.
- **Don't** leave a trailing connecting rail segment extending past the terminal node (`04`) of a horizontal route track.
