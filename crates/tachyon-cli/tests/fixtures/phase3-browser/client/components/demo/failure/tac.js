export default class Failure {
  hydrate() {
    throw new Error('expected activation failure')
  }
}
