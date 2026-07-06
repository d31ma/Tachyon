// @ts-check
import { access, mkdir, writeFile } from 'fs/promises';
import path from 'path';

const PNG_SIGNATURE = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
const DESKTOP_ICON_INSET = 0.09;
/** @type {Array<[string, number]>} */
const MACOS_ICON_SIZES = [
    ['icon_16x16.png', 16],
    ['icon_16x16@2x.png', 32],
    ['icon_32x32.png', 32],
    ['icon_32x32@2x.png', 64],
    ['icon_128x128.png', 128],
    ['icon_128x128@2x.png', 256],
    ['icon_256x256.png', 256],
    ['icon_256x256@2x.png', 512],
    ['icon_512x512.png', 512],
    ['icon_512x512@2x.png', 1024],
];
const IOS_APP_ICON_SIZES = [
    { idiom: 'iphone', size: 20, scales: [2, 3] },
    { idiom: 'iphone', size: 29, scales: [2, 3] },
    { idiom: 'iphone', size: 40, scales: [2, 3] },
    { idiom: 'iphone', size: 60, scales: [2, 3] },
    { idiom: 'ipad', size: 20, scales: [1, 2] },
    { idiom: 'ipad', size: 29, scales: [1, 2] },
    { idiom: 'ipad', size: 40, scales: [1, 2] },
    { idiom: 'ipad', size: 76, scales: [1, 2] },
    { idiom: 'ipad', size: 83.5, scales: [2] },
];

/**
 * Writes Tachyon's native app icon assets without adding an image dependency.
 * The SVG is the design source; PNG/ICO outputs are deterministic raster
 * derivatives for platforms that require bitmap icon payloads.
 */
export default class NativeIconAssets {
    /** @param {{ outputRoot: string, resourcesDir: string }} options */
    constructor(options) {
        this.outputRoot = options.outputRoot;
        this.resourcesDir = options.resourcesDir;
    }

    /** @returns {Promise<void>} */
    async write() {
        await mkdir(path.join(this.resourcesDir, 'shared', 'assets'), { recursive: true });
        await this.writeDefaultSvgIfMissing('app-icon.svg');
        await this.writeDefaultSvgIfMissing('favicon.svg');
        await writeFile(path.join(this.resourcesDir, 'TachyonIcon.svg'), NativeIconAssets.svg());
        await writeFile(path.join(this.resourcesDir, 'TachyonIcon.png'), NativeIconAssets.png(512, { rounded: true }));
        await writeFile(path.join(this.resourcesDir, 'TachyonIcon.ico'), NativeIconAssets.ico([16, 32, 48, 256]));
        await this.writeMacOSIconset();
        await this.writeMacOSIcns();
        await this.writeIOSAppIconset();
        await this.writeAndroidIcon();
    }

    /** @param {string} filename */
    async writeDefaultSvgIfMissing(filename) {
        const target = path.join(this.resourcesDir, 'shared', 'assets', filename);
        try {
            await access(target);
        }
        catch {
            await writeFile(target, NativeIconAssets.svg());
        }
    }

    async writeMacOSIconset() {
        const iconsetDir = path.join(this.outputRoot, 'Icons', 'macos', 'TachyonIcon.iconset');
        await mkdir(iconsetDir, { recursive: true });
        for (const [filename, size] of MACOS_ICON_SIZES) {
            await writeFile(path.join(iconsetDir, filename), NativeIconAssets.png(size, { rounded: true }));
        }
    }

    async writeMacOSIcns() {
        const iconsetDir = path.join(this.outputRoot, 'Icons', 'macos', 'TachyonIcon.iconset');
        const target = path.join(this.resourcesDir, 'TachyonIcon.icns');
        if (process.platform === 'darwin') {
            const proc = Bun.spawn(['iconutil', '-c', 'icns', iconsetDir, '-o', target], {
                stdout: 'pipe',
                stderr: 'pipe',
            });
            if (await proc.exited === 0)
                return;
        }
        await writeFile(target, NativeIconAssets.icns());
    }

    async writeIOSAppIconset() {
        const appIconDir = path.join(this.outputRoot, 'Assets.xcassets', 'AppIcon.appiconset');
        await mkdir(appIconDir, { recursive: true });
        await writeFile(path.join(this.outputRoot, 'Assets.xcassets', 'Contents.json'), JSON.stringify({
            info: { author: 'tachyon', version: 1 },
        }, null, 2));
        /** @type {Array<Record<string, string>>} */
        const images = [];
        for (const entry of IOS_APP_ICON_SIZES) {
            for (const scale of entry.scales) {
                const pixels = Math.round(entry.size * scale);
                const filename = `tachyon-${entry.idiom}-${String(entry.size).replace('.', '-')}@${scale}x.png`;
                await writeFile(path.join(appIconDir, filename), NativeIconAssets.png(pixels, { rounded: false }));
                images.push({
                    idiom: entry.idiom,
                    size: `${entry.size}x${entry.size}`,
                    scale: `${scale}x`,
                    filename,
                });
            }
        }
        await writeFile(path.join(appIconDir, 'tachyon-ios-marketing@1x.png'), NativeIconAssets.png(1024, { rounded: false }));
        images.push({
            idiom: 'ios-marketing',
            size: '1024x1024',
            scale: '1x',
            filename: 'tachyon-ios-marketing@1x.png',
        });
        await writeFile(path.join(appIconDir, 'Contents.json'), JSON.stringify({
            images,
            info: { author: 'tachyon', version: 1 },
        }, null, 2));
    }

    async writeAndroidIcon() {
        // Adaptive icon: the background layer fills the launcher mask edge to
        // edge (a plain drawable icon gets shrunk inside the mask and looks
        // undersized next to other apps), the foreground carries the mark,
        // and the monochrome layer supports Android 13+ themed icons.
        const resDir = path.join(this.outputRoot, 'app', 'src', 'main', 'res');
        const drawableDir = path.join(resDir, 'drawable');
        const mipmapDir = path.join(resDir, 'mipmap-anydpi-v26');
        await mkdir(drawableDir, { recursive: true });
        await mkdir(mipmapDir, { recursive: true });
        await writeFile(path.join(drawableDir, 'ic_launcher_background.xml'), NativeIconAssets.androidAdaptiveBackground());
        await writeFile(path.join(drawableDir, 'ic_launcher_foreground.xml'), NativeIconAssets.androidAdaptiveForeground());
        await writeFile(path.join(drawableDir, 'ic_launcher_monochrome.xml'), NativeIconAssets.androidAdaptiveMonochrome());
        const adaptiveIcon = `<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@drawable/ic_launcher_background"/>
    <foreground android:drawable="@drawable/ic_launcher_foreground"/>
    <monochrome android:drawable="@drawable/ic_launcher_monochrome"/>
</adaptive-icon>
`;
        await writeFile(path.join(mipmapDir, 'ic_launcher.xml'), adaptiveIcon);
        await writeFile(path.join(mipmapDir, 'ic_launcher_round.xml'), adaptiveIcon);
    }

    /** @returns {string} */
    static svg() {
        return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" role="img" aria-label="Tachyon">
  <defs>
    <linearGradient id="tachyon-icon-gradient" x1="35" y1="35" x2="93" y2="93" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#0EA5E9"/>
      <stop offset="1" stop-color="#14B8A6"/>
    </linearGradient>
  </defs>
  <rect width="128" height="128" rx="31" fill="#0F172A"/>
  <path d="M37.5 46.5 L55 64 L37.5 81.5" fill="none" stroke="url(#tachyon-icon-gradient)" stroke-width="7" stroke-linecap="round" stroke-linejoin="round" opacity="0.4"/>
  <path d="M51 39.5 L75.5 64 L51 88.5" fill="none" stroke="url(#tachyon-icon-gradient)" stroke-width="8.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.75"/>
  <circle cx="88" cy="64" r="11.5" fill="url(#tachyon-icon-gradient)"/>
</svg>
`;
    }

    /** @returns {string} */
    static androidAdaptiveBackground() {
        return `<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="108dp"
    android:height="108dp"
    android:viewportWidth="128"
    android:viewportHeight="128">
    <path android:fillColor="#0F172A" android:pathData="M0,0L128,0L128,128L0,128Z"/>
</vector>
`;
    }

    /**
     * Mark-only layer. The launcher shows the central ~72/108dp of the
     * canvas; the mark spans the 66dp safe zone so it fills the visible
     * surface the way first-party icons do.
     * @param {{ monochrome?: boolean }} [options]
     * @returns {string}
     */
    static androidAdaptiveForeground(options = {}) {
        const chevronA = options.monochrome ? '#FFFFFF' : '#0EA5E9';
        const chevronB = options.monochrome ? '#FFFFFF' : '#12ADC4';
        const dot = options.monochrome ? '#FFFFFF' : '#14B8A6';
        return `<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="108dp"
    android:height="108dp"
    android:viewportWidth="128"
    android:viewportHeight="128">
    <path android:strokeColor="${chevronA}" android:strokeWidth="7" android:strokeLineCap="round" android:strokeLineJoin="round" android:strokeAlpha="0.4" android:fillColor="@android:color/transparent" android:pathData="M37.5,46.5L55,64L37.5,81.5"/>
    <path android:strokeColor="${chevronB}" android:strokeWidth="8.5" android:strokeLineCap="round" android:strokeLineJoin="round" android:strokeAlpha="0.75" android:fillColor="@android:color/transparent" android:pathData="M51,39.5L75.5,64L51,88.5"/>
    <path android:fillColor="${dot}" android:pathData="M88,52.5A11.5,11.5 0,1 1,87.99 52.5Z"/>
</vector>
`;
    }

    /** @returns {string} */
    static androidAdaptiveMonochrome() {
        return NativeIconAssets.androidAdaptiveForeground({ monochrome: true });
    }

    /** @param {number} size @param {{ rounded?: boolean, inset?: number }} [options] @returns {Uint8Array} */
    static png(size, options = {}) {
        const pixels = NativeIconAssets.raster(size, options);
        return NativeIconAssets.encodePng(size, size, pixels);
    }

    /** @param {number[]} sizes @returns {Uint8Array} */
    static ico(sizes) {
        const images = sizes.map((size) => ({ size, bytes: NativeIconAssets.png(size, { rounded: true }) }));
        const headerLength = 6 + images.length * 16;
        const total = headerLength + images.reduce((sum, image) => sum + image.bytes.length, 0);
        const out = new Uint8Array(total);
        const view = new DataView(out.buffer);
        view.setUint16(0, 0, true);
        view.setUint16(2, 1, true);
        view.setUint16(4, images.length, true);
        let offset = headerLength;
        images.forEach((image, index) => {
            const entry = 6 + index * 16;
            out[entry] = image.size >= 256 ? 0 : image.size;
            out[entry + 1] = image.size >= 256 ? 0 : image.size;
            out[entry + 2] = 0;
            out[entry + 3] = 0;
            view.setUint16(entry + 4, 1, true);
            view.setUint16(entry + 6, 32, true);
            view.setUint32(entry + 8, image.bytes.length, true);
            view.setUint32(entry + 12, offset, true);
            out.set(image.bytes, offset);
            offset += image.bytes.length;
        });
        return out;
    }

    /** @returns {Uint8Array} */
    static icns() {
        const chunks = [
            ['icp4', NativeIconAssets.png(16, { rounded: true })],
            ['icp5', NativeIconAssets.png(32, { rounded: true })],
            ['icp6', NativeIconAssets.png(64, { rounded: true })],
            ['ic07', NativeIconAssets.png(128, { rounded: true })],
            ['ic08', NativeIconAssets.png(256, { rounded: true })],
            ['ic09', NativeIconAssets.png(512, { rounded: true })],
            ['ic10', NativeIconAssets.png(1024, { rounded: true })],
        ].map(([type, data]) => NativeIconAssets.icnsChunk(String(type), /** @type {Uint8Array} */ (data)));
        const totalLength = 8 + chunks.reduce((sum, chunk) => sum + chunk.length, 0);
        const out = new Uint8Array(totalLength);
        const view = new DataView(out.buffer);
        out.set(new TextEncoder().encode('icns'), 0);
        view.setUint32(4, totalLength);
        let offset = 8;
        for (const chunk of chunks) {
            out.set(chunk, offset);
            offset += chunk.length;
        }
        return out;
    }

    /** @param {string} type @param {Uint8Array} data */
    static icnsChunk(type, data) {
        const out = new Uint8Array(8 + data.length);
        const view = new DataView(out.buffer);
        out.set(new TextEncoder().encode(type), 0);
        view.setUint32(4, out.length);
        out.set(data, 8);
        return out;
    }

    /** @param {number} size @param {{ rounded?: boolean, inset?: number }} [options] @returns {Uint8Array} */
    static raster(size, options = {}) {
        const pixels = new Uint8Array(size * size * 4);
        const rounded = options.rounded !== false;
        const iconInset = rounded ? options.inset ?? DESKTOP_ICON_INSET : 0;
        const iconScale = 1 - iconInset * 2;
        for (let y = 0; y < size; y += 1) {
            for (let x = 0; x < size; x += 1) {
                const index = (y * size + x) * 4;
                const nx = (x + 0.5) / size;
                const ny = (y + 0.5) / size;
                const localX = iconScale === 0 ? nx : (nx - iconInset) / iconScale;
                const localY = iconScale === 0 ? ny : (ny - iconInset) / iconScale;
                const inFrame = localX >= 0 && localX <= 1 && localY >= 0 && localY <= 1;
                const alpha = rounded && inFrame ? NativeIconAssets.roundedRectAlpha(localX, localY) : rounded ? 0 : 1;
                // The tachyon mark: gradient chevrons chasing a particle on
                // dark slate, mirroring shared/assets icon artwork.
                const sky = [14, 165, 233];
                const teal = [20, 184, 166];
                const slate = [15, 23, 42];
                const mix = Math.min(1, Math.max(0, ((localX + localY) / 2 - 0.27) / 0.46));
                const gradient = NativeIconAssets.mix(sky, teal, mix);
                let color = slate;
                const chevronA = Math.min(
                    NativeIconAssets.distanceToSegment(localX, localY, 0.293, 0.363, 0.43, 0.5),
                    NativeIconAssets.distanceToSegment(localX, localY, 0.43, 0.5, 0.293, 0.637),
                );
                if (chevronA < 0.0273)
                    color = NativeIconAssets.mix(slate, gradient, 0.4);
                const chevronB = Math.min(
                    NativeIconAssets.distanceToSegment(localX, localY, 0.398, 0.309, 0.59, 0.5),
                    NativeIconAssets.distanceToSegment(localX, localY, 0.59, 0.5, 0.398, 0.691),
                );
                if (chevronB < 0.0332)
                    color = NativeIconAssets.mix(slate, gradient, 0.75);
                if (NativeIconAssets.distance(localX, localY, 0.6875, 0.5) < 0.0898)
                    color = gradient;
                pixels[index] = color[0];
                pixels[index + 1] = color[1];
                pixels[index + 2] = color[2];
                pixels[index + 3] = Math.round(alpha * 255);
            }
        }
        return pixels;
    }

    /** @param {number} x @param {number} y */
    static roundedRectAlpha(x, y) {
        const radius = 0.242;
        const edge = 0.008;
        const inset = 0.5 - radius;
        const dx = Math.max(Math.abs(x - 0.5) - inset, 0);
        const dy = Math.max(Math.abs(y - 0.5) - inset, 0);
        const distance = Math.hypot(dx, dy);
        if (distance <= radius - edge)
            return 1;
        if (distance >= radius)
            return 0;
        return (radius - distance) / edge;
    }



    /** @param {number[]} left @param {number[]} right @param {number} amount */
    static mix(left, right, amount) {
        return left.map((value, index) => Math.round(value + (right[index] - value) * amount));
    }


    /** @param {number} ax @param {number} ay @param {number} bx @param {number} by */
    static distance(ax, ay, bx, by) {
        return Math.hypot(ax - bx, ay - by);
    }

    /** @param {number} px @param {number} py @param {number} ax @param {number} ay @param {number} bx @param {number} by */
    static distanceToSegment(px, py, ax, ay, bx, by) {
        const dx = bx - ax;
        const dy = by - ay;
        const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)));
        return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
    }

    /** @param {number} width @param {number} height @param {Uint8Array} pixels */
    static encodePng(width, height, pixels) {
        const scanlineLength = width * 4 + 1;
        const raw = new Uint8Array(scanlineLength * height);
        for (let y = 0; y < height; y += 1) {
            raw[y * scanlineLength] = 0;
            raw.set(pixels.subarray(y * width * 4, (y + 1) * width * 4), y * scanlineLength + 1);
        }
        return NativeIconAssets.concat([
            PNG_SIGNATURE,
            NativeIconAssets.chunk('IHDR', NativeIconAssets.ihdr(width, height)),
            NativeIconAssets.chunk('IDAT', NativeIconAssets.zlibDeflate(raw)),
            NativeIconAssets.chunk('IEND', new Uint8Array()),
        ]);
    }

    /** @param {Uint8Array} bytes */
    static zlibDeflate(bytes) {
        const compressed = Bun.deflateSync(bytes);
        if (NativeIconAssets.hasZlibHeader(compressed))
            return compressed;
        const checksum = NativeIconAssets.adler32(bytes);
        const trailer = new Uint8Array(4);
        new DataView(trailer.buffer).setUint32(0, checksum);
        return NativeIconAssets.concat([new Uint8Array([0x78, 0x01]), compressed, trailer]);
    }

    /** @param {Uint8Array} bytes */
    static hasZlibHeader(bytes) {
        if (bytes.length < 2)
            return false;
        const compressionMethod = bytes[0] & 0x0f;
        const header = (bytes[0] << 8) | bytes[1];
        return compressionMethod === 8 && header % 31 === 0;
    }

    /** @param {number} width @param {number} height */
    static ihdr(width, height) {
        const data = new Uint8Array(13);
        const view = new DataView(data.buffer);
        view.setUint32(0, width);
        view.setUint32(4, height);
        data[8] = 8;
        data[9] = 6;
        return data;
    }

    /** @param {string} type @param {Uint8Array} data */
    static chunk(type, data) {
        const typeBytes = new TextEncoder().encode(type);
        const out = new Uint8Array(12 + data.length);
        const view = new DataView(out.buffer);
        view.setUint32(0, data.length);
        out.set(typeBytes, 4);
        out.set(data, 8);
        view.setUint32(8 + data.length, NativeIconAssets.crc32(NativeIconAssets.concat([typeBytes, data])));
        return out;
    }

    /** @param {Uint8Array[]} parts */
    static concat(parts) {
        const total = parts.reduce((sum, part) => sum + part.length, 0);
        const out = new Uint8Array(total);
        let offset = 0;
        for (const part of parts) {
            out.set(part, offset);
            offset += part.length;
        }
        return out;
    }

    /** @param {Uint8Array} bytes */
    static crc32(bytes) {
        let crc = 0xffffffff;
        for (const byte of bytes) {
            crc ^= byte;
            for (let bit = 0; bit < 8; bit += 1)
                crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
        }
        return (crc ^ 0xffffffff) >>> 0;
    }

    /** @param {Uint8Array} bytes */
    static adler32(bytes) {
        let a = 1;
        let b = 0;
        for (const byte of bytes) {
            a = (a + byte) % 65521;
            b = (b + a) % 65521;
        }
        return ((b << 16) | a) >>> 0;
    }
}
