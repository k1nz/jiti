# Design System Master File

> **LOGIC:** When building a specific page, first check `design-system/pages/[page-name].md`.
> If that file exists, its rules **override** this Master file.
> If not, strictly follow the rules below.

---

**Project:** Jiti
**Generated:** 2026-09-11 15:38:17
**Category:** Translator App
**Design Dials:** Variance 6/10 (Balanced / Modern) | Motion 4/10 (Standard) | Density 3/10 (Spacious)

---

## Global Rules

### Color Palette

| Role | Hex | CSS Variable |
|------|-----|--------------|
| Paper | `#FFF4DD` | `--paper` |
| Ink | `#26201A` | `--ink` |
| Ink muted | `#5C5347` | `--ink-2` |
| Ink faint | `#877B6B` | `--ink-3` |
| Sun / primary CTA | `#FFD84D` | `--sun` |
| Accent | `#FF5C2B` | `--accent` |
| Card | `#FFFDF7` | `--card` |
| Rose | `#FFB9CC` | `--rose` |
| Sky | `#A5DCFF` | `--sky` |
| Mint | `#B9E6A6` | `--mint` |
| Watch paper | `#FFF0BD` | `--watch-paper` |
| Peach | `#FFB07A` | `--peach` |

**Color Notes:** Codex Resets light-mode tokens. Warm cream paper, near-black ink, hard 2px ink borders. Sun yellow is the only primary CTA fill. Do not introduce teal, purple, or Inter-slate defaults.

### Typography

- **Heading Font:** Baloo 2 (display, 700–800)
- **Body Font:** system-ui, -apple-system, "Segoe UI", sans-serif
- **Mono:** ui-monospace, "SF Mono", Menlo, Consolas, monospace
- **Mood:** warm analog, rounded, friendly developer-tool, light neo-brutal
- **Fonts:** [Baloo 2](https://fonts.bunny.net/css?family=baloo-2:600,700,800)

**CSS Import:**
```css
@import url('https://fonts.bunny.net/css?family=baloo-2:600,700,800');
```

### Spacing Variables

*Density: 3/10 — Spacious*

| Token | Value | Usage |
|-------|-------|-------|
| `--space-xs` | `4px` / `0.25rem` | Tight gaps |
| `--space-sm` | `8px` / `0.5rem` | Icon gaps, inline spacing |
| `--space-md` | `24px` / `1.5rem` | Standard padding |
| `--space-lg` | `32px` / `2rem` | Section padding |
| `--space-xl` | `48px` / `3rem` | Large gaps |
| `--space-2xl` | `64px` / `4rem` | Section margins |
| `--space-3xl` | `96px` / `6rem` | Hero padding |

### Shadow Depths

| Level | Value | Usage |
|-------|-------|-------|
| `--shadow-sm` | `3px 3px 0 #26201A` | Pills, small controls |
| `--shadow` | `4px 4px 0 #26201A` | Cards, buttons |
| `--shadow-lg` | `6px 6px 0 #26201A` | Hero / featured cards |
| `--border` | `2px solid #26201A` | All chrome |

---

## Component Specs

### Buttons

```css
.btn-primary {
  background: #FFD84D;
  color: #26201A;
  border: 2px solid #26201A;
  box-shadow: 3px 3px 0 #26201A;
  padding: 8px 14px;
  border-radius: 999px;
  font-family: "Baloo 2", system-ui, sans-serif;
  font-weight: 700;
  cursor: pointer;
}

.btn-primary:hover { transform: translate(1px, 1px); box-shadow: 2px 2px 0 #26201A; }
.btn-primary:active { transform: translate(3px, 3px); box-shadow: 0 0 0 #26201A; }

.btn-secondary {
  background: #FFFDF7;
  color: #26201A;
  border: 2px solid #26201A;
  box-shadow: 3px 3px 0 #26201A;
  padding: 8px 14px;
  border-radius: 999px;
  font-weight: 700;
  cursor: pointer;
}
```

### Cards

```css
.card {
  background: #FFFDF7;
  border: 2px solid #26201A;
  border-radius: 14px;
  padding: 24px;
  box-shadow: 4px 4px 0 #26201A;
}
```

### Inputs

```css
.input {
  padding: 12px 16px;
  border: 1px solid #E2E8F0;
  border-radius: 8px;
  font-size: 16px;
  transition: border-color 200ms ease;
}

.input:focus {
  border-color: #78716C;
  outline: none;
  box-shadow: 0 0 0 3px #78716C20;
}
```

### Modals

```css
.modal-overlay {
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(4px);
}

.modal {
  background: white;
  border-radius: 16px;
  padding: 32px;
  box-shadow: var(--shadow-xl);
  max-width: 500px;
  width: 90%;
}
```

---

## Style Guidelines

**Style:** Neo Brutalism (Mobile)

**Keywords:** neo brutalism, pop art, stickers, thick borders, cream background, hot red, vivid yellow, soft violet, hard offset shadow, mechanical press, collage

**Best For:** Creative tools, collab platforms, Gen Z marketing & e-commerce, portfolio sites, sticker-book style content apps

**Key Effects:** Thick 4px black borders on all major elements, hard offset shadows (4–8px, no blur), mechanical press: translateX/Y equal to shadow offset, slightly rotated cards/badges (-2deg/2deg), high-saturation color blocking, spring/linear animations only

### Page Pattern

**Pattern Name:** App Store Style Landing

- **Conversion Strategy:** Show real screenshots. Include ratings (4.5+ stars). QR code for mobile. Platform-specific CTAs.
- **CTA Placement:** Download buttons prominent (App Store + Play Store) throughout
- **Section Order:** 1. Hero with device mockup, 2. Screenshots carousel, 3. Features with icons, 4. Reviews/ratings, 5. Download CTAs

---

## Motion

**Page Transition** (Standard) — Trigger: route change | Duration: 400-600ms | Easing: `power2.inOut`

```js
const tl = gsap.timeline(); tl.to('.transition-overlay', { yPercent: 0, duration: 0.4, ease: 'power2.inOut' }).call(navigate).to('.transition-overlay', { yPercent: -100, duration: 0.4, ease: 'power2.inOut', delay: 0.1 });
```

**Framework notes:** Keep the overlay element mounted at the layout root (outside the page component) so it survives the route swap

- ✅ Show a lightweight loading indicator if the destination route's data fetch outlasts the overlay
- ❌ Don't tie the overlay's reveal directly to data-fetch completion without a max-wait timeout; a slow API stalls the whole transition
- ⚡ Prefer CSS transform (yPercent) over top/left to keep the overlay animation on the compositor thread

---

## Anti-Patterns (Do NOT Use)

- ❌ Complex shadows
- ❌ 3D effects

### Additional Forbidden Patterns

- ❌ **Emojis as icons** — Use SVG icons (Heroicons, Lucide, Simple Icons)
- ❌ **Missing cursor:pointer** — All clickable elements must have cursor:pointer
- ❌ **Layout-shifting hovers** — Avoid scale transforms that shift layout
- ❌ **Low contrast text** — Maintain 4.5:1 minimum contrast ratio
- ❌ **Instant state changes** — Always use transitions (150-300ms)
- ❌ **Invisible focus states** — Focus states must be visible for a11y

---

## Pre-Delivery Checklist

Before delivering any UI code, verify:

- [ ] No emojis used as icons (use SVG instead)
- [ ] All icons from consistent icon set (Heroicons/Lucide)
- [ ] `cursor-pointer` on all clickable elements
- [ ] Hover states with smooth transitions (150-300ms)
- [ ] Light mode: text contrast 4.5:1 minimum
- [ ] Focus states visible for keyboard navigation
- [ ] `prefers-reduced-motion` respected
- [ ] Responsive: 375px, 768px, 1024px, 1440px
- [ ] No content hidden behind fixed navbars
- [ ] No horizontal scroll on mobile
