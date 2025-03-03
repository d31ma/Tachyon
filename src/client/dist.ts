#!/usr/bin/env bun
import Router from "../router.js";
import Yon from "./yon.js";

const start = Date.now()

await Yon.createStaticRoutes()
 
for(const route in Router.reqRoutes) {

    if(route.includes('hmr')) continue

    const res = await Router.reqRoutes[route][`GET`]()

    await Bun.write(Bun.file(`${process.cwd()}/dist/${route}`), await res.text())
}

await Bun.write(Bun.file(`${process.cwd()}/dist/index.html`), await Bun.file(`${import.meta.dir}/prod.html`).text())

console.log(`Built in ${Date.now() - start}ms`)