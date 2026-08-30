import { SearchService } from '../../../../services/search.js'

@Controller
export class SearchController {
  static async GET(request) {
    // Handler Protocol v1 carries no query string, so the term is a dynamic
    // route segment and arrives in `parameters`.
    const query = String(request?.parameters?.query ?? '').trim().toLowerCase()
    return { results: await new SearchService().find(query), query }
  }
}
