// @ts-check
new Worker('./tests/integration/server-worker.js').postMessage(`./src/cli/serve.js`);
await Bun.sleep(1000);
const res = await fetch(`http://localhost:8080/`, {
    headers: {
        'Accept': 'text/html'
    }
});
if (!res.body) {
    throw new Error('Expected a streaming body from the test server');
}
for await (const chunk of res.body) {
    console.log(new TextDecoder().decode(chunk));
}
export {};
