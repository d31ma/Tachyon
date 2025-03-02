new Worker('./tests/worker.ts').postMessage(`./src/serve.ts`)

await Bun.sleep(1000)

const res = await fetch(`http://localhost:8080/`, {
    headers: {
        'Accept': 'text/html'
    }
})

for await (const chunk of res.body!) {
    
    console.log(new TextDecoder().decode(chunk))
}

// const ws = new WebSocket(`ws://localhost:8080`)

// ws.onopen = (ev) => {
//     ws.send("Ping")
// }

// ws.onmessage = (ev) => {
//     console.log(JSON.parse(ev.data))
// }