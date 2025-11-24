import type { JwtPayload } from 'jwt-decode'
import type { ContentType } from './models.d'
import type { ContentSerializer, MediaSerializer } from './serialized.d'

export {}

declare global {
    type Role = import('./Role').Role
    type AuthRole = import('./models.d').AuthRole
    type AuthUser = import('./serialized.d').AuthUserSerializer
    type ContentAuthor = import('./models.d').ContentAuthor
    type ContentCategory = import('./models.d').ContentCategorySerializer
    type Locale = import('./models.d').Locale
    type GroupMember = import('./serialized.d').GroupMemberSerializer
    type Variants = import('./serialized.d').MediaVariantSerializer
    type SSEMessage = import('./sse.d').SSEMessage
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

    interface ContentTypeExt extends ContentType {
        active?: boolean
        check: boolean
        field?: string
        icon?: string
    }

    interface JwtPayloadExt extends JwtPayload {
        id: number
        role: Role
    }

    interface Media extends MediaSerializer {
        check?: boolean
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
