import type { InjectionKey } from 'vue'

export type GenericEditField =
    | 'title'
    | 'slug'
    | 'author'
    | 'tags'
    | 'category'
    | 'start_time'
    | 'end_time'
    | 'status'
    | 'delete'
export type GenericEditStatus = 'draft' | 'published' | 'archived'

export type GenericEditRouteConfig = {
    disabledFields?: GenericEditField[]
    defaultStatus?: GenericEditStatus
}

export type GenericEditConfig = Record<string, GenericEditRouteConfig>

export const genericEditConfigKey: InjectionKey<GenericEditConfig> = Symbol('genericEditConfig')
