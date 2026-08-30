// @ts-check

const handler = `@Controller
export class HealthController {
  static GET() { return { ok: true } }
  static POST(request) { return request.body }
}`

export default class {
  /** @param {HTMLElement} root */
  hydrate(root) {
    const sample = root.querySelector('[data-sample="handler"]')
    if (sample) sample.textContent = handler
  }
}
