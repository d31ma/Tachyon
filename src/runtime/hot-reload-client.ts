const HMR_RECONNECT_MS = 3000

function connectHMR() {
    fetch('/hmr').then(async res => {

        for await(const _ of res.body!) {
            window.location.reload()
        }

    }).catch(() => {
        setTimeout(connectHMR, HMR_RECONNECT_MS)
    })
}

connectHMR()