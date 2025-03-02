import { $ } from 'bun'

declare var self: Worker;

self.onmessage = async (event: MessageEvent) => {
    await $`${event.data}`
};