import type { JwtPayload } from 'jwt-decode'
import type { ContentType, MailTarget, ContentNodeTemplate } from './models.d'
import type { ContentEntrySerializer, MediaSerializer } from './serialized.d'

export {}

declare global {
    type Role = import('./Role').Role
    type AuthRole = import('./models.d').AuthRole
    type AuthUser = import('./serialized.d').AuthUserSerializer
    type Configuration = import('./models.d').Configuration
    type ContentAuthor = import('./models.d').ContentAuthor
    type CommentExt = import('./models.d').Comment
    type ContentCategory = import('./serialized.d').ContentCategorySerializer
    type Locale = import('./models.d').Locale
    type GroupMember = import('./serialized.d').GroupMemberSerializer
    type Variants = import('./serialized.d').MediaVariantSerializer
    type Category = import('./serialized.d').ContentCategorySerializer
    type Tag = import('./serialized.d').ContentTagSerializer
    type SSEMessage = import('./sse.d').SSEMessage
    type RespondObj = import('./query.d').RespondObj
    type ContentNodeSerializer = import('./serialized.d').ContentNodeSerializer
    type NodeSerializer = import('./serialized.d').NodeSerializer

    interface alertMessage {
        text: string
        variance: string
        seconds: number
    }

    interface Content extends ContentEntrySerializer {
        check: boolean
    }

    interface ContentTypeExt extends ContentType {
        active?: boolean
        check?: boolean
        field?: string
        icon?: string
    }

    interface MailTargetExt extends MailTarget {
        check?: boolean
    }

    interface NodeTemplateExt extends ContentNodeTemplate {
        check?: boolean
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

    interface TableRow {
        check?: boolean
        active: boolean
        up: boolean, name: string
        field: string,
        minWidth?: number
    }

    declare namespace Intl {
        type Key = 'calendar' | 'collation' | 'currency' | 'numberingSystem' | 'timeZone' | 'unit'

        function supportedValuesOf(input: Key): string[]
    }
}
