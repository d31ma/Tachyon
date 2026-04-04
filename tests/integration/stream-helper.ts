new Worker('./tests/integration/server-worker.ts').postMessage(`./src/cli/serve.ts`)

await Bun.sleep(1000)

const res = await fetch(`http://localhost:8080/`, {
    headers: {
        'Accept': 'text/html'
    }
})

for await (const chunk of res.body!) {
    
    console.log(new TextDecoder().decode(chunk))
}

export {}