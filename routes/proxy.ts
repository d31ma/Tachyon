export default async function middleware(request: Request, next: (request: Request) => Promise<Response>) {

    console.log("Within Proxy")

    const response = await next(request)

    return response
}