self.onmessage = async (e) => {
    self.postMessage(await import(e.data));
}