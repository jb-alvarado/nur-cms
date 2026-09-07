import { describe, expect, it } from 'vitest'
import { randomID, shortID } from './helper'

describe('shortID', () => {
    it('always returns a compact alphanumeric identifier', () => {
        for (let index = 0; index < 100; index++) {
            expect(shortID()).toMatch(/^[a-f0-9]{7}$/)
        }
    })

    it('creates a full-length upload identifier without undefined fragments', () => {
        expect(randomID()).toMatch(/^[a-f0-9]{32}$/)
    })
})
