// @ts-check
/**
 * @typedef {{
 *   requestId: string
 *   ipAddress: string
 *   protocol: string
 *   host: string
 *   bearer?: { token: string, verified: boolean }
 * }} YonContext
 */

const middleware = {
    /**
     * @param {Request} request
     * @param {YonContext} context
     * @returns {Promise<Response | void>}
     */
    async before(request, context) {
        console.info(`[middleware] ${request.method} ${new URL(request.url).pathname} requestId=${context.requestId}`);
        // Example: block requests with a custom header
        // if (request.headers.get('x-blocked')) {
        //     return Response.json({ detail: "Blocked by middleware" }, { status: 403 })
        // }
    },
    /**
     * @param {Request} request
     * @param {Response} response
     * @param {YonContext} context
     * @returns {Promise<Response>}
     */
    async after(request, response, context) {
        // Example: add a custom response header
        // const headers = new Headers(response.headers)
        // headers.set('X-Request-Id', context.requestId)
        // return new Response(response.body, { status: response.status, headers })
        return response;
    }
};
export default middleware;
