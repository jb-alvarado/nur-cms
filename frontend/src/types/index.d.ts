import type { JwtPayload } from 'jwt-decode'
import type { ContentSerializer } from './serialized.d'

export {}

declare global {
    type Role = import('./Role').Role
    type AuthRole = import('./models.d').AuthRole
    type AuthUser = import('./serialized.d').AuthUserSerializer
    type RespondObj = import('./query.d').RespondObj

    interface alertMessage {
        text: string
        variance: string
        seconds: number
    }

    interface Content extends ContentSerializer {
        check: boolean
        body: any
    }

    interface JwtPayloadExt extends JwtPayload {
        id: number
        role: Role
    }

    interface Token {
        access: string
        refresh: string
    }

    declare namespace Intl {
        type Key = 'calendar' | 'collation' | 'currency' | 'numberingSystem' | 'timeZone' | 'unit'

        function supportedValuesOf(input: Key): string[]
    }
}
