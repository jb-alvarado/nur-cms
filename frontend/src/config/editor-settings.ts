export const entryEditorStatuses = ['draft', 'published', 'archived'] as const
export type EntryEditorStatus = (typeof entryEditorStatuses)[number]

export const entryEditorFieldDefinitions = [
    { id: 'title', label: 'entryEditor.title' },
    { id: 'slug', label: 'entryEditor.slug' },
    { id: 'author', label: 'entryEditor.author' },
    { id: 'tags', label: 'entryEditor.tags' },
    { id: 'category', label: 'entryEditor.category' },
    { id: 'start_time', label: 'entryEditor.startTime' },
    { id: 'end_time', label: 'entryEditor.endTime' },
    { id: 'status', label: 'entryEditor.status' },
    { id: 'delete', label: 'entryEditor.delete' },
] as const

export type EntryEditorField = (typeof entryEditorFieldDefinitions)[number]['id']

const entryEditorFieldSet = new Set<string>(entryEditorFieldDefinitions.map((field) => field.id))

export function isEntryEditorField(field: string): field is EntryEditorField {
    return entryEditorFieldSet.has(field)
}
