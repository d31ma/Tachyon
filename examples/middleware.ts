import type { RequestContext } from "@vyckr/tachyon"

export default {

    async before(request: Request, context: RequestContext) {
        console.info(`[middleware] ${request.method} ${new URL(request.url).pathname} requestId=${context.requestId}`)

        // Example: block requests with a custom header
        // if (request.headers.get('x-blocked')) {
        //     return Response.json({ detail: "Blocked by middleware" }, { status: 403 })
        // }
    },

    async after(request: Request, response: Response, context: RequestContext) {
        // Example: add a custom response header
        // const headers = new Headers(response.headers)
        // headers.set('X-Request-Id', context.requestId)
        // return new Response(response.body, { status: response.status, headers })

        return response
    }
}
