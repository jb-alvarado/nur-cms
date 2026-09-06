const processingMessagePrefixes = [
    'Variants done: ',
    'Video variants done: ',
    'Video thumbnail done: ',
    'Variant generation failed: ',
    'Video processing failed: ',
    'Video thumbnail failed: ',
] as const

export function filenameFromProcessingMessage(message: string): string | undefined {
    const prefix = processingMessagePrefixes.find((candidate) => message.startsWith(candidate))
    const filename = prefix ? message.slice(prefix.length) : ''

    return filename || undefined
}
