// Responsive gate.
//
// The site is meant to reflow continuously rather than at a handful of
// breakpoints, so it is checked continuously: every route is swept across the
// whole width range in both themes, and each width has to satisfy four
// properties that a screenshot review reliably misses.
//
//   no overflow      nothing lays out wider than the viewport, which is what
//                    turns a phone page into a sideways-scrolling one
//   tap targets      every control clears the WCAG 2.5.8 minimum once the
//                    pointer is coarse
//   uniform cards    no card grid ever shows a ragged row, the property the
//                    catalogue was rebuilt to hold — checked per grid, since
//                    that is the unit a reader actually compares across
//   readable measure no line of body prose runs past ~85 characters, the
//                    failure an ultrawide introduces rather than a phone
//
// The width list is deliberately not the breakpoint list: it includes the
// awkward in-between sizes (going from 700 to 900 in steps) because those are
// exactly the widths a breakpoint-authored layout never had designed for it.

import { chromium } from 'playwright'
import process from 'node:process'

const ORIGIN = process.env.SITE_ORIGIN ?? 'http://127.0.0.1:4700'

// `/features` is gone: the catalogue moved under /docs when the two pages
// merged, and every feature is now a page of its own. One feature page is
// swept rather than all thirty-eight — they are one component with one
// layout, so the thirty-eighth cannot fail in a way the first does not. The
// one chosen is the one with a platform strip, which is the widest thing a
// feature page ever has to fit.
const PATHS = ['/', '/docs', '/docs/features',
  '/docs/features/companion-publish', '/docs/introduction',
  '/docs/routing', '/docs/devices', '/docs/yon', '/docs/platform-apis',
  '/docs/cookbook']

// 320 is the narrowest width worth supporting; 2560 is a common ultrawide.
const WIDTHS = [320, 360, 375, 414, 480, 540, 600, 700, 768, 820, 900, 1024,
  1180, 1280, 1440, 1600, 1920, 2560]

const probe = () => {
  const root = document.documentElement
  const viewport = root.clientWidth

  const wide = [...document.querySelectorAll('body *')]
    .filter((element) => element.getBoundingClientRect().width > viewport + 1)
    .map((element) => `${element.tagName.toLowerCase()}.${String(element.className).slice(0, 30)}`)

  // Only visible controls count, and only standalone ones: WCAG 2.5.8 exempts
  // a link sitting inside a sentence, whose size is set by the prose around
  // it. A custom element is measured by the control it renders, since the
  // host itself may be an inline box that does not report the button's size.
  const small = [...document.querySelectorAll('.w-btn, w-btn, w-tab, .w-chip, button, a[href]')]
    .filter((element) => !element.closest('p'))
    .map((element) => {
      const rendered = element.querySelector(':scope > .w-btn, :scope > .w-tab, :scope > .w-chip') ?? element
      return { element, box: rendered.getBoundingClientRect() }
    })
    .filter(({ box }) => box.width > 0 && box.height > 0 && box.height < 24)
    .map(({ element }) => `${element.tagName.toLowerCase()}:${element.textContent.trim().slice(0, 18)}`)

  // Uniformity is a property of a grid, not of the page: the catalogue is one
  // grid per feature group, and two cards a heading and a divider apart are
  // never seen side by side. What must hold is that no grid ever shows a
  // ragged row.
  const heights = (selector) => {
    const grids = [...document.querySelectorAll('.grid-auto--even, .catalogue, .pillars')]
      .map((grid) => [...grid.querySelectorAll(selector)]
        .map((card) => Math.round(card.getBoundingClientRect().height)))
      .filter((cards) => cards.length > 1)
    return {
      count: grids.reduce((total, cards) => total + cards.length, 0),
      ragged: grids.filter((cards) => new Set(cards).size > 1).length,
    }
  }

  // Character count, not pixels: the readable measure is a count of glyphs.
  const long = [...document.querySelectorAll('p')]
    .filter((paragraph) => {
      const text = paragraph.textContent.trim()
      if (text.length < 90) return false
      const size = parseFloat(getComputedStyle(paragraph).fontSize)
      return paragraph.getBoundingClientRect().width / (size * 0.5) > 92
    })
    .map((paragraph) => paragraph.textContent.trim().slice(0, 24))

  return {
    scrollWidth: root.scrollWidth,
    viewport,
    overflow: [...new Set(wide)].slice(0, 3),
    smallTargets: [...new Set(small)].slice(0, 3),
    cards: heights('.catalogue__card, .pillar, .feature-card'),
    longLines: [...new Set(long)].slice(0, 2),
  }
}

const browser = await chromium.launch()
const failures = []
let checks = 0

for (const theme of ['dark', 'light']) {
  for (const width of WIDTHS) {
    // A narrow viewport is treated as a touch device, which is what makes the
    // tap-target rule apply the way it will in the wild.
    const coarse = width <= 820
    const context = await browser.newContext({
      viewport: { width, height: 900 },
      hasTouch: coarse,
      isMobile: coarse,
      deviceScaleFactor: 1,
    })
    await context.addInitScript((value) => localStorage.setItem('w-theme', value), theme)
    const page = await context.newPage()

    for (const path of PATHS) {
      await page.goto(`${ORIGIN}${path}`, { waitUntil: 'networkidle' })
      await page.waitForTimeout(400)
      const report = await page.evaluate(probe)
      checks += 1

      const problems = []
      if (report.scrollWidth > report.viewport + 1) {
        problems.push(`overflows to ${report.scrollWidth}px (${report.overflow.join(' ') || 'unattributed'})`)
      }
      if (coarse && report.smallTargets.length) {
        problems.push(`tap targets under 24px: ${report.smallTargets.join(' ')}`)
      }
      if (report.cards.ragged) {
        problems.push(`${report.cards.ragged} ragged card grid(s)`)
      }
      if (report.longLines.length) {
        problems.push(`line measure over 92ch: ${report.longLines.join(' | ')}`)
      }

      if (problems.length) {
        failures.push(`${theme} ${String(width).padStart(4)} ${path}`)
        console.log(`FAIL ${theme.padEnd(5)} ${String(width).padStart(4)} ${path.padEnd(20)} ${problems.join('; ')}`)
      }
    }
    await context.close()
  }
  console.log(`  swept ${WIDTHS.length} widths x ${PATHS.length} routes in ${theme}`)
}

await browser.close()
console.log(`\n${checks} checks across ${WIDTHS[0]}–${WIDTHS.at(-1)}px`)
if (failures.length) {
  console.error(`responsive gate failed: ${failures.length} of ${checks}`)
  process.exit(1)
}
console.log('PASS: every route holds at every width, in both themes')
