// Browser entry. The compiler links the stylesheet from the document and
// removes this bare CSS import from the emitted module.
import '../styles/site.css'

// DuVay ships its components and behaviours as two scripts, vendored rather
// than fetched so every target stays self-contained under `default-src 'self'`.
const inBrowser = typeof document !== 'undefined' && typeof document.querySelector === 'function'

if (inBrowser && !document.querySelector('script[data-duvay]')) {
  for (const [file, marker] of [['duvay-wc.min.js', 'wc'], ['duvay.min.js', 'behaviours']]) {
    const script = document.createElement('script')
    script.src = new URL(`../assets/duvay/${file}`, import.meta.url).href
    script.defer = true
    script.dataset.duvay = marker
    document.head.append(script)
  }
}

// Applying the stored theme before first paint avoids a flash of the wrong one.
// DuVay falls back to light when nothing is stored, so a first visit is seeded
// with dark instead — it is the theme the palette above is designed around,
// and the toggle still owns every visit after this one.
if (inBrowser) {
  try {
    const stored = localStorage.getItem('w-theme') ?? 'dark'
    localStorage.setItem('w-theme', stored)
    document.documentElement.setAttribute('w-theme', stored)
  } catch {
    document.documentElement.setAttribute('w-theme', 'dark')
  }
}
