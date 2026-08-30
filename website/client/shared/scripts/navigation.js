// The documentation's shape, in one place.
//
// The sidebar, the catalogue index, the guide pages and the feature pages all
// need the same answers: which sections exist, what is in them, and what comes
// next. Each used to derive that itself, and the sidebar's group order was
// already a second copy of a list the data does not carry.
import docs from '/shared/data/docs.json' with { type: 'json' }
import catalogue from '/shared/data/features.json' with { type: 'json' }

/** The order feature groups are shown in, which the data does not carry. */
const GROUP_ORDER = [
  'Routing',
  'Tac views',
  'Browser storage',
  'Polyglot',
  'Yon server',
  'Native',
  'Tooling',
]

/** A URL-safe fragment of a human name, used for group anchors and ids. */
export const slugify = (value) =>
  String(value)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')

/** Every guide, in reading order. */
export const guides = () =>
  docs.order.map((slug) => ({
    slug,
    title: docs.topics[slug].title,
    summary: docs.topics[slug].summary,
    path: `/docs/${slug}`,
  }))

export const guide = (slug) => docs.topics[slug]

/** Every feature, in the order the catalogue declares them. */
export const features = () =>
  catalogue.features.map((feature) => ({ ...feature, path: `/docs/features/${feature.id}` }))

export const feature = (id) => features().find((entry) => entry.id === id)

/** The feature groups, each with the features in it. */
export const groups = () =>
  GROUP_ORDER.map((name) => ({
    name,
    slug: slugify(name),
    features: features().filter((entry) => entry.group === name),
  })).filter((group) => group.features.length > 0)

/**
 * Every documentation page, in one flat reading order.
 *
 * The pager walks this rather than a per-section list, so the last guide leads
 * into the first feature instead of into nothing — which is what made the old
 * pager stop halfway through the documentation.
 */
export const reading = () => [
  ...guides().map((entry) => ({ title: entry.title, path: entry.path })),
  ...groups().flatMap((group) => group.features.map((entry) => ({ title: entry.title, path: entry.path }))),
]

/** What comes before and after one path in that order. */
export const neighbours = (path) => {
  const order = reading()
  const index = order.findIndex((entry) => entry.path === path)
  return {
    previous: index > 0 ? order[index - 1] : null,
    next: index >= 0 && index < order.length - 1 ? order[index + 1] : null,
  }
}

/** The current path without its trailing slash, so `/docs/tac/` matches. */
export const currentPath = () => location.pathname.replace(/(.)\/+$/, '$1')

/** The last segment of the current path, which is a guide slug or feature id. */
export const currentSlug = () => currentPath().split('/').filter(Boolean).pop() ?? ''
