const url = new URL(window.location.href);

const ws = new WebSocket(`ws://localhost:9876${url.pathname}`);

ws.onopen = () => {
    console.log('HMR Enabled');
}

ws.onmessage = (event) => {
    console.log('HMR Update');
    window.location.reload()
}