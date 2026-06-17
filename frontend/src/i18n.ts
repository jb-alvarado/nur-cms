import { createI18n } from 'vue-i18n'

import de from './locales/de.ts'
import en from './locales/en.ts'

export const locales = [
    {
        code: 'de',
        language: 'de',
        name: 'Deutsch',
    },
    {
        code: 'en',
        language: 'en',
        name: 'English',
    },
]

const instance = createI18n({
    legacy: false,
    locale: 'en',
    messages: {
        de,
        en,
    },
})

export default instance

export const i18n = instance.global
