import { readFile } from 'node:fs/promises'

const bundle = await readFile(new URL('../dist/index.js', import.meta.url), 'utf8')

if (/\bprocess\s*(?:\.|\[)/u.test(bundle)) {
    throw new Error('The browser bundle contains an unresolved Node.js process reference.')
}

if (!bundle.includes('nur-cms-vue-admin')) {
    throw new Error('The browser bundle does not register the expected Custom Element.')
}
