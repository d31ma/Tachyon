const assert = require('node:assert/strict')
const fs = require('node:fs')
const vm = require('node:vm')

class PythonHostOwnershipGate {
  static host(state) {
    const context = {
      addEventListener: (_name, listener) => {
        if (state.listenerFailure) throw new Error('listener setup failed')
        state.listener = listener
      },
      removeEventListener: (_name, listener) => {
        assert.equal(state.listener, listener)
        state.listener = null
      },
    }
    const source = fs.readFileSync(process.argv[2], 'utf8').split('\nconst taskAnswers =')[0]
    vm.runInNewContext(source.replaceAll('import.meta.url', "'file:///fixture/engine.js'") + '\nglobalThis.Host = TacPythonHost', context)
    return context.Host
  }

  static fixture(options = {}) {
    const state = { ...options, parentDestroyed: false, copies: [], shutdownCalls: 0, listener: null }
    const borrowed = name => ({
      copy: () => {
        assert.equal(state.parentDestroyed, false)
        if (state.copyFailure === name) throw new Error(`${name} copy failed`)
        const ownership = { name, destroys: 0 }
        state.copies.push(ownership)
        const method = value => {
          assert.equal(ownership.destroys, 0, 'released proxy used')
          if (name === 'invoke') return `answer:${value}`
          state.shutdownCalls++
          if (state.shutdownFailure) throw new Error('shutdown failed')
        }
        method.destroy = () => {
          ownership.destroys++
          assert.equal(ownership.destroys, 1, 'proxy destroyed twice')
        }
        return method
      },
    })
    const type = {
      connect: (emit, done, storage, http) => { state.hosts = { emit, done, storage, http } },
      invoke: borrowed('invoke'),
      shutdown: borrowed('shutdown'),
      destroy: () => {
        assert.equal(state.parentDestroyed, false, 'parent destroyed twice')
        state.parentDestroyed = true
      },
    }
    return { state, type, Host: PythonHostOwnershipGate.host(state) }
  }

  static ownedMethodsSurviveParentAndReleaseOnce() {
    const { state, type, Host } = PythonHostOwnershipGate.fixture()
    const hosts = { emit: () => {}, done: () => {}, storage: () => '{}', http: () => ({}) }
    const host = Host.retain(type, hosts)
    assert.deepEqual(state.hosts, hosts)
    assert.equal(state.parentDestroyed, true)
    assert.equal(host.exchange('request'), 'answer:request')
    state.listener({ persisted: true })
    assert.equal(host.exchange('retained'), 'answer:retained')
    state.listener({ persisted: false })
    host.dispose()
    assert.equal(state.shutdownCalls, 1)
    assert.equal(state.listener, null)
    assert.deepEqual(state.copies.map(copy => copy.destroys), [1, 1])
    assert.throws(() => host.exchange('disposed'), /disposed/)
  }

  static incompleteSetupReleasesEveryOwnedProxy() {
    for (const options of [{ copyFailure: 'invoke' }, { copyFailure: 'shutdown' }, { listenerFailure: true }]) {
      const { state, type, Host } = PythonHostOwnershipGate.fixture(options)
      assert.throws(() => Host.retain(type), /failed/)
      assert.equal(state.parentDestroyed, true)
      assert.ok(state.copies.every(copy => copy.destroys === 1))
      assert.equal(state.listener, null)
    }
  }

  static shutdownFailureStillReleasesBothMethods() {
    const { state, type, Host } = PythonHostOwnershipGate.fixture({ shutdownFailure: true })
    const host = Host.retain(type)
    assert.throws(() => host.dispose(), /shutdown failed/)
    host.dispose()
    assert.equal(state.shutdownCalls, 1)
    assert.deepEqual(state.copies.map(copy => copy.destroys), [1, 1])
    assert.equal(state.listener, null)
  }

  static run() {
    PythonHostOwnershipGate.ownedMethodsSurviveParentAndReleaseOnce()
    PythonHostOwnershipGate.incompleteSetupReleasesEveryOwnedProxy()
    PythonHostOwnershipGate.shutdownFailureStillReleasesBothMethods()
    console.log('Owned method, partial setup, BFCache and disposal regressions passed')
  }
}

PythonHostOwnershipGate.run()
