# Download Page Overrides

> **PROJECT:** Jiti
> **Page Type:** Release / download landing
> **Reference:** Codex Resets light mode (https://codex-resets.com/) — visual language only, not branding.

---

## Page-Specific Rules

### Layout Overrides

- **Max Width:** 832px content column, centered on cream `--paper`
- **Layout:** Centered, spacious, one column. Nav is a single row inside the column.
- **Sections:** 1. Header  2. Hero (headline + sub + platform CTAs)  3. Product overlay mockup  4. Three pastel feature tiles  5. How-it-works numbered list  6. Shortcuts  7. FAQ  8. Footer CTA

### Typography Overrides

- Display: Baloo 2. Body: system-ui. Mono for shortcuts and version chips.
- Hero headline max 2 lines. Subtext max 20 words.

### Color Overrides

- Light mode only (the brief is Codex light).
- Primary download CTA = `--sun` with ink text (never white-on-white).
- Feature tiles use `--sun` / `--rose` / `--sky` once each.
- Product mockup interior uses Jiti panel tokens (`#F6F7F9`, `#1C2128`, `#E8910F`), framed in the Codex card chrome.

### Component Overrides

- Pills 999px, cards 14px. Hard offset shadows, mechanical press on buttons.
- Do not rotate cards. Do not use blur, glass, or Inter.
- Icons: Phosphor regular, never emoji.
- Platform-specific CTAs (macOS / Windows). Repeat the same labels in the footer.

### Motion

- Subtle: 180–280ms ease-out on press. Optional 400ms hero fade-in.
- Respect `prefers-reduced-motion`.
