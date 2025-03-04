import { test, beforeAll, expect, describe } from 'bun:test'

beforeAll(async () => {
    new Worker('./tests/worker.ts').postMessage(`./src/serve.ts`)
    await Bun.sleep(1000)
})

describe('/', () => {

    test('GET', async () => {
    
        const res = await fetch('http://localhost:8080')
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })

    test('POST', async () => {
    
        const res = await fetch('http://localhost:8080', {
            method: 'POST'
        })
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
    
    test('DELETE', async () => {
    
        const res = await fetch('http://localhost:8080', {
            method: 'DELETE'
        })
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
})


describe('/api', () => {

    test('GET', async () => {
    
        const res = await fetch('http://localhost:8080/api')
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
    
    test('POST', async () => {
    
        const res = await fetch('http://localhost:8080/api', {
            method: 'POST'
        })
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
    
    test('PUT', async () => {
    
        const res = await fetch('http://localhost:8080/api', {
            method: 'PUT'
        })
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
})


describe('/api/v2', () => {

    test('GET', async () => {
    
        const res = await fetch('http://localhost:8080/api/v2')
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
    
    test('DELETE', async () => {
    
        const res = await fetch('http://localhost:8080/api/v2', {
            method: 'DELETE'
        })
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
    
    test('PATCH', async () => {
    
        const res = await fetch('http://localhost:8080/api/v2/users', {
            method: 'PATCH'
        })
    
        console.log(await res.json())
    
        expect(res.status).toEqual(200)
    })
})