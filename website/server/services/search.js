// @ts-nocheck -- validated by the Tachyon compiler and website contract gates.
import { Fylo } from '../vendor/fylo.mjs'

// The searching itself, so the route stays a route. A controller answers HTTP
// methods and nothing else — a helper on it is never dispatched, which is why
// `score` lives here rather than beside `GET`.
@Service
export class SearchService {
  /** Where the site's document store lives on the serving host. */
  static ROOT = process.env.TAC_SITE_STORE ?? '/Volumes/DATA/tachyon-site'

  /** A page of results; the search box shows them all at once. */
  static MAX_RESULTS = 12

  /** Ranks a row against the query: a title hit beats a body hit. */
  static score(row, needle) {
    const title = String(row.title ?? '').toLowerCase()
    const text = String(row.text ?? '').toLowerCase()
    if (title.includes(needle)) return title.startsWith(needle) ? 3 : 2
    return text.includes(needle) ? 1 : 0
  }

  async find(query) {
    if (query.length < 2) return []
    const db = new Fylo(SearchService.ROOT)
    try {
      const rows = Object.values((await db.findDocs('search', {})) ?? {})
      return rows
        .map((row) => ({ row, rank: SearchService.score(row, query) }))
        .filter((hit) => hit.rank > 0)
        .sort((a, b) => b.rank - a.rank || a.row.title.length - b.row.title.length)
        .slice(0, SearchService.MAX_RESULTS)
        .map(({ row }) => ({ title: row.title, path: row.path, kind: row.kind }))
    } finally {
      await db.close()
    }
  }
}
