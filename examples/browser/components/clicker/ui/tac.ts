export default class extends Tac {
    $clicks: number = 0
    label: string = 'Nested Clicker'
    @subscribe
    release?: string
}
