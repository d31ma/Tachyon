// @ts-check

export default class extends Tac {
    /** @type {number} */
    intensity = 62
    /** @type {'calm' | 'surge'} */
    mood = 'calm'
    /** @type {number} */
    frame = 1
    /** @type {string} */
    message = 'Canvas ready: calm composition at 62% energy.'
    /** @type {string} */
    lastPaintIso = new Date().toISOString()
    /** @type {string} */
    lastPaintLabel = 'Painted just now'

    /** @param {string} value @returns {void} */
    setIntensity(value) {
        this.intensity = Math.max(5, Math.min(100, Number(value)))
        this.message = `Canvas updated: ${this.mood} composition at ${this.intensity}% energy.`
        this.redraw()
    }

    /** @param {'calm' | 'surge'} mood @returns {void} */
    chooseMood(mood) {
        this.mood = mood
        this.intensity = mood === 'surge' ? 88 : 38
        this.message = `${mood === 'surge' ? 'Surge' : 'Calm'} palette selected: ${this.intensity}% energy.`
        this.redraw()
    }

    /** @returns {void} */
    redraw() {
        this.frame += 1
        this.lastPaintIso = new Date().toISOString()
        this.lastPaintLabel = `Frame ${this.frame} painted`
        requestAnimationFrame(() => { this.paintCanvas() })
    }

    /** @returns {void} */
    paintCanvas() {
        const canvas = document.querySelector('#browser-studio canvas')
        if (!(canvas instanceof HTMLCanvasElement))
            return
        const context = canvas.getContext('2d')
        if (!context)
            return

        const width = canvas.width
        const height = canvas.height
        const energy = this.intensity / 100
        const primary = this.mood === 'surge' ? '#eb6a3c' : '#2257ad'
        const secondary = this.mood === 'surge' ? '#fbbf5f' : '#16a394'
        const background = context.createLinearGradient(0, 0, width, height)
        background.addColorStop(0, '#101c3d')
        background.addColorStop(1, this.mood === 'surge' ? '#3c1520' : '#071f26')
        context.fillStyle = background
        context.fillRect(0, 0, width, height)

        for (let index = 0; index < 6; index += 1) {
            const radius = 26 + (index * 18) + (energy * 48)
            const x = width * (0.15 + (index * 0.145))
            const wave = Math.sin((this.frame + index) * 0.8) * energy * 35
            const y = (height * 0.52) + wave
            context.beginPath()
            context.arc(x, y, radius, 0, Math.PI * 2)
            context.fillStyle = index % 2
                ? `${secondary}${Math.round(80 + energy * 70).toString(16)}`
                : `${primary}${Math.round(95 + energy * 70).toString(16)}`
            context.fill()
        }

        context.fillStyle = '#ffffff'
        context.font = '600 15px IBM Plex Sans, sans-serif'
        context.fillText(`${this.mood.toUpperCase()} / FRAME ${this.frame}`, 28, 38)
        context.fillStyle = 'rgba(255,255,255,0.65)'
        context.font = '14px IBM Plex Mono, monospace'
        context.fillText(`ENERGY ${this.intensity}%`, 28, height - 28)
    }

    @onMount
    prepareCanvas() {
        this.paintCanvas()
    }
}
